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
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!(path = %path.display(), "Loading scene");

    // Check if the file exists
    if !path.exists() {
        return Err(format!("Scene file does not exist: {}", path.display()).into());
    }

    // Reset physics with the entities: a load that fails below must not leave
    // bodies tracking actors that no longer exist.
    app.entities.clear();
    app.physics.reset();

    // Load the scene from file with metadata support
    {
        let (spec, scene_bytes, block_records) = crate::save::read_scene_and_metadata(path)?;
        // Remember the WorldSpec from the loaded file so autosaves and
        // subsequent writes preserve the original metadata.
        app.generation.last_spec = Some(spec.clone());
        tracing::info!(spec = ?spec, "Loaded WorldSpec from save");
        // Restore time of day from the persisted WorldSpec.
        app.simulation.set_time_of_day(spec.initial_time_of_day);
        let (camera_data, lights) =
            moho_game::scene_persistence::load_from_bytes(&scene_bytes, &mut app.entities)?;
        tracing::info!(
            path = %path.display(),
            count = lights.len(),
            "Scene loaded successfully"
        );

        // Re-add persisted lights to the renderer
        if let Some(ref mut wr) = app.window_renderer {
            for desc in &lights {
                if desc.enabled {
                    wr.renderer.add_point_light(
                        glam::Vec3::from_array(desc.position),
                        glam::Vec3::from_array(desc.color),
                        desc.intensity,
                        desc.range,
                    );
                }
            }
        }

        // Reconstruct VoxelGrid from persisted block records, then initialize LightSystem.
        let mut grid = moho_core::voxel::VoxelGrid::new(16);
        if block_records.is_empty() {
            // KNOWN LIMITATION: v1 saves do not contain block data. Light propagation
            // will be inactive until the world is regenerated and saved in v2 format.
            tracing::warn!(
                "Save file contains no block data (v1 format). \
                 Light propagation disabled for this session. \
                 Regenerate the world to fix permanently."
            );
        } else {
            for record in &block_records {
                let pos = moho_core::voxel::BlockPos::new(record.x, record.y, record.z);
                grid.mutator()
                    .place(pos, record.material_id, record.resource_id);
            }
            tracing::info!(
                count = block_records.len(),
                "Reconstructed VoxelGrid blocks for LightSystem"
            );
        }
        app.light_system = Some(moho_core::voxel::LightSystem::with_default_budget(
            grid,
            app.event_bus.clone(),
        ));

        // Initialize ChunkStreamer so the loaded world can stream additional chunks
        let mut terrain_config = moho_game::scene_builders::TerrainConfig::default();
        if let Some(s) = spec.seed {
            terrain_config.seed = s as u32;
        }
        terrain_config.world_size = spec.size_xz;
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

        // Restore camera handled below using camera_data
        if let Some((position, yaw, pitch)) = camera_data {
            app.simulation.set_position_yaw_pitch(position, yaw, pitch);
            // Clear any pending input so the restored camera
            // orientation isn't immediately overridden by
            // accumulated mouse deltas or smoothing state.
            app.input.system.clear_pending_input();
            tracing::info!(pos = ?position, yaw, pitch, "Restored camera position");
        } else {
            tracing::info!("No camera data found in scene file, keeping current position");
        }
    }

    // Initialize physics for loaded world.
    app.setup_physics_for_loaded_world();

    // Request a redraw to show the loaded scene
    if let Some(ref wr) = app.window_renderer {
        wr.window.request_redraw();
    }

    // Switch to game mode and hide menu
    app.game_state = crate::game_state::GameState::Playing;
    app.hide_menu();

    tracing::info!("Scene loading complete - switched to game mode");
    Ok(())
}
