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
        if let Some(rx) = app.generation.receiver.take() {
            let mut still_running = true;

            while let Ok(msg) = rx.try_recv() {
                match msg {
                    GenerationMsg::Progress(p) => {
                        self.handle_progress(app, p);
                    }
                    GenerationMsg::Completed {
                        scene_bytes,
                        spec,
                        grid,
                    } => {
                        self.handle_completed(app, scene_bytes, spec, grid);
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
                app.generation.receiver = Some(rx);
            } else {
                app.generation.receiver = None;
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
        grid: moho_core::voxel::VoxelGrid,
    ) {
        log::info!("Generation completed for spec={:?}", spec.name);

        app.generation.last_spec = Some(spec.clone());

        // Initialize LightSystem with the generated grid
        log::info!("Initializing LightSystem with generated grid");
        app.light_system = Some(moho_core::voxel::LightSystem::with_default_budget(
            grid,
            app.event_bus.clone(),
        ));

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
            Ok((camera_data, _lights)) => {
                // Generated worlds have no pre-spawned lights; nothing to restore.
                if let Some((position, yaw, pitch)) = camera_data {
                    app.simulation.set_position_yaw_pitch(position, yaw, pitch);
                    app.input.system.clear_pending_input();
                }
            }
            Err(e) => {
                log::error!("Failed to load generated scene bytes: {}", e);
            }
        }

        // Clean up generation state
        if let Some(h) = app.generation.handle.take() {
            let _ = h.join();
        }
        app.generation.cancel = None;

        // Initialize physics for new world
        self.setup_physics_for_world(app);

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
        if let Some(h) = app.generation.handle.take() {
            let _ = h.join();
        }
        app.generation.cancel = None;
        log::info!("Generation canceled by user");
    }

    /// Set up physics world after a world is loaded or generated.
    fn setup_physics_for_world(&self, app: &mut App) {
        app.physics.reset();

        // Register terrain colliders from all ECS chunks
        app.physics.sync_colliders_from_ecs(&app.world);

        // Terrain is generated in symmetric coords (-size/2 .. size/2), so origin is center.
        let terrain_y = app
            .light_system
            .as_ref()
            .and_then(|ls| ls.grid().get_height(0, 0))
            .unwrap_or(10) as f32;

        let spawn_pos = glam::Vec3::new(0.0, terrain_y + 3.0, 0.0);

        // Spawn character controller
        if let Some(ref mut pw) = app.physics.world {
            pw.add_character(spawn_pos);
        }

        // Sync simulation camera to spawn position
        let (yaw, pitch) = app.simulation.yaw_pitch();
        app.simulation.set_position_yaw_pitch(spawn_pos, yaw, pitch);

        // Spawn 3 test spheres above the center
        for i in 0..3 {
            let sphere_pos = glam::Vec3::new(0.0, terrain_y + 20.0, (i * 2) as f32);

            let body_handle = app
                .physics
                .world
                .as_mut()
                .map(|pw| pw.add_dynamic_sphere(sphere_pos, 0.5));

            if let Some(handle) = body_handle {
                use moho_core::actors::Sphere;
                use moho_core::materials::MaterialType;
                let sphere = Sphere::new(
                    sphere_pos,
                    0.5,
                    MaterialType::Metal {
                        albedo: glam::Vec3::new(0.8, 0.5, 0.2),
                        fuzz: 0.05,
                    },
                );
                let entity = app.world.push((sphere,));
                app.physics.test_bodies.push((handle, entity));
            }
        }

        log::info!(
            "Physics world ready: {} chunk colliders, {} test spheres, character at {:?}",
            app.physics.chunk_colliders.len(),
            app.physics.test_bodies.len(),
            spawn_pos
        );
    }

    /// Handle generation failure
    fn handle_failed(&self, app: &mut App, reason: String) {
        log::error!("Generation failed: {}", reason);
        if let Some(ui_adapter) = &app.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
        {
            a.finish_progress();
        }
        if let Some(h) = app.generation.handle.take() {
            let _ = h.join();
        }
        app.generation.cancel = None;
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
