//! Generation Processing
//!
//! Handles async world generation polling and progress updates.

use crate::App;
use crate::GenerationMsg;

/// Handles async generation task polling
pub struct GenerationProcessor;

impl GenerationProcessor {
    /// Create a new generation processor
    pub fn new() -> Self {
        Self
    }

    /// Poll the async generation channel and process messages
    pub fn poll_generation(&self, app: &mut App) {
        // Take the receiver so we can mutate `self` while processing messages
        if let Some(rx) = app.generation_receiver.take() {
            let mut still_running = true;

            while let Ok(msg) = rx.try_recv() {
                match msg {
                    GenerationMsg::Progress(p) => {
                        self.handle_progress(app, p);
                    }
                    GenerationMsg::Completed { scene_bytes, spec } => {
                        self.handle_completed(app, scene_bytes, spec);
                        still_running = false;
                    }
                    GenerationMsg::Canceled => {
                        self.handle_canceled(app);
                        still_running = false;
                    }
                    GenerationMsg::Failed(reason) => {
                        self.handle_failed(app, reason);
                        still_running = false;
                    }
                }
            }

            if still_running {
                // Put the receiver back for future polling
                app.generation_receiver = Some(rx);
            } else {
                // Drop the receiver and clear state
                app.generation_receiver = None;
            }
        }
    }

    /// Handle generation progress update
    fn handle_progress(&self, app: &mut App, progress: f32) {
        if let Some(ui_adapter) = &app.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
        {
            a.set_progress(progress);
        }
    }

    /// Handle generation completion
    fn handle_completed(
        &self,
        app: &mut App,
        scene_bytes: Vec<u8>,
        spec: moho_core::scene_builders::WorldSpec,
    ) {
        log::info!("Generation completed for spec={:?}", spec.name);

        // Remember the last WorldSpec
        app.last_world_spec = Some(spec.clone());

        // Complete progress UI
        if let Some(ui_adapter) = &app.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
        {
            a.set_progress(1.0);
            a.finish_progress();
        }

        // Load produced scene bytes into the main world
        app.world.clear();
        match app.scene.load_from_bytes(&scene_bytes, &mut app.world) {
            Ok(camera_data) => {
                if let Some((position, yaw, pitch)) = camera_data {
                    app.simulation.set_position_yaw_pitch(position, yaw, pitch);
                    app.input_system.clear_pending_input();
                }
            }
            Err(e) => {
                log::error!("Failed to load generated scene bytes: {}", e);
            }
        }

        // Clean up generation state
        if let Some(h) = app.generation_handle.take() {
            let _ = h.join();
        }
        app.generation_cancel = None;

        // Switch to game mode and hide menu
        app.game_state = crate::game_state::GameState::Playing;
        app.hide_menu();
    }

    /// Handle generation cancellation
    fn handle_canceled(&self, app: &mut App) {
        if let Some(ui_adapter) = &app.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
        {
            a.finish_progress();
        }
        if let Some(h) = app.generation_handle.take() {
            let _ = h.join();
        }
        app.generation_cancel = None;
        log::info!("Generation canceled by user");
    }

    /// Handle generation failure
    fn handle_failed(&self, app: &mut App, reason: String) {
        log::error!("Generation failed: {}", reason);
        if let Some(ui_adapter) = &app.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
        {
            a.finish_progress();
        }
        if let Some(h) = app.generation_handle.take() {
            let _ = h.join();
        }
        app.generation_cancel = None;
    }
}

impl Default for GenerationProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_processor_creation() {
        let processor = GenerationProcessor::new();
        assert_eq!(std::mem::size_of_val(&processor), 0); // Zero-sized type
    }

    #[test]
    fn test_generation_processor_default() {
        let _processor = GenerationProcessor;
        // Just verify it compiles and constructs
    }
}
