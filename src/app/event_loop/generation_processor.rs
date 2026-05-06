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
                        terrain_config,
                        grid,
                    } => {
                        self.handle_completed(app, scene_bytes, spec, terrain_config, *grid);
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
        terrain_config: moho_core::scene_builders::TerrainConfig,
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

        // Clear stale chunk files so saved data from a previous world cannot
        // override freshly generated terrain.
        if let Err(e) = crate::save::clear_chunk_files(&spec.name) {
            log::warn!("Failed to clear chunk files for '{}': {}", spec.name, e);
        }

        // Pre-generate the spawn-column chunks synchronously so that get_height and
        // physics colliders are ready before the character controller is placed.
        let preloaded = self.preload_spawn_area(&terrain_config, app);
        // Also publish events so the streaming pipeline keeps them in sync.
        for pos in &preloaded {
            app.event_bus
                .publish(moho_core::events::WorldEvent::ChunkMeshDirty {
                    chunk_pos: *pos,
                    terrain_dirty: true,
                    structure_dirty: false,
                });
        }

        // Initialize ChunkStreamer for on-demand terrain loading
        let streaming = moho_core::voxel::StreamingConfig {
            load_radius_chunks: app.prefs.world_load_radius(),
            unload_radius_chunks: app.prefs.world_unload_radius(),
            chunks_per_frame: app.prefs.world_chunks_per_frame(),
        };
        app.chunk_streamer = Some(crate::app::chunk_streamer::ChunkStreamer::new(
            terrain_config,
            streaming,
            spec.name.clone(),
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

        // Initialize physics for new world, seeding it with the preloaded meshes
        // so the character spawns on real terrain rather than into void.
        self.setup_physics_for_world(app, &preloaded);

        // Switch to game mode and hide menu
        app.game_state = crate::game_state::GameState::Playing;
        app.hide_menu();
    }

    /// Pre-generate terrain for a small radius around the spawn point (chunk 0,0).
    /// This runs synchronously so that `get_height(0, 0)` returns a real surface
    /// height before `setup_physics_for_world` places the character controller.
    fn preload_spawn_area(
        &self,
        terrain_config: &moho_core::scene_builders::TerrainConfig,
        app: &mut App,
    ) -> Vec<glam::IVec3> {
        use moho_core::voxel::streaming::generate_chunk;
        const PRELOAD_RADIUS: i32 = 2; // 2-chunk radius (5×5 columns) around origin
        const MIN_Y: i32 = 0;
        const MAX_Y: i32 = 8; // matches chunk_streamer::MAX_CHUNK_Y

        let Some(ls) = app.light_system.as_mut() else {
            return vec![];
        };
        let grid = ls.grid_mut();

        let mut loaded = Vec::new();
        for cx in -PRELOAD_RADIUS..=PRELOAD_RADIUS {
            for cz in -PRELOAD_RADIUS..=PRELOAD_RADIUS {
                for cy in MIN_Y..=MAX_Y {
                    let pos = glam::IVec3::new(cx, cy, cz);
                    let blocks = generate_chunk(terrain_config, pos);
                    if blocks.is_empty() {
                        continue;
                    }
                    for (world_pos, material_id, resource_id) in blocks {
                        grid.place_block(world_pos, material_id, resource_id);
                    }
                    grid.clear_chunk_modified(pos);
                    loaded.push(pos);
                }
            }
        }
        log::info!(
            "Preloaded {}×{} spawn-area chunk columns ({} non-empty chunks)",
            PRELOAD_RADIUS * 2 + 1,
            PRELOAD_RADIUS * 2 + 1,
            loaded.len()
        );
        loaded
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
    ///
    /// `preloaded_chunks` are positions that were synchronously generated in
    /// `preload_spawn_area`. Their meshes are built here (before the character
    /// is placed) so real terrain colliders exist at spawn time. Passing an
    /// empty slice is fine for the load-scene path.
    fn setup_physics_for_world(&self, app: &mut App, preloaded_chunks: &[glam::IVec3]) {
        app.physics.reset();

        // Register terrain colliders from all ECS chunks (handles the load-scene path).
        app.physics.sync_colliders_from_ecs(&app.world);

        // Build meshes and colliders for preloaded spawn-area chunks immediately so
        // the character doesn't fall through before async event processing kicks in.
        if let Some(ls) = &app.light_system {
            let grid = ls.grid();
            for &pos in preloaded_chunks {
                let chunk = moho_core::voxel::VoxelChunk::from_grid_hybrid(grid, pos);
                app.physics
                    .update_chunk_collider(pos, chunk.vertices(), chunk.indices());
                app.world.push((chunk,));
            }
        }

        // Scan outward from the world origin to find any preloaded surface block.
        // Perlin returns exactly 0 at integer grid points, so (0,0) may land on a
        // zero-density boundary with no block; a small spiral search finds the real
        // surface. Fall back to 64 (BASE_ELEVATION) when preload produced nothing.
        let terrain_y = {
            let found = app.light_system.as_ref().and_then(|ls| {
                let grid = ls.grid();
                for r in 0..=4i32 {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx.abs() != r && dz.abs() != r {
                                continue;
                            }
                            if let Some(h) = grid.get_height(dx, dz) {
                                return Some(h);
                            }
                        }
                    }
                }
                None
            });
            found.unwrap_or(64)
        } as f32;

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
