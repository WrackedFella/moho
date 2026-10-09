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
        spec: moho_game::scene_builders::WorldSpec,
        terrain_config: moho_game::scene_builders::TerrainConfig,
        grid: moho_voxel::VoxelGrid,
    ) {
        tracing::info!(spec = ?spec.name, "Generation completed");

        app.generation.last_spec = Some(spec.clone());

        // Initialize LightSystem with the generated grid
        tracing::info!("Initializing LightSystem with generated grid");
        app.light_system = Some(moho_voxel::LightSystem::with_default_budget(
            grid,
            app.event_bus.clone(),
        ));

        // Clear stale chunk files so saved data from a previous world cannot
        // override freshly generated terrain.
        if let Err(e) = crate::save::clear_chunk_files(&spec.name) {
            tracing::warn!(world = %spec.name, error = %e, "Failed to clear chunk files");
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
        let streaming = moho_voxel::StreamingConfig {
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
        crate::app::world_geometry::remove_all_chunk_meshes(app);
        app.entities.clear();
        app.physics.reset();
        match moho_game::scene_persistence::load_from_bytes(&scene_bytes, &mut app.entities) {
            Ok((camera_data, _lights)) => {
                crate::app::world_geometry::upsert_all_chunks(app);
                // Generated worlds have no pre-spawned lights; nothing to restore.
                if let Some((position, yaw, pitch)) = camera_data {
                    app.simulation.set_position_yaw_pitch(position, yaw, pitch);
                    app.input.actions.reset_look();
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to load generated scene bytes");
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
        terrain_config: &moho_game::scene_builders::TerrainConfig,
        app: &mut App,
    ) -> Vec<glam::IVec3> {
        use moho_game::scene_builders::generate_chunk;
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
                        grid.mutator().place(world_pos, material_id, resource_id);
                    }
                    grid.clear_chunk_modified(pos);
                    loaded.push(pos);
                }
            }
        }
        tracing::info!(
            side = PRELOAD_RADIUS * 2 + 1,
            count = loaded.len(),
            "Preloaded spawn-area chunk columns (non-empty chunks)"
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
        tracing::info!("Generation canceled by user");
    }

    /// Set up physics world after a world is loaded or generated.
    ///
    /// `preloaded_chunks` are positions that were synchronously generated in
    /// `preload_spawn_area`. Their meshes are built here (before the character
    /// is placed) so real terrain colliders exist at spawn time. Passing an
    /// empty slice is fine for the load-scene path.
    ///
    /// Expects physics to be already reset and the loaded chunks registered
    /// by the loader.
    fn setup_physics_for_world(&self, app: &mut App, preloaded_chunks: &[glam::IVec3]) {
        // Build meshes and colliders for preloaded spawn-area chunks immediately so
        // the character doesn't fall through before async event processing kicks in.
        if let Some(ls) = &app.light_system {
            let grid = ls.grid();
            let chunks: Vec<_> = preloaded_chunks
                .iter()
                .map(|&pos| moho_voxel::VoxelChunk::from_grid_hybrid(grid, pos))
                .collect();
            for chunk in chunks {
                crate::app::world_geometry::insert_chunk(app, chunk);
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
                use moho_core::materials::MaterialType;
                use moho_game::actors::Sphere;
                let sphere = Sphere::new(
                    sphere_pos,
                    0.5,
                    MaterialType::Metal {
                        albedo: glam::Vec3::new(0.8, 0.5, 0.2),
                        fuzz: 0.05,
                    },
                );
                let actor = app.entities.actors.spawn_sphere(sphere);
                app.physics.test_bodies.push((handle, actor));
            }
        }

        tracing::info!(
            chunks = app.entities.chunks.len(),
            test_spheres = app.physics.test_bodies.len(),
            pos = ?spawn_pos,
            "Physics world ready"
        );
    }

    /// Handle generation failure
    fn handle_failed(&self, app: &mut App, reason: String) {
        tracing::error!(reason = %reason, "Generation failed");
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
    use crate::game_state::GameState;

    #[test]
    fn completed_generation_enters_playing_with_terrain_colliders() {
        let mut app = App::headless();
        let spec = moho_game::scene_builders::WorldSpec {
            name: "headless-test-completed".to_string(),
            seed: Some(7),
            size_xz: 64,
            day_length_seconds: 600.0,
            night_length_seconds: 420.0,
            initial_time_of_day: 6.0,
        };
        let scene_bytes = moho_game::scene_persistence::encode_to_bytes(
            &moho_game::scene::SceneEntities::default(),
            None,
            &[],
        )
        .expect("encode empty scene");

        GenerationProcessor::new().handle_completed(
            &mut app,
            scene_bytes,
            spec,
            moho_game::scene_builders::TerrainConfig::default(),
            moho_voxel::VoxelGrid::new(16),
        );

        assert_eq!(app.game_state, GameState::Playing);
        assert!(app.chunk_streamer.is_some());
        assert!(!app.entities.chunks.is_empty(), "spawn area is meshed");
        let pw = app.physics.world.as_ref().expect("physics world");
        for chunk in app.entities.chunks.iter() {
            let id = crate::app::world_geometry::chunk_mesh_id(chunk.chunk_pos());
            assert_eq!(
                pw.world_mesh_collider(id).is_some(),
                chunk.has_geometry(),
                "chunk {} has a collider iff it has geometry",
                chunk.chunk_pos()
            );
        }
        assert!(
            app.physics.is_kcc_active(),
            "the character controller is placed"
        );
        assert_eq!(app.entities.actors.spheres().len(), 3);
        assert_eq!(app.physics.test_bodies.len(), 3);
        let terrain_y = app
            .light_system
            .as_ref()
            .and_then(|ls| ls.grid().get_height(0, 0))
            .expect("spawn column has terrain");
        assert!(app.simulation.position().y > terrain_y as f32);
    }

    #[test]
    fn second_completed_generation_replaces_the_first_worlds_physics_bodies() {
        use crate::app::world_geometry::tests::terrain_collider_count;
        let mut app = App::headless();
        let complete = |app: &mut App, name: &str| {
            let spec = moho_game::scene_builders::WorldSpec {
                name: name.to_string(),
                seed: Some(7),
                size_xz: 64,
                day_length_seconds: 600.0,
                night_length_seconds: 420.0,
                initial_time_of_day: 6.0,
            };
            let scene_bytes = moho_game::scene_persistence::encode_to_bytes(
                &moho_game::scene::SceneEntities::default(),
                None,
                &[],
            )
            .expect("encode empty scene");
            GenerationProcessor::new().handle_completed(
                app,
                scene_bytes,
                spec,
                moho_game::scene_builders::TerrainConfig::default(),
                moho_voxel::VoxelGrid::new(16),
            );
        };
        complete(&mut app, "headless-test-twice-first");
        assert_eq!(app.physics.test_bodies.len(), 3);

        complete(&mut app, "headless-test-twice-second");

        let pw = app.physics.world.as_ref().expect("physics world");
        let with_geometry = app
            .entities
            .chunks
            .iter()
            .filter(|c| c.has_geometry())
            .count();
        assert_eq!(app.physics.test_bodies.len(), 3, "one world's test bodies");
        assert_eq!(app.entities.actors.spheres().len(), 3);
        assert_eq!(
            terrain_collider_count(&app),
            with_geometry + 3,
            "terrain plus the three spheres, character aside"
        );
        assert_eq!(
            pw.collider_set.len(),
            with_geometry + 1 + 3,
            "terrain, one character and three spheres"
        );
        assert_eq!(
            pw.rigid_body_set.len(),
            1 + 3,
            "one character, three spheres"
        );
    }

    #[test]
    fn completed_generation_replaces_the_previous_worlds_meshes_with_spawn_chunks() {
        use crate::app::world_geometry::tests::{RecordingBackend, chunk_at};
        let mut app = App::headless();
        let spec = moho_game::scene_builders::WorldSpec {
            name: "headless-test-completed-meshes".to_string(),
            seed: Some(7),
            size_xz: 64,
            day_length_seconds: 600.0,
            night_length_seconds: 420.0,
            initial_time_of_day: 6.0,
        };
        let scene_bytes = moho_game::scene_persistence::encode_to_bytes(
            &moho_game::scene::SceneEntities::default(),
            None,
            &[],
        )
        .expect("encode empty scene");
        crate::app::world_geometry::insert_chunk(&mut app, chunk_at(glam::IVec3::new(90, 0, 90)));
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        let old = backend.registered[0];
        assert_eq!(app.scene.world_meshes_mut().draws().count(), 1);

        GenerationProcessor::new().handle_completed(
            &mut app,
            scene_bytes,
            spec,
            moho_game::scene_builders::TerrainConfig::default(),
            moho_voxel::VoxelGrid::new(16),
        );
        app.scene.world_meshes_mut().flush(&mut backend);

        let with_geometry = app
            .entities
            .chunks
            .iter()
            .filter(|c| c.has_geometry())
            .count();
        let drawn: Vec<u32> = app
            .scene
            .world_meshes_mut()
            .draws()
            .map(|(h, _)| h)
            .collect();
        assert!(with_geometry > 0, "the spawn area has chunks with geometry");
        assert!(
            backend.unregistered.contains(&old),
            "the old mesh was freed"
        );
        assert!(!drawn.contains(&old), "the old world is no longer drawn");
        assert_eq!(
            drawn.len(),
            with_geometry,
            "one draw per chunk with geometry"
        );
    }
}
