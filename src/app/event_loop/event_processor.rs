//! Event Processing
//!
//! Handles processing of various event types from the event bus:
//! - UI events (menu requests, settings, exit)
//! - Audio events (sounds, music)
//! - Graphics events (time of day, lighting)
//! - Input events (mouse wheel, keyboard)

use crate::App;
use crate::input_event::InputEvent;
use moho_core::events::{AudioEvent, GraphicsEvent, UiEvent};
use winit::event_loop::ActiveEventLoop;

/// Handles processing of all event types
pub struct EventProcessor;

impl EventProcessor {
    /// Create a new event processor
    pub fn new() -> Self {
        Self
    }

    /// Process all pending UI events from the event bus
    pub fn process_ui_events(&self, app: &mut App, event_loop: &ActiveEventLoop) {
        while let Ok(event) = app.ui_event_rx.try_recv() {
            self.handle_ui_event(app, event_loop, event);
        }
    }

    /// Process a single UI event
    fn handle_ui_event(&self, app: &mut App, event_loop: &ActiveEventLoop, event: UiEvent) {
        match event {
            UiEvent::LoadSceneRequested { path } => {
                log::info!("UI requested load scene: {:?}", path);
                if let Err(e) = app.load_scene(&path) {
                    log::error!("Failed to load scene from {:?}: {}", path, e);
                }
            }
            UiEvent::NewWorldRequested { name, seed, size } => {
                log::info!(
                    "UI requested new world: {} (seed: {:?}, size: {})",
                    name,
                    seed,
                    size
                );
                let spec = moho_core::scene_builders::WorldSpec {
                    name,
                    seed,
                    size_xz: size,
                    day_length_seconds: 600.0,
                    night_length_seconds: 420.0,
                    initial_time_of_day: 6.0,
                };
                if let Err(e) = app.generate_new_world(spec) {
                    log::error!("Failed to generate new world: {}", e);
                }
            }
            UiEvent::ExitRequested => {
                log::info!("UI requested exit");
                // Auto-save before exit
                if let Err(e) = app.auto_save_on_shutdown() {
                    log::warn!("Failed to auto-save on exit: {}", e);
                }
                event_loop.exit();
            }
            UiEvent::MenuShown { name } => {
                log::info!("UI requested show menu: {}", name);
                if let Some(ui_adapter) = &app.ui_adapter
                    && let Ok(mut adapter) = ui_adapter.lock()
                {
                    adapter.show_menu(&name);
                }
            }
            UiEvent::MenuHidden { name } => {
                log::info!("Menu hidden: {}", name);
                // Handle console close event
                if name == "console" {
                    app.exit_console();
                }
            }
            UiEvent::OverlayToggled { name, visible } => {
                log::info!("Overlay {} toggled: {}", name, visible);
                if visible && let Some(ref wr) = app.window_renderer {
                    wr.window.set_cursor_visible(true);
                }
            }
            UiEvent::SettingsSaved => {
                log::info!("Settings saved");
                // Settings are already saved by the UI adapter
                // Here we could reload/apply them if needed
            }
        }
    }

    /// Process all pending audio events from the event bus
    pub fn process_audio_events(&self, app: &mut App) {
        while let Ok(event) = app.audio_event_rx.try_recv() {
            let audio_event = self.map_audio_event(event);
            app.handle_audio_event(audio_event);
        }
    }

    /// Map core AudioEvent to moho_audio AudioEvent
    fn map_audio_event(&self, event: AudioEvent) -> moho_audio::AudioEvent {
        match event {
            AudioEvent::ButtonClick => moho_audio::AudioEvent::ButtonClick,
            AudioEvent::MenuNavigate => moho_audio::AudioEvent::MenuNavigate,
            AudioEvent::Confirm => moho_audio::AudioEvent::Confirm,
            AudioEvent::Cancel => moho_audio::AudioEvent::Cancel,
            AudioEvent::Error => moho_audio::AudioEvent::Error,
            AudioEvent::PlaySound { path, volume } => {
                moho_audio::AudioEvent::CustomSound { path, volume }
            }
            AudioEvent::MusicStart {
                path,
                volume,
                looped,
            } => moho_audio::AudioEvent::BackgroundMusic {
                path,
                volume,
                looped,
            },
            AudioEvent::MusicStop => {
                moho_audio::AudioEvent::Stop(moho_audio::AudioCategory::Music)
            }
            AudioEvent::MusicVolumeChanged { volume: _ } => {
                // Skip - not implemented in current audio system
                // Return a no-op that won't crash but also won't do anything
                moho_audio::AudioEvent::Stop(moho_audio::AudioCategory::Music)
            }
            AudioEvent::StopAll => moho_audio::AudioEvent::Stop(moho_audio::AudioCategory::All),
        }
    }

    /// Process all pending graphics events from the event bus
    pub fn process_graphics_events(&self, app: &mut App) {
        while let Ok(event) = app.graphics_event_rx.try_recv() {
            match event {
                GraphicsEvent::TimeOfDayChanged { time, .. } => {
                    // Set the game clock time directly (time is in hours 0-24)
                    app.simulation.set_time_of_day(time);
                    log::info!(
                        "Time set to {:.2} ({})",
                        time,
                        app.simulation.game_clock().time_string()
                    );
                }
                _ => {
                    // Other graphics events not yet handled
                }
            }
        }
    }

    /// Process all pending input events (mouse wheel, etc.)
    pub fn process_input_events(&self, app: &mut App) {
        // Collect all events first to avoid borrow checker issues
        let events: Vec<InputEvent> = if let Some(rx) = &app.unconsumed_input_rx {
            rx.try_iter().collect()
        } else {
            Vec::new()
        };

        // Now process the collected events
        for iev in events {
            self.handle_input_event(app, iev);
        }
    }

    /// Handle a single input event
    fn handle_input_event(&self, app: &mut App, event: InputEvent) {
        match event {
            InputEvent::MouseWheel { delta_y } => {
                // Only act on wheel events in game mode
                if app.game_state == crate::game_state::GameState::Playing {
                    // Simple zoom: move player forward/back along look direction
                    let dz = delta_y * 0.5; // tuning factor
                    let (yaw, pitch) = app.simulation.yaw_pitch();
                    let sy = yaw.sin();
                    let cy = yaw.cos();
                    let cp = pitch.cos();
                    let sp = pitch.sin();
                    let forward =
                        glam::Vec3::new(sy * cp, sp, cy * cp).normalize_or_zero();
                    let new_pos = app.simulation.position() + forward * dz;
                    app.simulation.set_position_yaw_pitch(new_pos, yaw, pitch);
                }
            }
        }
    }

    /// Check and propagate generation cancellation from UI
    pub fn check_generation_cancel(&self, app: &mut App) {
        if let Some(ui_adapter) = &app.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
            && a.take_progress_canceled()
            && let Some(cancel_flag) = &app.generation_cancel
        {
            cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_processor_creation() {
        let processor = EventProcessor::new();
        assert_eq!(std::mem::size_of_val(&processor), 0); // Zero-sized type
    }

    #[test]
    fn test_event_processor_default() {
        let _processor = EventProcessor::default();
        // Just verify it compiles and constructs
    }

    #[test]
    fn test_audio_event_mapping() {
        let processor = EventProcessor::new();
        
        // Test basic event mappings
        let mapped = processor.map_audio_event(AudioEvent::ButtonClick);
        assert!(matches!(mapped, moho_audio::AudioEvent::ButtonClick));

        let mapped = processor.map_audio_event(AudioEvent::Confirm);
        assert!(matches!(mapped, moho_audio::AudioEvent::Confirm));

        let mapped = processor.map_audio_event(AudioEvent::Cancel);
        assert!(matches!(mapped, moho_audio::AudioEvent::Cancel));
    }

    #[test]
    fn test_audio_event_play_sound_mapping() {
        let processor = EventProcessor::new();
        
        let mapped = processor.map_audio_event(AudioEvent::PlaySound {
            path: "test.wav".into(),
            volume: 0.5,
        });
        
        match mapped {
            moho_audio::AudioEvent::CustomSound { path, volume } => {
                assert_eq!(path, "test.wav");
                assert!((volume - 0.5).abs() < 0.001);
            }
            _ => panic!("Expected CustomSound variant"),
        }
    }

    #[test]
    fn test_audio_event_music_mapping() {
        let processor = EventProcessor::new();
        
        let mapped = processor.map_audio_event(AudioEvent::MusicStart {
            path: "music.ogg".into(),
            volume: 0.8,
            looped: true,
        });
        
        match mapped {
            moho_audio::AudioEvent::BackgroundMusic { path, volume, looped } => {
                assert_eq!(path, "music.ogg");
                assert!((volume - 0.8).abs() < 0.001);
                assert!(looped);
            }
            _ => panic!("Expected BackgroundMusic variant"),
        }
    }
}
