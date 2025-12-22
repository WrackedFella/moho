//! Event routing and action processing for the UI adapter
//!
//! This module handles:
//! - Menu action to event bus conversion
//! - Audio event emission
//! - Console action processing
use crate::UiAudioEvent;
use crate::screens::MenuAction;
use moho_core::EventBus;

/// Process a menu action and publish corresponding events to the event bus
pub fn process_menu_action(action: &MenuAction, event_bus: &EventBus) {
    use moho_core::events::UiEvent as CoreUiEvent;

    match action {
        MenuAction::LoadScene(path) => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            event_bus.publish(CoreUiEvent::LoadSceneRequested { path: path.clone() });
        }
        MenuAction::NewWorld => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            // Signal to open the new_world menu
            event_bus.publish(CoreUiEvent::MenuShown {
                name: "new_world".to_string(),
            });
        }
        MenuAction::GenerateWorld(spec) => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            event_bus.publish(CoreUiEvent::NewWorldRequested {
                name: "New World".to_string(),
                seed: spec.seed,
                size: spec.size_xz,
            });
        }
        MenuAction::Exit => {
            emit_audio_event(event_bus, UiAudioEvent::ButtonClick);
            event_bus.publish(CoreUiEvent::ExitRequested);
        }
        MenuAction::ShowMenu(name) => {
            emit_audio_event(event_bus, UiAudioEvent::MenuNavigate);
            event_bus.publish(CoreUiEvent::MenuShown { name: name.clone() });
        }
        MenuAction::Close => {
            emit_audio_event(event_bus, UiAudioEvent::Cancel);
            // Note: Menu hiding is handled by adapter, not through event bus
        }
        MenuAction::SettingsSaved(_prefs) => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            event_bus.publish(CoreUiEvent::SettingsSaved);
        }
        MenuAction::None => {
            // No action
        }
    }
}

/// Emit a UI audio event to the event bus
pub fn emit_audio_event(event_bus: &EventBus, audio_event: UiAudioEvent) {
    use moho_core::events::AudioEvent;

    let core_event = match audio_event {
        UiAudioEvent::ButtonClick => AudioEvent::ButtonClick,
        UiAudioEvent::MenuNavigate => AudioEvent::MenuNavigate,
        UiAudioEvent::Confirm => AudioEvent::Confirm,
        UiAudioEvent::Cancel => AudioEvent::Cancel,
        UiAudioEvent::Error => AudioEvent::Error,
    };

    event_bus.publish(core_event);
}

/// Process console action and publish corresponding events
pub fn process_console_action(action: crate::overlays::ConsoleAction, event_bus: &EventBus) {
    use crate::overlays::ConsoleAction;

    match action {
        ConsoleAction::Close => {
            use moho_core::events::UiEvent;
            event_bus.publish(UiEvent::MenuHidden {
                name: "console".to_string(),
            });
        }
        ConsoleAction::Quit => {
            use moho_core::events::UiEvent;
            event_bus.publish(UiEvent::ExitRequested);
        }
        ConsoleAction::ToggleGodMode => {
            use moho_core::events::DebugEvent;
            event_bus.publish(DebugEvent::ToggleGodMode { enabled: true });
        }
        ConsoleAction::ToggleNoclip => {
            use moho_core::events::DebugEvent;
            event_bus.publish(DebugEvent::ToggleCollision { enabled: false });
        }
        ConsoleAction::SetSunDirection(yaw, pitch) => {
            use moho_core::events::GraphicsEvent;
            // Convert degrees to radians
            let yaw_rad = yaw.to_radians();
            let pitch_rad = pitch.to_radians();
            event_bus.publish(GraphicsEvent::SunDirectionChanged {
                yaw: yaw_rad,
                pitch: pitch_rad,
            });
        }
        ConsoleAction::SetTimeOfDay(time) => {
            use moho_core::events::GraphicsEvent;
            // Time is now in hours (0-24) and will be set directly on the game clock
            // The sun_angle field is kept for backward compatibility but not used
            event_bus.publish(GraphicsEvent::TimeOfDayChanged {
                time,
                sun_angle: 0.0,
            });
        }
        ConsoleAction::SetDebugView(mode) => {
            use moho_core::events::GraphicsEvent;
            event_bus.publish(GraphicsEvent::DebugViewChanged { mode });
        }
        ConsoleAction::None => {
            // No action
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_core::events::EventBus;
    use moho_core::scene_builders::WorldSpec;
    use std::sync::Arc;

    #[test]
    fn test_process_load_scene_action() {
        let bus = Arc::new(EventBus::new());
        let action = MenuAction::LoadScene(std::path::PathBuf::from("test.bin"));

        process_menu_action(&action, &bus);

        // Event bus doesn't provide a way to read published events in tests,
        // but we verify no panic occurs
    }

    #[test]
    fn test_process_new_world_action() {
        let bus = Arc::new(EventBus::new());
        let action = MenuAction::NewWorld;

        process_menu_action(&action, &bus);
        // Verify no panic
    }

    #[test]
    fn test_process_generate_world_action() {
        let bus = Arc::new(EventBus::new());
        let spec = WorldSpec {
            name: "Test World".to_string(),
            seed: Some(12345),
            size_xz: 16,
            day_length_seconds: 1200.0,
            night_length_seconds: 600.0,
            initial_time_of_day: 6.0,
        };
        let action = MenuAction::GenerateWorld(spec);

        process_menu_action(&action, &bus);
        // Verify no panic
    }

    #[test]
    fn test_process_exit_action() {
        let bus = Arc::new(EventBus::new());
        let action = MenuAction::Exit;

        process_menu_action(&action, &bus);
        // Verify no panic
    }

    #[test]
    fn test_process_show_menu_action() {
        let bus = Arc::new(EventBus::new());
        let action = MenuAction::ShowMenu("settings".to_string());

        process_menu_action(&action, &bus);
        // Verify no panic
    }

    #[test]
    fn test_emit_audio_events() {
        let bus = Arc::new(EventBus::new());

        emit_audio_event(&bus, UiAudioEvent::ButtonClick);
        emit_audio_event(&bus, UiAudioEvent::MenuNavigate);
        emit_audio_event(&bus, UiAudioEvent::Confirm);
        emit_audio_event(&bus, UiAudioEvent::Cancel);
        emit_audio_event(&bus, UiAudioEvent::Error);

        // Verify no panics
    }
}
