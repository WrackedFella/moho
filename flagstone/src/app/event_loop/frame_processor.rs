//! Frame Processing
//!
//! Handles per-tick game state updates and per-frame lighting and HUD data.

use crate::App;
use crate::app::event_loop::event_processor::{lod_for_chunk, lod_player_chunk};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Frame counter for tracking frame numbers
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Walking speed in m/s.
const MOVE_SPEED: f32 = 4.0;
/// Sprinting speed in m/s.
const SPRINT_SPEED: f32 = 6.0;

/// Strategy-line walking feel: acceleration and friction high enough to reach speed, or
/// stop, within a tick. `max_speed` and `air_speed_cap` are set per tick from sprint.
const STRATEGY_MOVE_SETTINGS: moho_physics::MoveSettings = moho_physics::MoveSettings {
    max_speed: MOVE_SPEED,
    ground_accel: 360.0,
    friction: 60.0,
    stop_speed: 1.0,
    air_accel: 360.0,
    air_speed_cap: MOVE_SPEED,
    gravity: 18.0,
    jump_speed: 8.0,
};

/// Handles frame timing and game state updates
pub struct FrameProcessor;

impl FrameProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Publish frame start event
    pub fn publish_frame_start(&self, app: &mut App, dt: f32) -> u64 {
        let frame_number = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
        app.event_bus
            .publish(moho_core::events::SystemEvent::FrameStart {
                frame_number,
                delta_time: dt,
            });
        frame_number
    }

    /// Update game state for the current frame
    pub fn update_game_state(&self, app: &mut App, dt: f32) {
        if app.game_state != crate::game_state::GameState::Playing {
            return;
        }

        let use_kcc = app.is_kcc_walking();

        if use_kcc {
            self.update_game_state_fps_kcc(app, dt);
        } else {
            // Flying camera / noclip / RTS
            let grid = app
                .light_system
                .as_ref()
                .map(flagstone_voxel::LightSystem::grid);
            // Top face of the highest solid block in the column; `None` when the column is unloaded or has no solid block.
            let ground = |x: f32, z: f32| {
                grid?
                    .get_height(x.floor() as i32, z.floor() as i32)
                    .map(|top| top as f32 + 1.0)
            };
            let (view, proj, eye) = app.simulation.apply_input(dt, ground);
            app.camera = (view, proj, eye);
        }

        // Step rigid bodies in both KCC and non-KCC paths (test spheres, etc.)
        if !use_kcc {
            self.step_physics_bodies(app, dt);
        }

        // Clear zoom delta after use
        app.simulation.controller_input.zoom_delta = 0.0;
    }

    fn update_game_state_fps_kcc(&self, app: &mut App, dt: f32) {
        // Kill plane: anything below this Y is considered "off the map"
        const KILL_PLANE_Y: f32 = -30.0;

        let input = app.simulation.controller_input;
        let command = moho_physics::CharacterCommand {
            forward: input.forward,
            right: input.right,
            yaw_delta: input.yaw_delta,
            pitch_delta: input.pitch_delta,
            jump: app.physics.jump_pressed,
        };
        let speed = if input.sprint {
            SPRINT_SPEED
        } else {
            MOVE_SPEED
        };
        let settings = moho_physics::MoveSettings {
            max_speed: speed,
            air_speed_cap: speed,
            ..STRATEGY_MOVE_SETTINGS
        };
        let state = app
            .physics
            .move_character(&command, &settings, dt)
            .expect("KCC active but move_character returned None");
        let mut new_pos = state.position;

        if new_pos.y < KILL_PLANE_Y {
            let respawn_y = app
                .light_system
                .as_ref()
                .and_then(|ls| ls.grid().get_height(0, 0))
                .unwrap_or(10) as f32
                + 3.0;
            new_pos = glam::Vec3::new(0.0, respawn_y, 0.0);
            app.physics.teleport_character(new_pos);
            tracing::info!(pos = ?new_pos, "Player fell off map — respawning");
        }

        // Override simulation position with physics result (or respawn position)
        app.simulation
            .set_position_yaw_pitch(new_pos, state.yaw, state.pitch);

        // Rebuild camera from the updated simulation state
        app.camera =
            flagstone_game::controller::controller_to_camera(&app.simulation.player_controller);

        self.step_physics_bodies(app, dt);
    }

    /// Step dynamic rigid bodies and sync their positions into actors.
    /// Called from both the KCC path and the non-KCC path so test spheres move in all modes.
    fn step_physics_bodies(&self, app: &mut App, dt: f32) {
        let updates = app.physics.step(dt);
        for (actor, pos) in updates {
            if let Some(center) = app.entities.actors.center_mut(actor) {
                *center = pos;
            } else {
                let body = app.physics.forget_body(actor);
                tracing::warn!(body = ?body, actor = ?actor, "Physics body update for missing actor");
            }
        }
    }

    /// Build the cut-face caps of chunks whose ground changed since the last frame.
    pub fn update_cut_caps(&self, app: &mut App) {
        let Some(light_system) = app.light_system.as_ref() else {
            return;
        };
        app.cut_caps.refresh(
            light_system.grid(),
            &app.entities.chunks,
            app.scene.world_meshes_mut(),
            app.cut_face_material_idx,
            &app.terrain_palette,
        );
    }

    /// Update chunk streaming: evict distant chunks, load nearby ones.
    pub fn update_chunk_streaming(&self, app: &mut App) {
        if app.game_state != crate::game_state::GameState::Playing {
            return;
        }
        let player_pos = app.simulation.position();
        let mut evicted = Vec::new();
        if let (Some(streamer), Some(ls)) = (&mut app.chunk_streamer, &mut app.light_system) {
            const FACE_DIRS: [glam::IVec3; 6] = [
                glam::IVec3::X,
                glam::IVec3::NEG_X,
                glam::IVec3::Y,
                glam::IVec3::NEG_Y,
                glam::IVec3::Z,
                glam::IVec3::NEG_Z,
            ];
            let loaded;
            (loaded, evicted) = streamer.update(ls.grid_mut(), player_pos);
            // Light first, so the meshes the publishes below trigger read it.
            ls.light_arrived_chunks(&loaded);

            // Directly publish ChunkMeshDirty for every loaded chunk: arrival only
            // records chunks whose light or sky changed, and a new chunk needs its
            // first mesh either way.
            //
            // Also re-mesh each new chunk's 6 face neighbors: when neighbor N
            // was previously meshed without this chunk present, it generated an
            // exposed edge face. Now that this chunk exists, N must re-sample
            // the density field to close the seam.
            for pos in loaded {
                app.event_bus
                    .publish(flagstone_voxel::WorldEvent::ChunkMeshDirty {
                        chunk_pos: pos,
                        terrain_dirty: true,
                        structure_dirty: false,
                    });
                for dir in FACE_DIRS {
                    let neighbor = pos + dir;
                    if ls.grid().has_chunk(neighbor) {
                        app.event_bus
                            .publish(flagstone_voxel::WorldEvent::ChunkMeshDirty {
                                chunk_pos: neighbor,
                                terrain_dirty: true,
                                structure_dirty: false,
                            });
                    }
                }
            }
        }

        // The streamer borrow above ends first: removal needs all of `app`.
        for pos in evicted {
            crate::app::world_geometry::remove_chunk(app, pos);
        }

        // Re-scan for LOD tier changes only when the player crosses a chunk boundary.
        let player_chunk = lod_player_chunk(player_pos);
        if player_chunk.x != app.lod_player_chunk_cache.x
            || player_chunk.z != app.lod_player_chunk_cache.z
        {
            app.lod_player_chunk_cache = player_chunk;
            self.update_lod_transitions(app, player_chunk);
        }
    }

    /// Emit `ChunkMeshDirty` for any stored chunk whose LOD tier has become stale.
    ///
    /// Runs only when the player crosses an XZ chunk boundary. `process_world_events`
    /// stamps the new LOD onto the freshly generated chunk, so the comparison goes
    /// idle after all transitions in the new position are processed.
    fn update_lod_transitions(&self, app: &mut App, player_chunk: glam::IVec3) {
        let mut dirty: Vec<glam::IVec3> = Vec::new();
        for chunk in app.entities.chunks.iter() {
            if chunk.lod() != lod_for_chunk(chunk.chunk_pos(), player_chunk) {
                dirty.push(chunk.chunk_pos());
            }
        }

        for pos in dirty {
            app.event_bus
                .publish(flagstone_voxel::WorldEvent::ChunkMeshDirty {
                    chunk_pos: pos,
                    terrain_dirty: true,
                    structure_dirty: false,
                });
        }
    }

    /// Update light propagation system for the current frame
    pub fn update_light_system(&self, app: &mut App) {
        if app.game_state != crate::game_state::GameState::Playing {
            return;
        }

        if let Some(ref mut light_system) = app.light_system {
            // Update player position for priority calculation
            let player_pos = app.camera.2; // Camera eye position
            light_system.set_player_position(player_pos);

            // Process light updates within frame budget
            light_system.process_frame();

            // Emit mesh dirty events for affected chunks
            let dirty_count = light_system.emit_dirty_events();
            if dirty_count > 0 {
                tracing::trace!(
                    count = dirty_count,
                    "Light system emitted mesh dirty events"
                );
            }
        }
    }

    pub fn update_lighting(
        &self,
        app: &App,
        clock: &moho_app::GameClock,
        renderer: &mut dyn moho_renderer::RendererBackend,
    ) {
        if app.game_state != crate::game_state::GameState::Playing {
            return;
        }

        let (sun_dir, moon_dir) = clock.celestial_directions();
        let time = clock.time_of_day();

        let sun_intensity = if sun_dir.y > 0.0 { 1.0 } else { 0.0 };

        let moon_base_intensity = self.calculate_moon_intensity(moon_dir, time);

        let lighting = moho_renderer::LightingGpu {
            sun_direction: [sun_dir.x, sun_dir.y, sun_dir.z, sun_intensity],
            sun_color: [1.0, 0.95, 0.8, 0.0],
            moon_direction: [moon_dir.x, moon_dir.y, moon_dir.z, moon_base_intensity],
            moon_color: [0.7, 0.8, 0.9, 0.0],
            ambient: self.calculate_ambient_lighting(time),
            params: [time, app.debug_mode as f32, 0.0, 0.0],
            // The renderer fills the sky palette from the time of day.
            sky_zenith: [0.0; 4],
            sky_horizon: [0.0; 4],
            sky_ground: [0.0; 4],
        };
        renderer.update_lighting(lighting);
    }

    /// Calculate moon intensity based on position and time
    fn calculate_moon_intensity(&self, moon_dir: glam::Vec3, time: f32) -> f32 {
        if moon_dir.y <= 0.0 {
            return 0.0; // Moon below horizon
        }

        // Moon is stronger at night, weaker during day transitions
        if !(5.0..21.0).contains(&time) {
            // Deep night: full moon brightness
            0.4
        } else if (5.0..7.0).contains(&time) {
            // Dawn: moon fading
            let t = (time - 5.0) / 2.0; // 0-1 over 2 hours
            0.4 * (1.0 - t) // 0.4 -> 0.0
        } else if (17.0..21.0).contains(&time) {
            // Dusk: moon rising
            let t = (time - 17.0) / 4.0; // 0-1 over 4 hours
            0.4 * t // 0.0 -> 0.4
        } else {
            // Day: moon barely visible if at all
            0.0
        }
    }

    /// Returns the ambient uniform: sky-ambient strength in w, xyz unused
    fn calculate_ambient_lighting(&self, time: f32) -> [f32; 4] {
        if (7.0..17.0).contains(&time) {
            [0.0, 0.0, 0.0, 0.15]
        } else if (5.0..7.0).contains(&time) {
            let t = (time - 5.0) / 2.0; // 0-1 over 2 hours
            let intensity = 0.05 + t * 0.10; // 0.05 -> 0.15
            [0.0, 0.0, 0.0, intensity]
        } else if (17.0..21.0).contains(&time) {
            let t = (time - 17.0) / 4.0; // 0-1 over 4 hours
            let intensity = 0.15 - t * 0.10; // 0.15 -> 0.05
            [0.0, 0.0, 0.0, intensity]
        } else {
            [0.0, 0.0, 0.0, 0.05]
        }
    }

    /// Publish frame end event and process deferred events
    pub fn publish_frame_end(&self, app: &mut App, frame_number: u64) {
        app.event_bus
            .publish(moho_core::events::SystemEvent::FrameEnd { frame_number });
        app.event_bus.process_deferred();
    }

    /// Push current world state into the overlay HUD data.
    pub fn update_hud_data(&self, app: &App, clock: &moho_app::GameClock, tick_length: Duration) {
        let Some(ui_adapter) = &app.ui_adapter else {
            return;
        };

        let pos = app.simulation.position();
        let cp = lod_player_chunk(pos);
        let chunk_pos = [cp.x, cp.y, cp.z];

        let mode = app.simulation.camera_mode();
        let is_fps = mode == flagstone_game::controller::CameraMode::FirstPerson;

        let (yaw, _pitch) = app.simulation.yaw_pitch();

        // Collect unique XZ chunk positions for the debug minimap.
        let loaded_chunk_xz = app
            .light_system
            .as_ref()
            .map(|ls| {
                let mut seen = std::collections::HashSet::new();
                ls.grid()
                    .chunk_positions()
                    .filter(|p| seen.insert((p.x, p.z)))
                    .map(|p| [p.x, p.z])
                    .collect()
            })
            .unwrap_or_default();

        let mut hotbar: Vec<(u32, u32)> = app.pawn.inventory.iter().collect();
        hotbar.sort_by_key(|(resource_id, _)| *resource_id);

        // Only one tool exists today (`flagstone_game::tools::STARTING_TOOL`); the
        // label is a fixed string until a real tool-name lookup exists.
        let equipped_tool_label = app.pawn.equipped_tool.map(|_| "Pickaxe".to_string());

        let data = flagstone_ui::overlays::HudData {
            player_position: [pos.x, pos.y, pos.z],
            chunk_position: chunk_pos,
            camera_mode: format!("{mode:?}"),
            is_fps_mode: is_fps,
            frame_time_secs: tick_length.as_secs_f32(),
            time_of_day: clock.time_of_day(),
            material_under_crosshair: None,
            camera_yaw: yaw,
            player_health: 1.0,
            player_stamina: 1.0,
            loaded_chunk_xz,
            hotbar,
            equipped_tool_label,
            selected_slot: app.pawn.selected_slot,
            view_height: crate::app::view_level::layer_extent(app)
                .map(|extent| app.view_level.hud(extent)),
        };

        if let Ok(mut adapter) = ui_adapter.lock() {
            adapter.update_hud_data(data);
        }
    }
}

impl Default for FrameProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;
    use flagstone_voxel::{VoxelChunk, WorldEvent};
    use moho_physics::HitTag;

    const DT: f32 = 1.0 / 60.0;

    fn drain_mesh_dirty(app: &App) -> Vec<glam::IVec3> {
        app.world_event_rx
            .try_iter()
            .filter_map(|event| match event {
                WorldEvent::ChunkMeshDirty { chunk_pos, .. } => Some(chunk_pos),
                _ => None,
            })
            .collect()
    }

    fn spawn_sphere_with_body(
        app: &mut App,
        center: glam::Vec3,
    ) -> flagstone_game::actors::ActorId {
        let sphere = flagstone_game::actors::Sphere::new(
            center,
            0.5,
            flagstone_voxel::MaterialType::Lambertian {
                albedo: glam::Vec3::ONE,
            },
        );
        let actor = app.entities.actors.spawn_sphere(sphere);
        let handle = app
            .physics
            .world
            .as_mut()
            .expect("a new PhysicsController has a world")
            .add_dynamic_sphere(center, 0.5);
        app.physics.test_bodies.push((handle, actor));
        actor
    }

    #[test]
    fn update_game_state_moves_falling_sphere() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        spawn_sphere_with_body(&mut app, glam::Vec3::new(0.0, 50.0, 0.0));

        moho_app::HeadlessLoop::new(moho_app::LoopConfig::new(flagstone_game::TICK_HZ))
            .step(&mut app, 30);

        let y = app.entities.actors.spheres()[0].center.y;
        assert!(y < 49.0, "sphere should fall under gravity, y = {y}");
    }

    #[test]
    fn fps_with_active_kcc_moves_player_under_gravity() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(100.0, 100.0, 100.0), 0.0, 0.0);
        app.physics.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        assert!(app.physics.is_kcc_active());

        for _ in 0..30 {
            FrameProcessor::new().update_game_state(&mut app, DT);
        }

        let pos = app.simulation.position();
        assert!(
            pos.x.abs() < 0.01 && pos.z.abs() < 0.01,
            "player follows the character body, pos = {pos:?}"
        );
        assert!(
            pos.y < 50.0 && pos.y > 30.0,
            "player falls from the spawn height, pos = {pos:?}"
        );
    }

    #[test]
    fn chunk_streaming_while_playing_loads_chunks_and_publishes_mesh_dirty() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(8.0, 80.0, 8.0), 0.0, 0.0);
        app.chunk_streamer = Some(crate::app::chunk_streamer::ChunkStreamer::new(
            flagstone_game::scene_builders::TerrainConfig::default(),
            flagstone_voxel::StreamingConfig {
                load_radius_chunks: 0,
                unload_radius_chunks: 1,
                chunks_per_frame: 1,
            },
            "headless-test-chunk-streaming",
        ));
        let far = glam::IVec3::new(10, 4, 10);
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        grid.mutator()
            .place(flagstone_voxel::BlockPos::new(165, 70, 165), 1, None);
        // Unmodified, so eviction doesn't write a chunk file to the working directory.
        grid.clear_chunk_modified(far);
        app.entities.chunks.insert(VoxelChunk::empty(far));
        drain_mesh_dirty(&app);

        FrameProcessor::new().update_chunk_streaming(&mut app);

        let grid = app.light_system.as_ref().expect("light system").grid();
        let loaded: Vec<glam::IVec3> = grid.chunk_positions().collect();
        assert!(!loaded.is_empty(), "the player's column is loaded");
        assert!(loaded.iter().all(|p| p.x == 0 && p.z == 0));
        let dirty = drain_mesh_dirty(&app);
        for pos in &loaded {
            assert!(dirty.contains(pos), "loaded chunk {pos:?} must be remeshed");
        }
        assert!(
            app.entities.chunks.get(far).is_none(),
            "evicted chunk is dropped from the store"
        );
    }

    #[test]
    fn streamed_chunks_arrive_lit() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(8.0, 80.0, 8.0), 0.0, 0.0);
        app.chunk_streamer = Some(crate::app::chunk_streamer::ChunkStreamer::new(
            flagstone_game::scene_builders::TerrainConfig::default(),
            flagstone_voxel::StreamingConfig {
                load_radius_chunks: 0,
                unload_radius_chunks: 1,
                chunks_per_frame: 16,
            },
            "headless-test-streamed-lit",
        ));
        // Light left over from before the chunks' blocks existed, one voxel per layer.
        let planted: Vec<glam::IVec3> = (0..=8)
            .map(|layer| glam::IVec3::new(1, layer * 16 + 1, 1))
            .collect();
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        for pos in &planted {
            grid.set_block_light_rgb(*pos, [9, 9, 9]);
        }

        FrameProcessor::new().update_chunk_streaming(&mut app);

        let grid = app.light_system.as_ref().expect("light system").grid();
        let loaded: Vec<glam::IVec3> = grid.chunk_positions().collect();
        assert!(!loaded.is_empty(), "the player's column is loaded");
        for pos in planted {
            if loaded.contains(&(pos >> 4)) {
                assert_eq!(
                    grid.block_light_rgb_at(pos),
                    [0, 0, 0],
                    "stale light at {pos}"
                );
            }
        }
        let open_air = (0..16)
            .flat_map(|x| (0..16).map(move |z| (x, z)))
            .filter_map(|(x, z)| grid.get_height(x, z).map(|h| glam::IVec3::new(x, h + 1, z)))
            .find(|above| above.y.rem_euclid(16) != 0 && loaded.contains(&(*above >> 4)))
            .expect("some loaded column has open air above its surface in the same chunk");
        assert!(
            grid.sky_exposed_at(open_air),
            "air above terrain at {open_air} sees the sky"
        );
        let buried = open_air - glam::IVec3::Y;
        assert!(
            !grid.sky_exposed_at(buried),
            "the surface block itself does not"
        );
    }

    #[test]
    fn lod_transition_marks_only_stale_chunks_dirty() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(8.0, 80.0, 8.0), 0.0, 0.0);
        let near_full_detail = glam::IVec3::new(1, 4, 0);
        let far_full_detail = glam::IVec3::new(6, 4, 0);
        app.entities
            .chunks
            .insert(VoxelChunk::empty(near_full_detail));
        app.entities
            .chunks
            .insert(VoxelChunk::empty(far_full_detail));
        drain_mesh_dirty(&app);

        FrameProcessor::new().update_chunk_streaming(&mut app);

        assert_eq!(drain_mesh_dirty(&app), vec![far_full_detail]);
    }

    #[test]
    fn moon_intensity_by_time() {
        let processor = FrameProcessor::new();
        let above = glam::Vec3::Y;
        let below = glam::Vec3::NEG_Y;
        let cases = [
            (below, 12.0, 0.0),
            (below, 23.0, 0.0),
            (above, 23.0, 0.4),
            (above, 5.0, 0.4),
            (above, 6.0, 0.2),
            (above, 7.0, 0.0),
            (above, 12.0, 0.0),
            (above, 18.0, 0.1),
            (above, 19.0, 0.2),
        ];

        for (moon_dir, time, expected) in cases {
            let got = processor.calculate_moon_intensity(moon_dir, time);

            assert!(
                (got - expected).abs() < 1e-6,
                "moon {moon_dir:?} at {time}: expected {expected}, got {got}"
            );
        }
    }

    #[test]
    fn ambient_strength_by_time() {
        let processor = FrameProcessor::new();
        let cases = [
            (12.0, 0.15),
            (23.0, 0.05),
            (5.0, 0.05),
            (6.0, 0.10),
            (6.99, 0.1495),
            (18.0, 0.125),
            (19.0, 0.10),
        ];

        for (time, expected_strength) in cases {
            let ambient = processor.calculate_ambient_lighting(time);

            assert_eq!(
                ambient[..3],
                [0.0, 0.0, 0.0],
                "colour channels are unused at {time}: {ambient:?}"
            );
            assert!(
                (ambient[3] - expected_strength).abs() < 1e-6,
                "strength at {time}: expected {expected_strength}, got {}",
                ambient[3]
            );
        }
    }

    fn floor_mesh() -> moho_render_api::WorldMesh {
        moho_render_api::WorldMesh::new(
            vec![
                [-50.0, 0.0, -50.0],
                [50.0, 0.0, -50.0],
                [-50.0, 0.0, 50.0],
                [50.0, 0.0, 50.0],
            ],
            vec![[0.0, 1.0, 0.0]; 4],
            vec![1.0; 4],
            vec![[0.0; 3]; 4],
            vec![1.0; 4],
            vec![0; 4],
            vec![0, 2, 1, 1, 2, 3],
        )
        .expect("valid floor mesh")
    }

    /// Walks forward on a floor until at speed, jumps for one tick, and returns the
    /// horizontal speed on every airborne tick that follows.
    fn airborne_speeds_after_jumping_while_walking(sprint: bool) -> Vec<f32> {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.physics
            .world
            .as_mut()
            .expect("physics world")
            .set_world_mesh(moho_render_api::WorldMeshId(1), &floor_mesh(), HitTag(0));
        app.physics.spawn_player(glam::Vec3::new(0.0, 1.2, 0.0));
        let frame_processor = FrameProcessor::new();
        for _ in 0..120 {
            frame_processor.update_game_state(&mut app, DT);
        }
        let input = app.simulation.controller_input_mut();
        input.forward = 1.0;
        input.sprint = sprint;
        for _ in 0..30 {
            frame_processor.update_game_state(&mut app, DT);
        }
        app.physics.jump_pressed = true;
        let player = app.physics.player.expect("player");
        let mut speeds = Vec::new();

        for tick in 0..20 {
            frame_processor.update_game_state(&mut app, DT);
            app.physics.jump_pressed = false;
            let state = app
                .physics
                .world
                .as_ref()
                .expect("physics world")
                .character(player)
                .expect("player resolves");
            if !state.grounded {
                speeds.push(glam::Vec2::new(state.velocity.x, state.velocity.z).length());
            }
            assert!(
                tick > 0 || !state.grounded,
                "the jump tick leaves the ground"
            );
        }
        speeds
    }

    #[test]
    fn jumping_while_walking_keeps_walking_speed_in_the_air() {
        let speeds = airborne_speeds_after_jumping_while_walking(false);

        assert!(speeds.len() > 10, "stayed airborne: {speeds:?}");
        assert!(
            speeds.iter().all(|s| *s <= 4.0 + 1e-3),
            "air speeds {speeds:?}"
        );
    }

    #[test]
    fn jumping_while_sprinting_keeps_sprint_speed_in_the_air() {
        let speeds = airborne_speeds_after_jumping_while_walking(true);

        assert!(speeds.len() > 10, "stayed airborne: {speeds:?}");
        assert!(
            speeds.iter().all(|s| (*s - 6.0).abs() <= 1e-2),
            "air speeds {speeds:?}"
        );
    }

    #[test]
    fn sprinting_on_the_ground_reaches_sprint_speed() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.physics
            .world
            .as_mut()
            .expect("physics world")
            .set_world_mesh(moho_render_api::WorldMeshId(1), &floor_mesh(), HitTag(0));
        app.physics.spawn_player(glam::Vec3::new(0.0, 1.2, 0.0));
        let frame_processor = FrameProcessor::new();
        for _ in 0..120 {
            frame_processor.update_game_state(&mut app, DT);
        }
        let input = app.simulation.controller_input_mut();
        input.forward = 1.0;
        input.sprint = true;

        for _ in 0..30 {
            frame_processor.update_game_state(&mut app, DT);
        }

        let player = app.physics.player.expect("player");
        let state = app
            .physics
            .world
            .as_ref()
            .expect("physics world")
            .character(player)
            .expect("player resolves");
        let speed = glam::Vec2::new(state.velocity.x, state.velocity.z).length();
        assert!(state.grounded, "{state:?}");
        assert!((speed - 6.0).abs() < 1e-2, "ground speed {speed}");
    }

    #[test]
    fn sprint_jump_from_standing_reaches_sprint_speed_in_the_air() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.physics
            .world
            .as_mut()
            .expect("physics world")
            .set_world_mesh(moho_render_api::WorldMeshId(1), &floor_mesh(), HitTag(0));
        app.physics.spawn_player(glam::Vec3::new(0.0, 1.2, 0.0));
        let frame_processor = FrameProcessor::new();
        for _ in 0..120 {
            frame_processor.update_game_state(&mut app, DT);
        }
        let input = app.simulation.controller_input_mut();
        input.forward = 1.0;
        input.sprint = true;
        app.physics.jump_pressed = true;
        let player = app.physics.player.expect("player");
        let mut airborne_speeds = Vec::new();

        for _ in 0..20 {
            frame_processor.update_game_state(&mut app, DT);
            app.physics.jump_pressed = false;
            let state = app
                .physics
                .world
                .as_ref()
                .expect("physics world")
                .character(player)
                .expect("player resolves");
            if !state.grounded {
                airborne_speeds.push(glam::Vec2::new(state.velocity.x, state.velocity.z).length());
            }
        }

        assert!(airborne_speeds.len() > 10, "{airborne_speeds:?}");
        let last = *airborne_speeds.last().unwrap();
        assert!((last - 6.0).abs() < 1e-2, "{airborne_speeds:?}");
        assert!(
            airborne_speeds.iter().all(|s| *s <= 6.0 + 1e-2),
            "{airborne_speeds:?}"
        );
    }

    fn hud_adapter() -> flagstone_ui::EguiAdapter {
        let mut adapter = flagstone_ui::build_adapter(
            None,
            std::sync::Arc::new(moho_core::events::EventBus::new()),
            moho_core::prefs::Prefs::default(),
        );
        adapter.set_game_state(flagstone_ui::GameState::Playing);
        adapter
    }

    /// Whether the overlay frame `update_hud_data` produces for an app in `mode`
    /// equals the frame painted from `expected_is_fps` HUD data of the same world.
    fn hud_matches_data_flagged(
        mode: flagstone_game::controller::CameraMode,
        expected_is_fps: bool,
    ) -> bool {
        use moho_render_api::UiFrameSource;
        let mut app = App::headless();
        app.simulation.set_camera_mode(mode);
        let clock = moho_app::GameClock::default();
        let adapter = std::sync::Arc::new(std::sync::Mutex::new(hud_adapter()));
        app.ui_adapter = Some(std::sync::Arc::clone(&adapter));
        FrameProcessor::new().update_hud_data(&app, &clock, Duration::from_secs_f32(DT));
        let produced = adapter
            .lock()
            .unwrap()
            .ui_frame([800, 600])
            .expect("playing paints");

        let pos = app.simulation.position();
        let cp = lod_player_chunk(pos);
        let mut reference = hud_adapter();
        reference.update_hud_data(flagstone_ui::overlays::HudData {
            player_position: [pos.x, pos.y, pos.z],
            chunk_position: [cp.x, cp.y, cp.z],
            camera_mode: format!("{mode:?}"),
            is_fps_mode: expected_is_fps,
            frame_time_secs: DT,
            time_of_day: clock.time_of_day(),
            camera_yaw: app.simulation.yaw_pitch().0,
            equipped_tool_label: Some("Pickaxe".to_string()),
            ..Default::default()
        });
        let expected = reference.ui_frame([800, 600]).expect("playing paints");

        produced == expected
    }

    /// The overlay frame `update_hud_data` produces for `app`, and the frame painted from
    /// the same world's HUD data with `view_height` in place of what the app reports.
    fn hud_frames_with(
        app: &mut App,
        view_height: Option<flagstone_ui::overlays::ViewHeightHud>,
    ) -> (moho_render_api::UiFrame, moho_render_api::UiFrame) {
        use moho_render_api::UiFrameSource;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::Isometric);
        let clock = moho_app::GameClock::default();
        let adapter = std::sync::Arc::new(std::sync::Mutex::new(hud_adapter()));
        app.ui_adapter = Some(std::sync::Arc::clone(&adapter));
        FrameProcessor::new().update_hud_data(app, &clock, Duration::from_secs_f32(DT));
        let produced = adapter
            .lock()
            .unwrap()
            .ui_frame([800, 600])
            .expect("playing paints");

        let pos = app.simulation.position();
        let cp = lod_player_chunk(pos);
        let mut reference = hud_adapter();
        reference.update_hud_data(flagstone_ui::overlays::HudData {
            player_position: [pos.x, pos.y, pos.z],
            chunk_position: [cp.x, cp.y, cp.z],
            camera_mode: "Isometric".to_string(),
            is_fps_mode: false,
            frame_time_secs: DT,
            time_of_day: clock.time_of_day(),
            camera_yaw: app.simulation.yaw_pitch().0,
            equipped_tool_label: Some("Pickaxe".to_string()),
            view_height,
            ..Default::default()
        });
        let expected = reference.ui_frame([800, 600]).expect("playing paints");
        (produced, expected)
    }

    #[test]
    fn update_hud_data_carries_the_view_height() {
        use crate::app::cut_caps::tests::ground_app;
        use flagstone_ui::overlays::ViewHeightHud;
        let cases = [(None, 32), (Some(27), 27), (Some(1), 1)];

        for (chosen, level) in cases {
            let mut app = ground_app();
            crate::app::view_level::update(&mut app, chosen);
            let shown = ViewHeightHud {
                floor: 0,
                top: 32,
                level,
            };

            let (produced, expected) = hud_frames_with(&mut app, Some(shown));
            let (_, without_slider) = hud_frames_with(&mut app, None);

            assert!(
                produced == expected,
                "level {level}: not the slider's frame"
            );
            assert!(
                produced != without_slider,
                "level {level}: the HUD data carries no slider"
            );
        }
    }

    #[test]
    fn update_hud_data_flags_first_person_mode_to_the_overlays() {
        use flagstone_game::controller::CameraMode;

        assert!(hud_matches_data_flagged(CameraMode::FirstPerson, true));
        assert!(!hud_matches_data_flagged(CameraMode::FirstPerson, false));
    }

    #[test]
    fn update_hud_data_flags_non_first_person_mode_to_the_overlays() {
        use flagstone_game::controller::CameraMode;

        assert!(hud_matches_data_flagged(CameraMode::Isometric, false));
        assert!(!hud_matches_data_flagged(CameraMode::Isometric, true));
    }
}
