//! Frame Processing
//!
//! Handles per-tick game state updates and per-frame lighting and HUD data.

use crate::App;
use crate::app::event_loop::event_processor::{lod_for_chunk, lod_player_chunk};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Frame counter for tracking frame numbers
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);

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

        // --- Physics KCC path (FPS only) ---
        let is_fps = app.simulation.camera_mode() == moho_game::controller::CameraMode::FirstPerson;
        let use_kcc = is_fps && app.physics.is_kcc_active();

        if use_kcc {
            self.update_game_state_fps_kcc(app, dt);
        } else {
            // Flying camera / noclip / RTS
            let (view, proj, eye) = app.simulation.apply_input(dt);
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
        const MOVE_SPEED: f32 = 4.0;
        const SPRINT_SPEED: f32 = 6.0;

        // Capture movement intent before zeroing it
        let forward_input = app.simulation.controller_input.forward;
        let right_input = app.simulation.controller_input.right;
        let sprint = app.simulation.controller_input.sprint;

        // Zero translational input so apply_input only updates yaw/pitch
        {
            let ci = app.simulation.controller_input_mut();
            ci.forward = 0.0;
            ci.right = 0.0;
            ci.up = 0.0;
        }
        // apply_input updates yaw/pitch from deltas, returns camera
        app.simulation.apply_input(dt);

        // Read the now-updated yaw/pitch for horizontal movement calculation
        let (yaw, pitch) = app.simulation.yaw_pitch();

        let speed = if sprint { SPRINT_SPEED } else { MOVE_SPEED };

        // Build horizontal movement in world space from yaw + input.
        // Right vector matches controller.rs convention: (-cos(yaw), 0, sin(yaw))
        let sy = yaw.sin();
        let cy = yaw.cos();
        let fwd_world = glam::Vec3::new(sy, 0.0, cy);
        let right_world = glam::Vec3::new(-cy, 0.0, sy);

        let horizontal = (fwd_world * forward_input + right_world * right_input)
            .normalize_or_zero()
            * speed
            * dt;

        // Physics character movement + jump (JUMP_VELOCITY lives in PhysicsController)
        let mut new_pos = app
            .physics
            .move_character(horizontal, dt)
            .expect("KCC active but move_character returned None");

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
        app.simulation.set_position_yaw_pitch(new_pos, yaw, pitch);

        // Rebuild camera from the updated simulation state
        app.camera = moho_game::controller::controller_to_camera(&app.simulation.player_controller);

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

            // Directly publish ChunkMeshDirty for every loaded chunk. The light
            // system's emit_dirty_events only covers chunks that went through a
            // light-processing job, so freshly placed terrain blocks would never
            // reach the event processor otherwise.
            //
            // Also re-mesh each new chunk's 6 face neighbors: when neighbor N
            // was previously meshed without this chunk present, it generated an
            // exposed edge face. Now that this chunk exists, N must re-sample
            // the density field to close the seam.
            for pos in loaded {
                app.event_bus
                    .publish(moho_voxel::WorldEvent::ChunkMeshDirty {
                        chunk_pos: pos,
                        terrain_dirty: true,
                        structure_dirty: false,
                    });
                for dir in FACE_DIRS {
                    let neighbor = pos + dir;
                    if ls.grid().has_chunk(neighbor) {
                        app.event_bus
                            .publish(moho_voxel::WorldEvent::ChunkMeshDirty {
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
                .publish(moho_voxel::WorldEvent::ChunkMeshDirty {
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

        let (ambient_color, ambient_intensity) = self.calculate_ambient_lighting(time);

        let lighting = moho_renderer::LightingGpu {
            sun_direction: [sun_dir.x, sun_dir.y, sun_dir.z, sun_intensity],
            sun_color: [1.0, 0.95, 0.8, 0.0],
            moon_direction: [moon_dir.x, moon_dir.y, moon_dir.z, moon_base_intensity],
            moon_color: [0.7, 0.8, 0.9, 0.0],
            ambient: [
                ambient_color[0],
                ambient_color[1],
                ambient_color[2],
                ambient_intensity,
            ],
            params: [time, app.debug_mode as f32, 0.0, 0.0],
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

    /// Calculate ambient lighting based on time of day
    /// Returns (color, intensity)
    fn calculate_ambient_lighting(&self, time: f32) -> ([f32; 3], f32) {
        // Night: 0-5, 21-24 (very low, blue-tinted)
        // Dawn: 5-7 (increasing, warm tint)
        // Day: 7-17 (full brightness, neutral)
        // Dusk: 17-21 (decreasing, warm tint)
        if (7.0..17.0).contains(&time) {
            // Day: full brightness, cool ambient
            ([0.4, 0.5, 0.6], 0.15)
        } else if (5.0..7.0).contains(&time) {
            // Dawn: increasing brightness, warm tint
            let t = (time - 5.0) / 2.0; // 0-1 over 2 hours
            let intensity = 0.05 + t * 0.10; // 0.05 -> 0.15
            ([0.5, 0.45, 0.4], intensity)
        } else if (17.0..21.0).contains(&time) {
            // Dusk: decreasing brightness, warm tint
            let t = (time - 17.0) / 4.0; // 0-1 over 4 hours
            let intensity = 0.15 - t * 0.10; // 0.15 -> 0.05
            ([0.5, 0.4, 0.35], intensity)
        } else {
            // Night: very low brightness, blue tint
            ([0.3, 0.35, 0.5], 0.05)
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
        let is_fps = mode == moho_game::controller::CameraMode::FirstPerson;

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

        // Only one tool exists today (`moho_game::tools::STARTING_TOOL`); the
        // label is a fixed string until a real tool-name lookup exists.
        let equipped_tool_label = app.pawn.equipped_tool.map(|_| "Pickaxe".to_string());

        let data = moho_ui::overlays::HudData {
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
    use moho_voxel::{VoxelChunk, WorldEvent};

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

    fn spawn_sphere_with_body(app: &mut App, center: glam::Vec3) -> moho_game::actors::ActorId {
        let sphere = moho_game::actors::Sphere::new(
            center,
            0.5,
            moho_voxel::MaterialType::Lambertian {
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

        moho_app::HeadlessLoop::new(moho_app::LoopConfig::new(moho_game::TICK_HZ))
            .step(&mut app, 30);

        let y = app.entities.actors.spheres()[0].center.y;
        assert!(y < 49.0, "sphere should fall under gravity, y = {y}");
    }

    #[test]
    fn fps_with_active_kcc_moves_player_under_gravity() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_camera_mode(moho_game::controller::CameraMode::FirstPerson);
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(100.0, 100.0, 100.0), 0.0, 0.0);
        app.physics
            .world
            .as_mut()
            .expect("a new PhysicsController has a world")
            .add_character(glam::Vec3::new(0.0, 50.0, 0.0));
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
            moho_game::scene_builders::TerrainConfig::default(),
            moho_voxel::StreamingConfig {
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
            .place(moho_voxel::BlockPos::new(165, 70, 165), 1, None);
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
    fn ambient_lighting_by_time() {
        let processor = FrameProcessor::new();
        let cases = [
            (12.0, [0.4, 0.5, 0.6], 0.15),
            (23.0, [0.3, 0.35, 0.5], 0.05),
            (5.0, [0.5, 0.45, 0.4], 0.05),
            (6.0, [0.5, 0.45, 0.4], 0.10),
            (6.99, [0.5, 0.45, 0.4], 0.1495),
            (18.0, [0.5, 0.4, 0.35], 0.125),
            (19.0, [0.5, 0.4, 0.35], 0.10),
        ];

        for (time, expected_color, expected_intensity) in cases {
            let (color, intensity) = processor.calculate_ambient_lighting(time);

            assert!(
                glam::Vec3::from(color).abs_diff_eq(glam::Vec3::from(expected_color), 1e-6),
                "colour at {time}: {color:?}"
            );
            assert!(
                (intensity - expected_intensity).abs() < 1e-6,
                "intensity at {time}: expected {expected_intensity}, got {intensity}"
            );
        }
    }
}
