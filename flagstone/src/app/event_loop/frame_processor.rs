//! Frame Processing
//!
//! Handles per-tick game state updates and per-frame lighting and HUD data.

use crate::App;
use crate::app::event_loop::event_processor::{lod_for_chunk, lod_player_chunk, publish_pawn_dig};
use crate::app::input_state::Designation;
use flagstone_game::jobs::{self, JobAction, Pose};
use flagstone_game::pawn::{MINE_REACH, PawnId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Frame counter for tracking frame numbers
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Walking speed in m/s.
const MOVE_SPEED: f32 = 4.0;
/// Sprinting speed in m/s.
const SPRINT_SPEED: f32 = 6.0;

/// Height of the pawn's capsule centre (where mining starts) above its feet: the selection
/// box's half height, pinned to the capsule by a test.
const PAWN_CENTRE_HEIGHT: f32 = flagstone_game::selection::PAWN_HALF_EXTENTS.y;

/// Strategy-line walking feel: acceleration and friction high enough to reach speed, or
/// stop, within a tick. `max_speed` and `air_speed_cap` are set per tick from sprint.
const STRATEGY_MOVE_SETTINGS: moho_physics::MoveSettings = moho_physics::MoveSettings {
    max_speed: MOVE_SPEED,
    ground_accel: 360.0,
    friction: 60.0,
    stop_speed: 1.0,
    air_accel: 360.0,
    air_speed_cap: MOVE_SPEED,
    gravity: None,
    jump_speed: 8.0,
    max_velocity: 90.0,
    autohop: true,
    crouch_ticks: 0,
    prone_ticks: 0,
    lean_ticks: 0,
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
            // A possessed pawn (first person) takes no job.
            if app.simulation.camera_mode() == flagstone_game::controller::CameraMode::Isometric {
                self.run_pawn_job(app, dt);
            }
        }

        // Step rigid bodies in both KCC and non-KCC paths (test spheres, etc.)
        if !use_kcc {
            self.step_physics_bodies(app, dt);
        }

        // Clear zoom delta after use
        app.simulation.controller_input.zoom_delta = 0.0;
    }

    /// Runs the job of the pawn nobody possesses: walks it with the character controller or
    /// digs with its own mining action, and does nothing for a pawn with nothing to do.
    fn run_pawn_job(&self, app: &mut App, dt: f32) {
        let Some(body) = app
            .physics
            .world
            .as_ref()
            .zip(app.physics.player)
            .and_then(|(world, handle)| world.character(handle))
        else {
            return;
        };
        let pose = Pose {
            feet: body.position - glam::Vec3::Y * PAWN_CENTRE_HEIGHT,
            eye: body.position,
            yaw: body.yaw,
        };
        let Some(light_system) = app.light_system.as_ref() else {
            return;
        };
        let action = jobs::step(
            PawnId(0),
            &mut app.pawn,
            &pose,
            &mut app.colony.marks,
            light_system.grid(),
        );

        match action {
            JobAction::Wait => {}
            JobAction::Walk { to } => self.walk_pawn(app, &pose, to, dt),
            JobAction::Dig { aim, .. } => self.dig_with_pawn(app, &pose, aim),
        }
    }

    /// Moves the pawn's body one tick toward `to`, and its simulation pose with it.
    fn walk_pawn(&self, app: &mut App, pose: &Pose, to: glam::Vec3, dt: f32) {
        let walk = jobs::steer(pose, to);
        let command = moho_physics::CharacterCommand {
            forward: walk.forward,
            yaw_delta: walk.yaw_delta,
            ..moho_physics::CharacterCommand::default()
        };
        if let Some(moved) = app
            .physics
            .move_character(&command, &STRATEGY_MOVE_SETTINGS, dt)
        {
            app.simulation
                .set_position_yaw_pitch(moved.position, moved.yaw, moved.pitch);
        }
    }

    /// Digs toward `aim` with the pawn's own mining action, ends its job, and adds the yield
    /// to the colony's stock.
    fn dig_with_pawn(&self, app: &mut App, pose: &Pose, aim: glam::Vec3) {
        let Some(light_system) = app.light_system.as_mut() else {
            return;
        };
        let dir = (aim - pose.eye).normalize_or_zero();
        let outcome = app
            .pawn
            .dig(light_system.grid_mut(), pose.eye, dir, MINE_REACH);
        jobs::finish(&mut app.pawn, &mut app.colony.marks);
        if let Some(outcome) = outcome {
            if let Some(yield_) = outcome.yield_ {
                app.colony.stock.add(yield_.resource_id, yield_.amount);
            }
            publish_pawn_dig(app, &outcome);
        }
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
            ..moho_physics::CharacterCommand::default()
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

        let by_resource = |inventory: &flagstone_game::inventory::Inventory| {
            let mut entries: Vec<(u32, u32)> = inventory.iter().collect();
            entries.sort_by_key(|(resource_id, _)| *resource_id);
            entries
        };
        let hotbar = by_resource(&app.pawn.inventory);
        let colony_stock = by_resource(&app.colony.stock);

        // Only one tool exists today (`flagstone_game::tools::STARTING_TOOL`); the
        // label is a fixed string until a real tool-name lookup exists.
        let equipped_tool_label = app.pawn.equipped_tool.map(|_| "Pickaxe".to_string());

        let selection =
            app.selection
                .selected()
                .is_some()
                .then(|| flagstone_ui::overlays::SelectionHud {
                    title: "Pawn".to_string(),
                    tool: equipped_tool_label.clone(),
                    carried: hotbar
                        .iter()
                        .map(|&(id, count)| (flagstone_game::inventory::resource_name(id), count))
                        .collect(),
                });

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
            pickups: app
                .pawn
                .pickups
                .entries()
                .iter()
                .map(|e| (e.resource_id, e.amount, e.life()))
                .collect(),
            selection,
            mine_designation_active: app.input.designation == Designation::Mine,
            colony_stock,
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
pub(crate) mod tests {
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
            "saves",
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
            "saves",
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
    fn hud_data_carries_the_designation_state() {
        use crate::app::input_state::Designation;
        use moho_render_api::UiFrameSource;
        let produced_for = |designation: Designation| {
            let mut app = App::headless();
            app.simulation
                .set_camera_mode(flagstone_game::controller::CameraMode::Isometric);
            app.input.designation = designation;
            let clock = moho_app::GameClock::default();
            let adapter = std::sync::Arc::new(std::sync::Mutex::new(hud_adapter()));
            app.ui_adapter = Some(std::sync::Arc::clone(&adapter));
            FrameProcessor::new().update_hud_data(&app, &clock, Duration::from_secs_f32(DT));
            let produced = adapter
                .lock()
                .unwrap()
                .ui_frame([800, 600])
                .expect("playing paints");
            (app, clock, produced)
        };
        let (app, clock, inactive) = produced_for(Designation::None);
        let (_, _, active) = produced_for(Designation::Mine);

        let pos = app.simulation.position();
        let cp = lod_player_chunk(pos);
        let painted_with = |mine_designation_active: bool| {
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
                mine_designation_active,
                ..Default::default()
            });
            reference.ui_frame([800, 600]).expect("playing paints")
        };
        assert!(
            inactive == painted_with(false),
            "no designation is not the inactive control"
        );
        assert!(
            active == painted_with(true),
            "the designation is not on the HUD data"
        );
        assert!(active != inactive, "the control looks the same either way");
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

    /// The strategy-view overlay frame `update_hud_data` produces for `app`, once egui has
    /// sized its areas.
    fn produced_strategy_hud(app: &mut App) -> moho_render_api::UiFrame {
        use moho_render_api::UiFrameSource;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::Isometric);
        let clock = moho_app::GameClock::default();
        let adapter = std::sync::Arc::new(std::sync::Mutex::new(hud_adapter()));
        app.ui_adapter = Some(std::sync::Arc::clone(&adapter));
        FrameProcessor::new().update_hud_data(app, &clock, Duration::from_secs_f32(DT));
        let mut adapter = adapter.lock().unwrap();
        let _sizing = adapter.ui_frame([800, 600]);
        adapter.ui_frame([800, 600]).expect("playing paints")
    }

    /// The HUD data of `app`'s world as the strategy view reports it, before the pawn's
    /// own fields.
    fn strategy_hud_data(app: &App) -> flagstone_ui::overlays::HudData {
        let pos = app.simulation.position();
        let cp = lod_player_chunk(pos);
        flagstone_ui::overlays::HudData {
            player_position: [pos.x, pos.y, pos.z],
            chunk_position: [cp.x, cp.y, cp.z],
            camera_mode: "Isometric".to_string(),
            is_fps_mode: false,
            frame_time_secs: DT,
            time_of_day: moho_app::GameClock::default().time_of_day(),
            camera_yaw: app.simulation.yaw_pitch().0,
            ..Default::default()
        }
    }

    fn painted_strategy_hud(data: flagstone_ui::overlays::HudData) -> moho_render_api::UiFrame {
        use moho_render_api::UiFrameSource;
        let mut reference = hud_adapter();
        reference.update_hud_data(data);
        let _sizing = reference.ui_frame([800, 600]);
        reference.ui_frame([800, 600]).expect("playing paints")
    }

    fn app_carrying_iron_ore(selected: bool) -> App {
        let mut app = App::headless();
        app.pawn.deposit(flagstone_game::inventory::ResourceYield {
            resource_id: 1,
            amount: 12,
        });
        if selected {
            app.selection
                .click(Some(flagstone_game::selection::PLAYER_PAWN));
        }
        app
    }

    #[test]
    fn selection_panel_lists_the_tool_and_carried_resources() {
        use flagstone_ui::overlays::SelectionHud;
        let mut app = app_carrying_iron_ore(true);
        let mut unselected = app_carrying_iron_ore(false);

        let produced = produced_strategy_hud(&mut app);

        let panel = SelectionHud {
            title: "Pawn".to_string(),
            tool: Some("Pickaxe".to_string()),
            carried: vec![("Iron ore".to_string(), 12)],
        };
        let expected = painted_strategy_hud(flagstone_ui::overlays::HudData {
            hotbar: vec![(1, 12)],
            equipped_tool_label: Some("Pickaxe".to_string()),
            selection: Some(panel),
            ..strategy_hud_data(&app)
        });
        assert!(produced == expected, "not the panel the spec lists");
        assert!(
            produced != produced_strategy_hud(&mut unselected),
            "the panel is not drawn"
        );
    }

    #[test]
    fn selection_panel_omits_the_tool_when_unequipped() {
        use flagstone_ui::overlays::SelectionHud;
        let mut app = app_carrying_iron_ore(true);
        app.pawn.select_slot(3);

        let produced = produced_strategy_hud(&mut app);

        let panel = SelectionHud {
            title: "Pawn".to_string(),
            tool: None,
            carried: vec![("Iron ore".to_string(), 12)],
        };
        let expected = painted_strategy_hud(flagstone_ui::overlays::HudData {
            hotbar: vec![(1, 12)],
            equipped_tool_label: None,
            selected_slot: 3,
            selection: Some(panel),
            ..strategy_hud_data(&app)
        });
        assert!(produced == expected, "the panel lists a tool nobody holds");
    }

    #[test]
    fn no_selection_means_no_panel() {
        let mut selected = app_carrying_iron_ore(true);
        let mut unselected = app_carrying_iron_ore(false);

        let with_panel = produced_strategy_hud(&mut selected);
        let produced = produced_strategy_hud(&mut unselected);

        let expected = painted_strategy_hud(flagstone_ui::overlays::HudData {
            hotbar: vec![(1, 12)],
            equipped_tool_label: Some("Pickaxe".to_string()),
            selection: None,
            ..strategy_hud_data(&unselected)
        });
        assert!(produced == expected, "something is drawn for no pawn");
        assert!(
            produced != with_panel,
            "selecting the pawn draws no panel to compare against"
        );
    }

    #[test]
    fn hud_data_carries_the_player_pawns_pickups() {
        use moho_render_api::UiFrameSource;
        let mut app = App::headless();
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.pawn
            .pickups
            .push(flagstone_game::inventory::ResourceYield {
                resource_id: 7,
                amount: 2,
            });
        for _ in 0..90 {
            app.pawn.pickups.tick();
        }
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
        let painted_with = |pickups: Vec<(u32, u32, f32)>| {
            let mut reference = hud_adapter();
            reference.update_hud_data(flagstone_ui::overlays::HudData {
                player_position: [pos.x, pos.y, pos.z],
                chunk_position: [cp.x, cp.y, cp.z],
                camera_mode: "FirstPerson".to_string(),
                is_fps_mode: true,
                frame_time_secs: DT,
                time_of_day: clock.time_of_day(),
                camera_yaw: app.simulation.yaw_pitch().0,
                equipped_tool_label: Some("Pickaxe".to_string()),
                pickups,
                ..Default::default()
            });
            reference.ui_frame([800, 600]).expect("playing paints")
        };
        assert!(
            produced != painted_with(Vec::new()),
            "the pawn's pickup is not on the HUD"
        );
        assert!(
            produced == painted_with(vec![(7, 2, 0.25)]),
            "a pickup with 30 of 120 ticks left is drawn at a quarter life"
        );
    }

    /// The pawn's body state in the physics world.
    pub(crate) fn pawn_state(app: &App) -> moho_physics::CharacterState {
        app.physics
            .world
            .as_ref()
            .expect("physics world")
            .character(app.physics.player.expect("the pawn has a body"))
            .expect("the pawn's body resolves")
    }

    /// A strategy view over smooth ground: blocks x `0..32`, y `0..=7`, z `0..16`, so the top
    /// face is at y = 8, plus what `edit` adds. The chunks are meshed with colliders, and the
    /// pawn rests on the ground at x 2.5, z 8.5 facing +Z, its simulation pose matching.
    pub(crate) fn colony_world(edit: impl FnOnce(&mut flagstone_voxel::VoxelGrid)) -> App {
        use flagstone_game::controller::CameraMode;
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation.set_camera_mode(CameraMode::Isometric);
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        grid.mutator()
            .fill_region(glam::IVec3::ZERO, glam::IVec3::new(31, 7, 15), 0);
        edit(grid);
        let chunks: Vec<VoxelChunk> = (0..2)
            .map(|x| glam::IVec3::new(x, 0, 0))
            .map(|pos| VoxelChunk::from_grid_lod(grid, pos, 0))
            .collect();
        for chunk in chunks {
            crate::app::world_geometry::insert_chunk(&mut app, chunk);
        }
        app.physics.spawn_player(glam::Vec3::new(2.5, 12.0, 8.5));
        for _ in 0..180 {
            app.physics
                .move_character(
                    &moho_physics::CharacterCommand::default(),
                    &STRATEGY_MOVE_SETTINGS,
                    DT,
                )
                .expect("the pawn has a body");
            app.physics.step(DT);
        }
        let state = pawn_state(&app);
        assert!(state.grounded, "the pawn rests on the ground: {state:?}");
        app.simulation
            .set_position_yaw_pitch(state.position, state.yaw, state.pitch);
        app
    }

    fn block_pos(block: [i32; 3]) -> flagstone_voxel::BlockPos {
        flagstone_voxel::BlockPos::new(block[0], block[1], block[2])
    }

    fn is_solid(app: &App, block: [i32; 3]) -> bool {
        app.light_system
            .as_ref()
            .expect("light system")
            .grid()
            .is_solid_at(block_pos(block))
    }

    /// Runs ticks until `block` is gone or `limit` ticks have passed, and returns the pawn's
    /// body after each tick.
    fn run_until_dug(
        app: &mut App,
        block: [i32; 3],
        limit: u32,
    ) -> Vec<moho_physics::CharacterState> {
        let mut sim =
            moho_app::HeadlessLoop::<App>::new(moho_app::LoopConfig::new(flagstone_game::TICK_HZ));
        let mut states = Vec::new();
        for _ in 0..limit {
            sim.step(app, 1);
            states.push(pawn_state(app));
            if !is_solid(app, block) {
                break;
            }
        }
        states
    }

    /// The block four metres from where the pawn rests, at the top of the ground.
    const NEAR_BLOCK: [i32; 3] = [6, 7, 8];
    const ORE: u32 = 7;
    const TICK_LIMIT: u32 = 900;

    fn world_with_marked(block: [i32; 3], resource: Option<u32>) -> App {
        let mut app = colony_world(|grid| {
            grid.mutator().place(block_pos(block), 1, resource);
        });
        app.colony.marks.toggle(block);
        app
    }

    #[test]
    fn an_idle_pawn_digs_a_marked_ore_block_and_the_stock_grows_by_one() {
        let mut app = world_with_marked(NEAR_BLOCK, Some(ORE));
        let start = pawn_state(&app).position;

        let states = run_until_dug(&mut app, NEAR_BLOCK, TICK_LIMIT);

        let end = states.last().expect("ticks ran").position;
        assert!(!is_solid(&app, NEAR_BLOCK), "the block is removed");
        assert!(
            end.distance(start) > 1.0,
            "the pawn walked to it: {start:?} to {end:?}"
        );
        assert_eq!(
            app.colony.marks.iter().count(),
            0,
            "the mining marker is gone"
        );
        assert_eq!(
            app.colony.stock.count(ORE),
            1,
            "the colony total grew by one"
        );
        assert_eq!(
            app.colony.stock.iter().count(),
            1,
            "and nothing else was added"
        );
        assert!(
            app.pawn.inventory.is_empty(),
            "a job does not fill the pawn's own pack"
        );
    }

    #[test]
    fn a_pawn_job_dig_goes_through_the_shared_publisher() {
        use moho_core::events::AudioEvent;
        let mut app = world_with_marked(NEAR_BLOCK, Some(ORE));
        let removed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let dirty = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let cues = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (removed_sink, dirty_sink, cue_sink) = (removed.clone(), dirty.clone(), cues.clone());
        app.event_bus.subscribe(move |e: &WorldEvent| match e {
            WorldEvent::BlockRemoved { position, .. } => {
                removed_sink.lock().unwrap().push(*position);
            }
            WorldEvent::ChunkMeshDirty { chunk_pos, .. } => {
                dirty_sink.lock().unwrap().push(*chunk_pos);
            }
            _ => {}
        });
        app.event_bus.subscribe(move |e: &AudioEvent| {
            if let AudioEvent::PlaySoundAt { path, position, .. } = e {
                cue_sink.lock().unwrap().push((path.clone(), *position));
            }
        });

        run_until_dug(&mut app, NEAR_BLOCK, TICK_LIMIT);

        assert_eq!(
            *removed.lock().unwrap(),
            vec![block_pos(NEAR_BLOCK)],
            "one removal, published once"
        );
        assert!(
            dirty.lock().unwrap().contains(&glam::IVec3::ZERO),
            "the chunk of the dug block is marked for re-meshing"
        );
        let dig_asset = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/dig.wav");
        let cues = cues.lock().unwrap();
        assert_eq!(cues.len(), 1, "one dig cue: {cues:?}");
        assert_eq!(std::path::Path::new(&cues[0].0), dig_asset.as_path());
        assert_eq!(
            cues[0].1,
            glam::Vec3::new(6.5, 7.5, 8.5),
            "at the block's centre"
        );
    }

    #[test]
    fn a_plain_block_is_dug_and_the_stock_is_unchanged() {
        let mut app = world_with_marked(NEAR_BLOCK, None);
        let start = pawn_state(&app).position;

        let states = run_until_dug(&mut app, NEAR_BLOCK, TICK_LIMIT);

        let end = states.last().expect("ticks ran").position;
        assert!(!is_solid(&app, NEAR_BLOCK), "the block is removed");
        assert!(
            end.distance(start) > 1.0,
            "the pawn walked to it: {start:?} to {end:?}"
        );
        assert_eq!(
            app.colony.marks.iter().count(),
            0,
            "the mining marker is gone"
        );
        assert!(app.colony.stock.is_empty(), "a plain block yields nothing");
        assert!(app.pawn.inventory.is_empty());
    }

    /// The far side of a one-block rise: blocks x `8..=11`, z `0..16` raised to y = 8, and the
    /// block to dig beyond it.
    const BEYOND_THE_RISE: [i32; 3] = [14, 7, 8];
    /// A block 11.5 m from where the pawn rests, on level ground.
    const FAR_BLOCK: [i32; 3] = [14, 7, 8];

    #[test]
    fn the_pawn_crosses_a_one_block_rise_without_leaving_the_ground() {
        let mut app = colony_world(|grid| {
            grid.mutator()
                .fill_region(glam::IVec3::new(8, 8, 0), glam::IVec3::new(11, 8, 15), 0);
            grid.mutator().place(block_pos(BEYOND_THE_RISE), 1, None);
        });
        app.colony.marks.toggle(BEYOND_THE_RISE);
        let rest_height = pawn_state(&app).position.y;

        let states = run_until_dug(&mut app, BEYOND_THE_RISE, TICK_LIMIT);

        assert!(!is_solid(&app, BEYOND_THE_RISE), "the block is removed");
        let highest = states.iter().map(|s| s.position.y).fold(f32::MIN, f32::max);
        assert!(
            highest > rest_height + 0.8,
            "the pawn climbed the rise: rest {rest_height}, highest {highest}"
        );
        let airborne: Vec<usize> = states
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, state)| !state.grounded)
            .map(|(tick, _)| tick)
            .collect();
        assert!(airborne.is_empty(), "left the ground on ticks {airborne:?}");
    }

    #[test]
    fn the_pawn_digs_within_the_mining_reach() {
        let mut app = world_with_marked(FAR_BLOCK, None);

        let states = run_until_dug(&mut app, FAR_BLOCK, TICK_LIMIT);

        assert!(!is_solid(&app, FAR_BLOCK), "the block is removed");
        let eye = states.last().expect("ticks ran").position;
        let centre = glam::Vec3::new(14.5, 7.5, 8.5);
        assert!(
            eye.distance(centre) <= flagstone_game::pawn::MINE_REACH,
            "dug from {eye:?}, {} from the block",
            eye.distance(centre)
        );
    }

    #[test]
    fn nothing_marked_leaves_the_pawn_where_it_was() {
        let mut app = colony_world(|_| {});
        let before = (pawn_state(&app).position, app.simulation.position());
        let mut sim =
            moho_app::HeadlessLoop::<App>::new(moho_app::LoopConfig::new(flagstone_game::TICK_HZ));

        sim.step(&mut app, 120);

        assert_eq!(
            (pawn_state(&app).position, app.simulation.position()),
            before
        );
    }

    #[test]
    fn cancelling_the_mark_stops_the_pawn_and_keeps_the_block() {
        let mut app = world_with_marked(FAR_BLOCK, None);
        let start = pawn_state(&app).position;
        let mut sim =
            moho_app::HeadlessLoop::<App>::new(moho_app::LoopConfig::new(flagstone_game::TICK_HZ));
        sim.step(&mut app, 60);
        let walking = pawn_state(&app).position;

        app.colony.marks.toggle(FAR_BLOCK);
        sim.step(&mut app, 2);
        let stopped_at = pawn_state(&app).position;
        sim.step(&mut app, 120);

        assert!(walking.distance(start) > 0.5, "the pawn was on its way");
        assert_eq!(
            pawn_state(&app).position,
            stopped_at,
            "the pawn stands where the cancel found it"
        );
        assert!(is_solid(&app, FAR_BLOCK), "the block is not removed");
        assert_eq!(app.colony.marks.iter().count(), 0);
        assert!(app.colony.stock.is_empty());
    }

    #[test]
    fn a_possessed_pawn_runs_no_job() {
        let mut app = world_with_marked(NEAR_BLOCK, Some(ORE));
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        let before = pawn_state(&app).position;
        let mut sim =
            moho_app::HeadlessLoop::<App>::new(moho_app::LoopConfig::new(flagstone_game::TICK_HZ));

        sim.step(&mut app, 300);

        let after = pawn_state(&app).position;
        assert!(
            glam::Vec2::new(after.x - before.x, after.z - before.z).length() < 0.01,
            "the pawn walked on its own: {before:?} to {after:?}"
        );
        assert!(is_solid(&app, NEAR_BLOCK), "nothing was dug");
        assert!(app.colony.stock.is_empty());
        assert!(
            app.colony
                .marks
                .iter()
                .all(|(_, mark)| mark.claimed_by.is_none()),
            "a possessed pawn takes no mark"
        );
    }

    #[test]
    fn hud_data_carries_the_colony_stock() {
        let stocked = |entries: &[(u32, u32)]| {
            let mut app = App::headless();
            for &(resource, count) in entries {
                app.colony.stock.add(resource, count);
            }
            app
        };
        let mut app = stocked(&[(7, 3), (1, 2)]);
        let mut empty = stocked(&[]);

        let produced = produced_strategy_hud(&mut app);

        let expected = painted_strategy_hud(flagstone_ui::overlays::HudData {
            equipped_tool_label: Some("Pickaxe".to_string()),
            colony_stock: vec![(1, 2), (7, 3)],
            ..strategy_hud_data(&app)
        });
        assert!(produced == expected, "not the totals sorted by resource id");
        assert!(
            produced != produced_strategy_hud(&mut empty),
            "the totals are not drawn"
        );
    }

    fn headless_loop() -> moho_app::HeadlessLoop<App> {
        moho_app::HeadlessLoop::new(moho_app::LoopConfig::new(flagstone_game::TICK_HZ))
    }

    #[test]
    fn the_pawn_digs_two_marks_one_after_another() {
        let plain = NEAR_BLOCK;
        let ore = [10, 7, 8];
        let mut app = colony_world(|grid| {
            grid.mutator().place(block_pos(plain), 1, None);
            grid.mutator().place(block_pos(ore), 1, Some(ORE));
        });
        app.colony.marks.toggle(plain);
        app.colony.marks.toggle(ore);
        let mut sim = headless_loop();

        for _ in 0..TICK_LIMIT {
            sim.step(&mut app, 1);
            if !is_solid(&app, plain) && !is_solid(&app, ore) {
                break;
            }
        }

        assert!(
            !is_solid(&app, plain) && !is_solid(&app, ore),
            "both blocks are dug"
        );
        assert_eq!(
            app.colony.marks.iter().count(),
            0,
            "no marker is left, claimed or not"
        );
        assert_eq!(app.colony.stock.count(ORE), 1, "only the ore counts");
        assert_eq!(app.colony.stock.iter().count(), 1);
        assert!(app.pawn.inventory.is_empty());
        assert_eq!(app.pawn.job, flagstone_game::jobs::Job::Idle);
    }

    #[test]
    fn a_pawn_whose_block_was_removed_behind_its_back_takes_the_next_mark() {
        let mut app = world_with_marked(FAR_BLOCK, None);
        let start = pawn_state(&app).position;
        let mut sim = headless_loop();
        sim.step(&mut app, 60);
        let walking = pawn_state(&app).position;
        let second = [9, 7, 8];
        app.colony.marks.toggle(second);
        app.light_system
            .as_mut()
            .expect("light system")
            .grid_mut()
            .mutator()
            .remove(block_pos(FAR_BLOCK));
        app.colony.marks.block_removed(FAR_BLOCK);

        let mut left_claimed = false;
        for _ in 0..TICK_LIMIT {
            sim.step(&mut app, 1);
            left_claimed |= app.colony.marks.contains(FAR_BLOCK);
            if !is_solid(&app, second) {
                break;
            }
        }

        assert!(walking.distance(start) > 0.5, "it was on its way");
        assert!(!is_solid(&app, second), "the remaining mark is dug");
        assert!(!left_claimed, "the removed block's mark never comes back");
        assert_eq!(app.colony.marks.iter().count(), 0);
        assert!(app.colony.stock.is_empty(), "plain blocks add nothing");
        assert_eq!(app.pawn.job, flagstone_game::jobs::Job::Idle);
    }

    #[test]
    fn a_stale_mark_on_empty_air_is_never_claimed() {
        let mut app = world_with_marked(NEAR_BLOCK, Some(ORE));
        app.light_system
            .as_mut()
            .expect("light system")
            .grid_mut()
            .mutator()
            .remove(block_pos(NEAR_BLOCK));
        let before = (pawn_state(&app).position, app.simulation.position());
        let mut sim = headless_loop();

        sim.step(&mut app, 300);

        assert!(!is_solid(&app, NEAR_BLOCK));
        assert_eq!(
            (pawn_state(&app).position, app.simulation.position()),
            before,
            "the pawn stays where it is"
        );
        assert_eq!(app.pawn.job, flagstone_game::jobs::Job::Idle);
        assert!(
            app.colony.marks.contains(NEAR_BLOCK),
            "the stale mark stays"
        );
        assert!(
            app.colony
                .marks
                .iter()
                .all(|(_, mark)| mark.claimed_by.is_none()),
            "and is not claimed"
        );
        assert!(app.colony.stock.is_empty());
    }
}
