//! Scene loading — deserializes a saved scene file and reconstructs world state.
//!
//! This module contains the logic that was previously inlined in `App::load_scene`,
//! including entity reconstruction, light system initialization, chunk streamer setup,
//! camera restoration, and physics initialization.

/// Load a saved scene from `path` and fully reconstruct world state.
///
/// On success:
/// - `app.entities` is cleared and rebuilt from the scene bytes
/// - `app.light_system` is replaced with a fresh `LightSystem` from persisted blocks
/// - `app.chunk_streamer` is initialized for the loaded world's terrain config
/// - Camera position, yaw, and pitch are restored from the save file
/// - Physics colliders are reset and repopulated
/// - Game state transitions to `Playing` and the menu is hidden
#[allow(dead_code)]
pub fn load_scene(
    app: &mut crate::App,
    clock: &mut moho_app::GameClock,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!(path = %path.display(), "Loading scene");

    // Check if the file exists
    if !path.exists() {
        return Err(format!("Scene file does not exist: {}", path.display()).into());
    }

    // Read and validate the save before touching the running world, so a
    // foreign, unsupported or corrupt file leaves it as it was.
    let (spec, scene_bytes, block_records) = crate::save::read_scene_and_metadata(path)?;

    // Reset physics with the entities: a load that fails below must not leave
    // bodies tracking actors that no longer exist.
    crate::app::world_geometry::remove_all_chunk_meshes(app);
    app.entities.clear();
    app.physics.reset();

    {
        // Decode the scene before applying the spec, so a bad scene leaves the spec untouched.
        let (camera_data, lights) =
            flagstone_game::scene_persistence::load_from_bytes(&scene_bytes, &mut app.entities)?;
        crate::app::world_geometry::upsert_all_chunks(app);
        // Remember the WorldSpec from the loaded file so autosaves and
        // subsequent writes preserve the original metadata.
        app.generation.last_spec = Some(spec.clone());
        tracing::info!(spec = ?spec, "Loaded WorldSpec from save");
        // Restore time of day from the persisted WorldSpec.
        clock.reset_to(spec.initial_time_of_day);
        tracing::info!(
            path = %path.display(),
            count = lights.len(),
            "Scene loaded successfully"
        );

        // Re-add persisted lights to the renderer
        if app.window.is_some() {
            for desc in lights.iter().filter(|desc| desc.enabled) {
                app.pending_render
                    .push(crate::RenderRequest::AddPointLight {
                        position: glam::Vec3::from_array(desc.position),
                        color: glam::Vec3::from_array(desc.color),
                        intensity: desc.intensity,
                        range: desc.range,
                    });
            }
        }

        // Reconstruct VoxelGrid from persisted block records, then initialize LightSystem.
        let mut grid = flagstone_voxel::VoxelGrid::new(16);
        if block_records.is_empty() {
            // Without block records there is nothing to light; the light system stays off.
            tracing::warn!(
                "Save file contains no block data. \
                 Light propagation disabled for this session. \
                 Regenerate the world to fix permanently."
            );
        } else {
            for record in &block_records {
                let pos = flagstone_voxel::BlockPos::new(record.x, record.y, record.z);
                grid.mutator()
                    .place(pos, record.material_id, record.resource_id);
            }
            tracing::info!(
                count = block_records.len(),
                "Reconstructed VoxelGrid blocks for LightSystem"
            );
        }
        let mut light_system =
            flagstone_voxel::LightSystem::with_default_budget(grid, app.event_bus.clone());
        // Lit before the first tick; the dirty events re-mesh the scene from the lit grid.
        let loaded: Vec<glam::IVec3> = light_system.grid().chunk_positions().collect();
        light_system.light_arrived_chunks(&loaded);
        light_system.emit_dirty_events();
        app.light_system = Some(light_system);

        // Initialize ChunkStreamer so the loaded world can stream additional chunks
        let mut terrain_config = flagstone_game::scene_builders::TerrainConfig::default();
        if let Some(s) = spec.seed {
            terrain_config.seed = s as u32;
        }
        terrain_config.world_size = spec.size_xz;
        let streaming = flagstone_voxel::StreamingConfig {
            load_radius_chunks: app.prefs.world_load_radius(),
            unload_radius_chunks: app.prefs.world_unload_radius(),
            chunks_per_frame: app.prefs.world_chunks_per_frame(),
        };
        app.chunk_streamer = Some(crate::app::chunk_streamer::ChunkStreamer::new(
            terrain_config,
            streaming,
            spec.name.clone(),
        ));

        // Restore camera handled below using camera_data
        if let Some((position, yaw, pitch)) = camera_data {
            app.simulation.set_position_yaw_pitch(position, yaw, pitch);
            // Clear any pending input so the restored camera
            // orientation isn't immediately overridden by
            // accumulated mouse deltas or smoothing state.
            app.input.actions.reset_look();
            tracing::info!(pos = ?position, yaw, pitch, "Restored camera position");
        } else {
            tracing::info!("No camera data found in scene file, keeping current position");
        }
    }

    // Initialize physics for loaded world.
    app.setup_physics_for_loaded_world();

    // Request a redraw to show the loaded scene
    if let Some(window) = &app.window {
        window.request_redraw();
    }

    // Switch to game mode and hide menu
    app.game_state = crate::game_state::GameState::Playing;
    app.hide_menu();

    tracing::info!("Scene loading complete - switched to game mode");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flagstone_game::scene_builders::{TerrainConfig, WorldSpec, generate_chunk};
    use flagstone_game::{TICK_HZ, controller::CameraMode};
    use flagstone_voxel::{BlockPos, VoxelChunk, VoxelGrid, WorldEvent};
    use moho_app::{HeadlessLoop, LoopConfig};

    // No chunk files exist under saves/ for these worlds, so streaming generates from seed.
    const WIDE_WORLD: &str = "scene_loader_unit_test_wide_world";
    const SMALL_WORLD: &str = "scene_loader_unit_test_small_world";

    fn spec(name: &str, size_xz: u32) -> WorldSpec {
        WorldSpec {
            name: name.to_string(),
            size_xz,
            ..WorldSpec::default()
        }
    }

    fn fill_column(grid: &mut VoxelGrid, column: (i32, i32), size_xz: u32) {
        let config = TerrainConfig {
            world_size: size_xz,
            ..TerrainConfig::default()
        };
        for cy in 0..=8 {
            for (pos, material, resource) in
                generate_chunk(&config, glam::IVec3::new(column.0, cy, column.1))
            {
                grid.mutator().place(pos, material, resource);
            }
        }
    }

    /// The terrain a world of `size_xz` has in `columns`, as the streamer would generate it.
    fn terrain(columns: &[(i32, i32)], size_xz: u32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for &column in columns {
            fill_column(&mut grid, column, size_xz);
        }
        grid
    }

    fn surface(grid: &VoxelGrid, x: i32, z: i32) -> f32 {
        grid.get_height(x, z)
            .unwrap_or_else(|| panic!("terrain at ({x}, {z})")) as f32
    }

    /// What a save leaves behind for the loader.
    struct Save {
        /// Terrain columns written as block records.
        block_columns: Vec<(i32, i32)>,
        /// Whether the chunk meshes of those columns are saved with the scene.
        meshes: bool,
        position: glam::Vec3,
        world: &'static str,
        size_xz: u32,
    }

    impl Save {
        fn write(&self) -> tempfile::TempDir {
            let temp = tempfile::tempdir().expect("temp dir");
            let mut saved = crate::App::headless();
            saved.generation.last_spec = Some(spec(self.world, self.size_xz));
            saved
                .simulation
                .set_position_yaw_pitch(self.position, 0.0, 0.0);
            let grid = saved
                .light_system
                .as_mut()
                .expect("App starts with a light system")
                .grid_mut();
            for &column in &self.block_columns {
                fill_column(grid, column, self.size_xz);
            }
            if self.meshes {
                let chunks: Vec<VoxelChunk> = grid
                    .chunk_positions()
                    .map(|pos| VoxelChunk::from_grid_hybrid(grid, pos))
                    .collect();
                for chunk in chunks {
                    saved.entities.chunks.insert(chunk);
                }
            }
            crate::app::autosave::auto_save_on_shutdown(
                &mut saved,
                &moho_app::GameClock::default(),
                temp.path(),
                &[],
            )
            .expect("autosave");
            temp
        }

        fn load(&self) -> (crate::App, tempfile::TempDir) {
            let temp = self.write();
            let mut app = crate::App::headless();
            load_scene(
                &mut app,
                &mut moho_app::GameClock::default(),
                &temp.path().join("scene.bin"),
            )
            .expect("load");
            (app, temp)
        }
    }

    fn wide_save(x: f32, z: f32, block_columns: Vec<(i32, i32)>, meshes: bool) -> Save {
        Save {
            block_columns,
            meshes,
            position: glam::Vec3::new(x, 200.0, z),
            world: WIDE_WORLD,
            size_xz: 256,
        }
    }

    fn assert_collider_under_pawn(app: &crate::App, column: (i32, i32), surface_y: f32) {
        let chunk = glam::IVec3::new(column.0, (surface_y as i32).div_euclid(16), column.1);
        assert!(
            app.physics
                .world
                .as_ref()
                .expect("physics world")
                .world_mesh_collider(crate::app::world_geometry::chunk_mesh_id(chunk))
                .is_some(),
            "terrain collider of chunk {chunk} exists when the pawn spawns"
        );
    }

    #[test]
    fn load_without_saved_meshes_puts_pawn_on_the_surface() {
        let (app, _temp) = wide_save(40.5, 40.5, vec![(2, 2)], false).load();

        let surface_y = surface(&terrain(&[(2, 2)], 256), 40, 40);
        let pos = app.simulation.position();
        assert_eq!(
            (pos.x, pos.y, pos.z),
            (40.5, surface_y + 3.0, 40.5),
            "the pawn spawns 3 above the saved column's surface"
        );
        assert_collider_under_pawn(&app, (2, 2), surface_y);
    }

    #[test]
    fn load_with_saved_meshes_puts_pawn_on_the_surface() {
        let (app, _temp) = wide_save(40.5, 40.5, vec![(2, 2)], true).load();

        let surface_y = surface(&terrain(&[(2, 2)], 256), 40, 40);
        let pos = app.simulation.position();
        assert_eq!(
            (pos.x, pos.y, pos.z),
            (40.5, surface_y + 3.0, 40.5),
            "the pawn spawns 3 above the saved column's surface"
        );
        assert_collider_under_pawn(&app, (2, 2), surface_y);
    }

    #[test]
    fn load_of_a_save_without_blocks_spawns_on_streamed_terrain() {
        let (app, _temp) = wide_save(40.5, 40.5, vec![], false).load();

        let surface_y = surface(&terrain(&[(2, 2)], 256), 40, 40);
        let pos = app.simulation.position();
        assert_eq!(
            (pos.x, pos.y, pos.z),
            (40.5, surface_y + 3.0, 40.5),
            "the pawn spawns on terrain streamed from the seed"
        );
        assert_collider_under_pawn(&app, (2, 2), surface_y);
    }

    #[test]
    fn loaded_pawn_stays_on_terrain_for_ten_seconds() {
        let (mut app, _temp) = wide_save(40.5, 40.5, vec![], false).load();
        app.simulation.set_camera_mode(CameraMode::FirstPerson);
        let surface_y = surface(&terrain(&[(2, 2)], 256), 40, 40);
        let mut sim = HeadlessLoop::<crate::App>::new(LoopConfig::new(TICK_HZ));
        let mut lowest = f32::MAX;

        for _ in 0..10 * TICK_HZ {
            sim.step(&mut app, 1);
            lowest = lowest.min(app.simulation.position().y);
        }

        let pos = app.simulation.position();
        assert!(
            lowest >= surface_y,
            "the pawn never sank below the surface {surface_y}: lowest {lowest}"
        );
        assert!(
            (pos.x - 40.5).abs() < 1.0 && (pos.z - 40.5).abs() < 1.0,
            "the pawn was not respawned elsewhere: {pos:?}"
        );
    }

    #[test]
    fn saved_position_maps_to_its_column_with_floor() {
        let reference = terrain(&[(-1, -1), (0, 0)], 256);
        let floor_surface = surface(&reference, -1, -1);
        assert_ne!(
            floor_surface,
            surface(&reference, 0, 0),
            "the two candidate columns differ in height"
        );

        let (app, _temp) = wide_save(-0.5, -0.5, vec![(-1, -1), (0, 0)], false).load();

        let pos = app.simulation.position();
        assert_eq!(
            (pos.x, pos.y, pos.z),
            (-0.5, floor_surface + 3.0, -0.5),
            "the pawn stands over block (-1, -1), not (0, 0)"
        );
    }

    #[test]
    fn load_with_no_terrain_keeps_the_saved_position() {
        let save = Save {
            block_columns: vec![],
            meshes: false,
            position: glam::Vec3::new(1000.0, 200.0, 1000.0),
            world: SMALL_WORLD,
            size_xz: 64,
        };

        let (app, _temp) = save.load();

        assert_eq!(app.simulation.position(), save.position);
    }

    #[test]
    fn loading_a_world_lights_it_before_the_first_tick() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut saved = crate::App::headless();
        let grid = saved
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        grid.mutator()
            .fill_region(BlockPos::new(0, 0, 0), BlockPos::new(15, 7, 15), 0);
        grid.mutator().place(BlockPos::new(5, 12, 5), 0, None);
        crate::app::autosave::auto_save_on_shutdown(
            &mut saved,
            &moho_app::GameClock::default(),
            temp.path(),
            &[],
        )
        .expect("autosave");
        let mut app = crate::App::headless();
        let _ = app.world_event_rx.try_iter().count();

        load_scene(
            &mut app,
            &mut moho_app::GameClock::default(),
            &temp.path().join("scene.bin"),
        )
        .expect("load");

        let grid = app.light_system.as_ref().expect("light system").grid();
        assert!(
            grid.sky_exposed_at(BlockPos::new(8, 10, 8)),
            "open air above the ground sees the sky"
        );
        assert!(
            !grid.sky_exposed_at(BlockPos::new(5, 10, 5)),
            "air under the roof block does not"
        );
        let dirtied: Vec<glam::IVec3> = app
            .world_event_rx
            .try_iter()
            .filter_map(|event| match event {
                WorldEvent::ChunkMeshDirty { chunk_pos, .. } => Some(chunk_pos),
                _ => None,
            })
            .collect();
        assert!(
            dirtied.contains(&glam::IVec3::ZERO),
            "the loaded chunk is queued for re-meshing: {dirtied:?}"
        );
    }
}
