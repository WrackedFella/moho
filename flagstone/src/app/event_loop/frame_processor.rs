//! Frame Processing
//!
//! Handles per-tick game state updates and per-frame lighting and HUD data.

use crate::App;
use crate::app::event_loop::event_processor::{lod_for_chunk, lod_player_chunk};
use crate::app::event_loop::world_edit::apply_pawn_dig;
use crate::app::input_state::Designation;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Frame counter for tracking frame numbers
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Walking speed in m/s.
const MOVE_SPEED: f32 = 4.0;
/// Sprinting speed in m/s.
const SPRINT_SPEED: f32 = 6.0;

/// A working pawn's walking speed in m/s. Faster than about 3 m/s, a pawn climbing the 45
/// degree face of a one-block rise carries its upward speed over the crest and leaves the
/// ground.
const JOB_WALK_SPEED: f32 = 3.0;

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

/// The character command and move settings that carry out one tick of a pawn's walk.
fn job_walk_command(
    walk: flagstone_game::jobs::WalkStep,
) -> (moho_physics::CharacterCommand, moho_physics::MoveSettings) {
    let command = moho_physics::CharacterCommand {
        forward: walk.forward,
        yaw_delta: walk.yaw_delta,
        ..moho_physics::CharacterCommand::default()
    };
    let settings = moho_physics::MoveSettings {
        max_speed: JOB_WALK_SPEED,
        air_speed_cap: JOB_WALK_SPEED,
        ..STRATEGY_MOVE_SETTINGS
    };
    (command, settings)
}

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

        self.run_pawn_job(app, dt);

        // Step rigid bodies in both KCC and non-KCC paths (test spheres, etc.)
        if !use_kcc {
            self.step_physics_bodies(app, dt);
        }

        // Clear zoom delta after use
        app.simulation.controller_input.zoom_delta = 0.0;
    }

    /// Runs every pawn's job for one tick: each walk goes through the same character step as
    /// the first-person path, each dig through the same mining action from that pawn's eye. An
    /// idle pawn is not moved at all. The possessed pawn, if any, takes no job: the player
    /// moves it, after which the others work on.
    fn run_pawn_job(&self, app: &mut App, dt: f32) {
        use flagstone_game::jobs::{self, EYE_HEIGHT, JobAction, Pose};
        use flagstone_game::pawn::{MINE_REACH, PawnId};
        use std::collections::BTreeMap;

        let Some(world) = app.physics.world.as_ref() else {
            return;
        };
        let bodies: BTreeMap<PawnId, moho_physics::CharacterState> = app
            .physics
            .pawns
            .iter()
            .filter_map(|(&id, &handle)| world.character(handle).map(|state| (id, state)))
            .collect();
        let poses: BTreeMap<PawnId, Pose> = bodies
            .iter()
            .map(|(&id, state)| {
                let pose = Pose {
                    feet: state.eye_position() - glam::Vec3::Y * EYE_HEIGHT,
                    yaw: state.yaw,
                };
                (id, pose)
            })
            .collect();
        let Some(light_system) = app.light_system.as_mut() else {
            return;
        };
        let actions = jobs::step_crew(
            &mut app.crew,
            &poses,
            &mut app.colony.marks,
            light_system.grid(),
            app.possessed,
        );

        for (id, action) in actions {
            match action {
                JobAction::Wait => {}
                JobAction::Walk { to } => {
                    let (command, settings) = job_walk_command(jobs::steer(poses[&id], to));
                    if app.physics.move_pawn(id, &command, &settings, dt).is_none() {
                        tracing::warn!(pawn = ?id, "Pawn job walked but the character did not move");
                    }
                }
                JobAction::Dig { aim, .. } => {
                    let eye = bodies[&id].eye_position();
                    let grid = light_system.grid_mut();
                    let pawn = app
                        .crew
                        .get_mut(id)
                        .expect("step_crew answers only for pawns of the crew");
                    let Some(outcome) = pawn.dig(grid, eye, (aim - eye).normalize(), MINE_REACH)
                    else {
                        tracing::debug!(pawn = ?id, ?aim, "Pawn dig found nothing solid within reach");
                        continue;
                    };
                    apply_pawn_dig(&app.event_bus, &mut app.colony.stock, grid, &outcome);
                }
            }
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

        // Nothing is possessed in the strategy view: no hotbar, tool or pickups to show.
        let possessed = app.possessed.and_then(|id| app.crew.get(id));
        let hotbar = possessed.map_or_else(Vec::new, |pawn| pawn.inventory.sorted());

        // Only one tool exists today (`flagstone_game::tools::STARTING_TOOL`); the
        // label is a fixed string until a real tool-name lookup exists.
        let tool_label =
            |pawn: &flagstone_game::pawn::Pawn| pawn.equipped_tool.map(|_| "Pickaxe".to_string());
        let equipped_tool_label = possessed.and_then(tool_label);

        let selection = app
            .selection
            .selected()
            .and_then(|id| app.crew.get(id))
            .map(|selected| flagstone_ui::overlays::SelectionHud {
                title: "Pawn".to_string(),
                tool: tool_label(selected),
                carried: selected
                    .inventory
                    .sorted()
                    .into_iter()
                    .map(|(id, count)| (flagstone_game::inventory::resource_name(id), count))
                    .collect(),
            });

        let colony_stock = app.colony.stock.sorted();

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
            selected_slot: possessed.map(|pawn| pawn.selected_slot).unwrap_or_default(),
            view_height: crate::app::view_level::layer_extent(app)
                .map(|extent| app.view_level.hud(extent)),
            pickups: possessed
                .map(|pawn| {
                    pawn.pickups
                        .entries()
                        .iter()
                        .map(|e| (e.resource_id, e.amount, e.life()))
                        .collect()
                })
                .unwrap_or_default(),
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
        app.possessed_pawn_mut()
            .deposit(flagstone_game::inventory::ResourceYield {
                resource_id: 1,
                amount: 12,
            });
        if selected {
            app.selection
                .click(Some(flagstone_game::selection::FIRST_PAWN));
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
        app.possessed_pawn_mut().select_slot(3);

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
    fn the_selection_panel_shows_the_selected_pawns_inventory() {
        use flagstone_ui::overlays::SelectionHud;
        let mut app = App::headless();
        app.crew = flagstone_game::crew::Crew::default();
        let ids: Vec<_> = (0..3)
            .map(|_| app.crew.spawn(flagstone_game::pawn::Pawn::default()))
            .collect();
        for (id, (resource_id, amount)) in [(ids[1], (2, 5)), (ids[2], (1, 9))] {
            app.crew
                .get_mut(id)
                .expect("the pawn is in the crew")
                .deposit(flagstone_game::inventory::ResourceYield {
                    resource_id,
                    amount,
                });
        }
        let panel_of = |carried: Vec<(String, u32)>| SelectionHud {
            title: "Pawn".to_string(),
            tool: Some("Pickaxe".to_string()),
            carried,
        };
        let expected_with = |app: &App, panel: SelectionHud| {
            painted_strategy_hud(flagstone_ui::overlays::HudData {
                equipped_tool_label: Some("Pickaxe".to_string()),
                selection: Some(panel),
                ..strategy_hud_data(app)
            })
        };

        app.selection.click(Some(ids[2]));
        let iron_selected = produced_strategy_hud(&mut app);
        app.selection.click(Some(ids[1]));
        let other_selected = produced_strategy_hud(&mut app);

        assert!(
            iron_selected == expected_with(&app, panel_of(vec![("Iron ore".to_string(), 9)])),
            "the panel is not the iron pawn's inventory"
        );
        assert!(
            other_selected == expected_with(&app, panel_of(vec![("Resource 2".to_string(), 5)])),
            "the panel is not the other pawn's inventory"
        );
    }

    #[test]
    fn the_strategy_hud_after_stepping_out_carries_no_hotbar_and_no_tool() {
        use moho_app::{Game, TickContext};
        let mut app = App::headless();
        app.game_state = crate::game_state::GameState::Playing;
        app.possessed_pawn_mut()
            .deposit(flagstone_game::inventory::ResourceYield {
                resource_id: 1,
                amount: 12,
            });
        let step_out = crate::app::game::StrategyCommand {
            toggle_view: true,
            ..Default::default()
        };
        let mut clock = moho_app::GameClock::default();
        let mut ctx = TickContext {
            tick: 0,
            tick_length: Duration::from_secs_f32(DT),
            clock: &mut clock,
        };

        Game::tick(&mut app, &mut ctx, &step_out);

        assert_eq!(
            app.simulation.camera_mode(),
            flagstone_game::controller::CameraMode::Isometric,
            "setup: the player stepped out"
        );
        assert_eq!(app.possessed, None, "setup: nothing is possessed");
        let produced = produced_strategy_hud(&mut app);
        let expected = painted_strategy_hud(strategy_hud_data(&app));
        assert!(
            produced == expected,
            "the strategy HUD shows a pawn's pockets"
        );
    }

    #[test]
    fn hud_data_carries_the_player_pawns_pickups() {
        use moho_render_api::UiFrameSource;
        let mut app = App::headless();
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.possessed_pawn_mut()
            .pickups
            .push(flagstone_game::inventory::ResourceYield {
                resource_id: 7,
                amount: 2,
            });
        for _ in 0..90 {
            app.possessed_pawn_mut().pickups.tick();
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

    mod colony_work {
        use super::*;
        use crate::app::dig_site::{
            Block, GROUND_TOP, PAWN_CELL, block_exists, dig_site, dig_site_with_pawns, pawn_start,
            pawn_state, pawn_state_of, rise, run_until,
        };
        use flagstone_game::controller::CameraMode;
        use flagstone_game::jobs::Job;
        use flagstone_game::pawn::{MINE_REACH, PawnId};
        use flagstone_game::selection::FIRST_PAWN;

        const ORE: u32 = 7;
        /// Four metres east of the pawn, on flat ground.
        const NEAR: [i32; 3] = [16, 8, 12];
        /// Twelve metres east of the pawn.
        const FAR: [i32; 3] = [24, 8, 12];
        const GIVE_UP: u32 = 900;

        fn site_with_mark(blocks: &[Block], mark: [i32; 3]) -> App {
            let mut app = dig_site(blocks);
            app.colony.marks.toggle(mark);
            app
        }

        fn walked(app: &App) -> f32 {
            pawn_state(app).position.distance(pawn_start())
        }

        fn ore_total(app: &App) -> u32 {
            app.colony.stock.count(ORE)
        }

        #[test]
        fn an_idle_pawn_digs_a_marked_ore_block_and_the_stock_grows_by_one() {
            let mut app = site_with_mark(&[(NEAR, Some(ORE))], NEAR);

            let removed = run_until(&mut app, GIVE_UP, |app| !block_exists(app, NEAR));

            assert!(removed, "the block is still standing after {GIVE_UP} ticks");
            assert!(walked(&app) > 1.5, "the pawn dug from where it started");
            assert!(!app.colony.marks.contains(NEAR), "the marker is gone");
            assert_eq!(ore_total(&app), 1);
            assert_eq!(
                app.colony.stock.iter().count(),
                1,
                "nothing else is counted"
            );
            assert!(
                app.crew
                    .get(FIRST_PAWN)
                    .is_some_and(|pawn| pawn.inventory.is_empty()),
                "a pawn at work fills the colony, not its pockets"
            );
        }

        #[test]
        fn a_plain_block_is_dug_and_the_stock_is_unchanged() {
            let mut app = site_with_mark(&[(NEAR, None)], NEAR);

            let removed = run_until(&mut app, GIVE_UP, |app| !block_exists(app, NEAR));

            assert!(removed, "the block is still standing after {GIVE_UP} ticks");
            assert!(walked(&app) > 1.5, "the pawn dug from where it started");
            assert!(!app.colony.marks.contains(NEAR), "the marker is gone");
            assert!(app.colony.stock.is_empty());
            assert!(
                app.crew
                    .get(FIRST_PAWN)
                    .is_some_and(|pawn| pawn.inventory.is_empty())
            );
        }

        #[test]
        fn the_pawn_crosses_a_one_block_rise_without_leaving_the_ground() {
            let target = [18, 8, 12];
            let mut blocks = rise(14..=16);
            blocks.push((target, Some(ORE)));
            let mut app = site_with_mark(&blocks, target);
            let mut sim = crate::app::dig_site::headless_loop();
            let mut airborne_on = Vec::new();
            let mut highest_centre = f32::MIN;

            for tick in 0..GIVE_UP {
                if !block_exists(&app, target) {
                    break;
                }
                sim.step(&mut app, 1);
                let state = pawn_state(&app);
                highest_centre = highest_centre.max(state.position.y);
                if tick > 0 && !state.grounded {
                    airborne_on.push(tick);
                }
            }

            assert!(!block_exists(&app, target), "the pawn never got through");
            assert!(
                pawn_state(&app).position.x > 14.5,
                "the pawn dug from beyond the ridge"
            );
            assert!(
                highest_centre > pawn_start().y + 0.5,
                "the pawn went over the ridge, highest centre {highest_centre}"
            );
            assert!(
                airborne_on.is_empty(),
                "left the ground on ticks {airborne_on:?}"
            );
            assert_eq!(ore_total(&app), 1);
        }

        #[test]
        fn the_pawn_does_not_dig_the_block_under_its_feet() {
            let under_feet = [PAWN_CELL[0], GROUND_TOP, PAWN_CELL[2]];
            let mut app = site_with_mark(&[], under_feet);
            let mut sim = crate::app::dig_site::headless_loop();

            let removed = run_until(&mut app, GIVE_UP, |app| !block_exists(app, under_feet));
            sim.step(&mut app, 120);

            let pawn = pawn_state(&app);
            let standing_centre = (GROUND_TOP + 1) as f32 + 1.15;
            assert!(removed, "the block is still standing after {GIVE_UP} ticks");
            assert!(pawn.grounded, "the pawn is hanging in the air");
            assert!(
                (pawn.position.y - standing_centre).abs() < 0.1,
                "the pawn is at {} instead of on the ground at {standing_centre}",
                pawn.position.y
            );
            assert!(
                pawn.position.x.floor() as i32 != under_feet[0]
                    || pawn.position.z.floor() as i32 != under_feet[2],
                "the pawn is still over the hole"
            );
        }

        #[test]
        fn the_pawn_digs_within_the_mining_reach() {
            let mut app = site_with_mark(&[(FAR, Some(ORE))], FAR);
            let far_from_start = pawn_start().distance(glam::Vec3::new(24.5, 8.5, 12.5));

            let removed = run_until(&mut app, GIVE_UP, |app| !block_exists(app, FAR));

            let eye = pawn_state(&app).eye_position();
            let block_centre = glam::Vec3::new(24.5, 8.5, 12.5);
            assert!(far_from_start > MINE_REACH, "the block starts out of reach");
            assert!(removed, "the block is still standing after {GIVE_UP} ticks");
            assert!(
                eye.distance(block_centre) <= MINE_REACH,
                "dug from {} m, reach is {MINE_REACH} m",
                eye.distance(block_centre)
            );
            assert!(walked(&app) > 3.0, "the pawn closed the distance first");
        }

        #[test]
        fn nothing_marked_leaves_the_pawn_where_it_was() {
            let mut app = dig_site(&[(NEAR, Some(ORE))]);
            let before = pawn_state(&app);
            let sim_before = (app.simulation.position(), app.simulation.yaw_pitch());

            crate::app::dig_site::headless_loop().step(&mut app, 120);

            assert_eq!(pawn_state(&app), before);
            assert_eq!(
                (app.simulation.position(), app.simulation.yaw_pitch()),
                sim_before
            );
            assert!(block_exists(&app, NEAR), "an unmarked block is left alone");
            assert!(app.colony.stock.is_empty());
        }

        #[test]
        fn cancelling_the_mark_stops_the_pawn_and_keeps_the_block() {
            let mut app = site_with_mark(&[(FAR, Some(ORE))], FAR);
            let mut sim = crate::app::dig_site::headless_loop();
            sim.step(&mut app, 30);
            let underway = walked(&app);

            app.colony.marks.toggle(FAR);
            sim.step(&mut app, 1);
            let stopped_at = pawn_state(&app).position;
            sim.step(&mut app, 120);

            assert!(underway > 1.0, "the pawn was walking, moved {underway} m");
            assert_eq!(
                pawn_state(&app).position,
                stopped_at,
                "the pawn kept walking after the cancel"
            );
            assert!(block_exists(&app, FAR), "the block is not removed");
            assert!(app.colony.stock.is_empty());
            assert!(!app.colony.marks.contains(FAR));
        }

        #[test]
        fn while_one_pawn_is_possessed_another_digs_the_mark() {
            // The possessed pawn stands nearer the block, so it would take the mark if it worked.
            let idle_cell = [PAWN_CELL[0], PAWN_CELL[1], 18];
            let mut app = dig_site_with_pawns(&[(NEAR, Some(ORE))], &[PAWN_CELL, idle_cell]);
            app.colony.marks.toggle(NEAR);
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            app.possessed = Some(FIRST_PAWN);
            let possessed_at = pawn_state(&app).position;
            let possessed_worked = std::cell::Cell::new(false);

            let removed = run_until(&mut app, GIVE_UP, |app| {
                possessed_worked.set(
                    possessed_worked.get()
                        || is_mining(app, FIRST_PAWN)
                        || claimant(app, NEAR) == Some(FIRST_PAWN),
                );
                !block_exists(app, NEAR)
            });

            assert!(!possessed_worked.get(), "the possessed pawn took the mark");
            assert!(
                app.crew
                    .get(FIRST_PAWN)
                    .is_some_and(|pawn| pawn.inventory.is_empty()),
                "the possessed pawn dug"
            );
            assert!(removed, "the block is still standing after {GIVE_UP} ticks");
            assert!(
                distance_walked(&app, PawnId(1), idle_cell) > 1.5,
                "the other pawn dug where it stood"
            );
            assert_eq!(ore_total(&app), 1, "the block was dug once");
            assert!(!app.colony.marks.contains(NEAR), "the marker is gone");
            let after = pawn_state(&app).position;
            assert!(
                (after.x - possessed_at.x).abs() < 1e-3 && (after.z - possessed_at.z).abs() < 1e-3,
                "the possessed pawn moved from {possessed_at:?} to {after:?}"
            );
        }

        /// The trench's columns: the ground is gone across the whole field here except under
        /// the causeways.
        const TRENCH_X: std::ops::RangeInclusive<i32> = 18..=19;
        /// The pawn crosses the trench; the block is beyond it.
        const BEYOND: [i32; 3] = [24, 8, 12];

        fn rebuild_collision(app: &mut App) {
            let grid = app.light_system.as_ref().expect("light system").grid();
            let chunks: Vec<flagstone_voxel::VoxelChunk> = (0..2)
                .flat_map(|x| (0..2).map(move |z| glam::IVec3::new(x, 0, z)))
                .map(|pos| flagstone_voxel::VoxelChunk::from_grid_lod(grid, pos, 0))
                .collect();
            for chunk in chunks {
                crate::app::world_geometry::insert_chunk(app, chunk);
            }
            app.physics.step(1.0 / 60.0);
        }

        /// A site whose ground is cut by a trench at x 18 and 19, crossed only by causeways
        /// three blocks wide centred on each of `bridges` (z), with `mark` marked beyond it.
        fn trench_site(bridges: &[i32], mark: [i32; 3]) -> App {
            let mut app = site_with_mark(&[(mark, None)], mark);
            let grid = app.light_system.as_mut().expect("light system").grid_mut();
            for x in TRENCH_X {
                for z in (0..32).filter(|z| bridges.iter().all(|b| (z - b).abs() > 1)) {
                    for y in 0..=GROUND_TOP {
                        grid.mutator()
                            .remove(flagstone_voxel::BlockPos::new(x, y, z));
                    }
                }
            }
            rebuild_collision(&mut app);
            app
        }

        /// Removes the ground at column `x` over `zs` the way a player's edit does: the grid,
        /// then `BlockRemoved` and a remesh for each block.
        fn cut_ground(app: &mut App, x: i32, zs: std::ops::RangeInclusive<i32>) {
            use flagstone_voxel::{BlockChangeReason, BlockPos, WorldEvent};
            let grid = app.light_system.as_mut().expect("light system").grid_mut();
            let chunk_size = grid.chunk_size();
            let mut removed = Vec::new();
            for z in zs {
                for y in 0..=GROUND_TOP {
                    let position = BlockPos::new(x, y, z);
                    if grid.mutator().remove(position) {
                        removed.push(position);
                    }
                }
            }
            for position in removed {
                app.event_bus.publish(WorldEvent::BlockRemoved {
                    position,
                    old_material_id: 0,
                    reason: BlockChangeReason::Player,
                });
                for chunk_pos in flagstone_voxel::chunks_meshed_from(position, chunk_size) {
                    app.event_bus.publish(WorldEvent::ChunkMeshDirty {
                        chunk_pos,
                        terrain_dirty: true,
                        structure_dirty: true,
                    });
                }
            }
        }

        fn claimed(app: &App, mark: [i32; 3]) -> bool {
            app.colony
                .marks
                .iter()
                .any(|(pos, m)| pos == mark && m.claimed_by.is_some())
        }

        #[test]
        fn a_hole_cut_in_the_route_stops_the_pawn_on_the_ground() {
            let mut app = trench_site(&[12], BEYOND);
            let mut sim = crate::app::dig_site::headless_loop();
            let walking = run_until(&mut app, GIVE_UP, |app| pawn_state(app).position.x > 15.5);
            assert!(walking, "the pawn never set out along the causeway");
            assert!(claimed(&app, BEYOND), "the walk is under way");

            let x_at_the_cut = pawn_state(&app).position.x;
            cut_ground(&mut app, 18, 11..=13);
            let ground_feet = (GROUND_TOP + 1) as f32;
            for tick in 0..120 {
                sim.step(&mut app, 1);

                let state = pawn_state(&app);
                let feet = state.position.y - 1.15;
                let centre = state.position.floor().as_ivec3();
                assert!(state.grounded, "airborne on tick {tick}: {state:?}");
                assert!(
                    (feet - ground_feet).abs() <= 0.05,
                    "feet at {feet}, ground at {ground_feet}, tick {tick}"
                );
                assert!(
                    !block_exists(&app, centre.to_array()),
                    "inside a solid cell {centre} on tick {tick}"
                );
                assert!(
                    (state.position.x - x_at_the_cut).abs() < 0.5,
                    "kept walking toward the hole: from {x_at_the_cut} to {} on tick {tick}",
                    state.position.x
                );
            }

            assert!(block_exists(&app, BEYOND), "nothing dug the block");
            assert!(app.colony.marks.contains(BEYOND), "the mark stays");
            assert!(!claimed(&app, BEYOND), "and is free to take");
        }

        #[test]
        fn a_cut_route_is_walked_around_and_the_block_dug() {
            let mark = [24, 8, 16];
            let mut app = trench_site(&[12, 20], mark);
            let mut sim = crate::app::dig_site::headless_loop();
            let walking = run_until(&mut app, GIVE_UP, |app| pawn_state(app).position.x > 15.5);
            assert!(walking, "the pawn never set out along the near causeway");
            let mut over_the_trench = Vec::new();
            let z_at_the_cut = pawn_state(&app).position.z;
            assert!(
                (11.0..14.0).contains(&z_at_the_cut),
                "the pawn is not on the near causeway's line: z {z_at_the_cut}"
            );

            cut_ground(&mut app, 18, 11..=13);
            for _ in 0..1500 {
                if !block_exists(&app, mark) {
                    break;
                }
                sim.step(&mut app, 1);
                let position = pawn_state(&app).position;
                if position.x > 18.1 && position.x < 19.9 {
                    over_the_trench.push(position.z);
                }
            }

            assert!(!block_exists(&app, mark), "the block was never dug");
            assert!(!over_the_trench.is_empty(), "the pawn never crossed");
            assert!(
                over_the_trench.iter().all(|z| (19.0..22.0).contains(z)),
                "crossed away from the far causeway: {over_the_trench:?}"
            );
            assert!(!app.colony.marks.contains(mark), "the marker is gone");
        }

        /// How many blocks of the field, ground and everything placed on it, are standing.
        fn solid_blocks(app: &App) -> usize {
            (0..32)
                .flat_map(|x| {
                    (0..=GROUND_TOP + 2).flat_map(move |y| (0..32).map(move |z| [x, y, z]))
                })
                .filter(|&pos| block_exists(app, pos))
                .count()
        }

        fn claimant(app: &App, mark: [i32; 3]) -> Option<PawnId> {
            app.colony
                .marks
                .iter()
                .find(|(pos, _)| *pos == mark)
                .and_then(|(_, m)| m.claimed_by)
        }

        fn is_mining(app: &App, id: PawnId) -> bool {
            matches!(
                app.crew.get(id).map(|pawn| &pawn.job),
                Some(Job::Mine { .. })
            )
        }

        fn distance_walked(app: &App, id: PawnId, from: [i32; 3]) -> f32 {
            let start = glam::Vec3::new(from[0] as f32 + 0.5, from[1] as f32, from[2] as f32 + 0.5);
            let feet = pawn_state_of(app, id).position - glam::Vec3::Y * 1.15;
            feet.distance(start)
        }

        #[test]
        fn two_pawns_one_mark_the_block_is_dug_once_and_one_pawn_never_moves() {
            let idle_cell = [PAWN_CELL[0], PAWN_CELL[1], 18];
            let mut app = dig_site_with_pawns(&[(NEAR, Some(ORE))], &[PAWN_CELL, idle_cell]);
            app.colony.marks.toggle(NEAR);
            let second = PawnId(1);
            let idle_before = pawn_state_of(&app, second);
            let solid_before = solid_blocks(&app);
            let mut sim = crate::app::dig_site::headless_loop();
            let mut first_worked_it = false;
            let mut second_took_work = false;

            for _ in 0..GIVE_UP {
                if !block_exists(&app, NEAR) {
                    break;
                }
                sim.step(&mut app, 1);
                first_worked_it |=
                    claimant(&app, NEAR) == Some(FIRST_PAWN) && is_mining(&app, FIRST_PAWN);
                second_took_work |= is_mining(&app, second);
            }
            sim.step(&mut app, 120);

            assert!(!block_exists(&app, NEAR), "the block is still standing");
            assert!(first_worked_it, "the first pawn never held the mark");
            assert!(
                !second_took_work,
                "the second pawn took a mark that was claimed"
            );
            assert_eq!(
                pawn_state_of(&app, second),
                idle_before,
                "the idle pawn moved"
            );
            assert!(
                distance_walked(&app, FIRST_PAWN, PAWN_CELL) > 1.5,
                "the first pawn dug where it stood"
            );
            assert_eq!(ore_total(&app), 1, "the block was dug once");
            assert_eq!(
                solid_blocks(&app),
                solid_before - 1,
                "only the marked block went"
            );
        }

        #[test]
        fn a_crew_of_three_digs_a_row_of_three_marks_once_each() {
            let rows = [8, 12, 16];
            let cells = rows.map(|z| [12, GROUND_TOP + 1, z]);
            let marks = rows.map(|z| [20, GROUND_TOP + 1, z]);
            let blocks: Vec<Block> = marks.iter().map(|&mark| (mark, Some(ORE))).collect();
            let mut app = dig_site_with_pawns(&blocks, &cells);
            for mark in marks {
                app.colony.marks.toggle(mark);
            }
            let solid_before = solid_blocks(&app);

            let removed = run_until(&mut app, GIVE_UP, |app| {
                marks.iter().all(|&mark| !block_exists(app, mark))
            });
            crate::app::dig_site::headless_loop().step(&mut app, 120);

            assert!(removed, "a block is still standing after {GIVE_UP} ticks");
            assert_eq!(ore_total(&app), 3, "each block was dug once");
            assert_eq!(
                solid_blocks(&app),
                solid_before - 3,
                "only the marked blocks went"
            );
            assert_eq!(
                app.colony.marks.iter().count(),
                0,
                "every mark is worked off"
            );
            let mut columns = Vec::new();
            for (index, row) in rows.into_iter().enumerate() {
                let id = PawnId(index as u32);
                let position = pawn_state_of(&app, id).position;
                assert!(
                    (position.z - (row as f32 + 0.5)).abs() < 1.5,
                    "pawn {index} dug in another pawn's row: {position:?}"
                );
                assert!(
                    position.x > 17.0,
                    "pawn {index} never walked to its block: {position:?}"
                );
                assert!(!is_mining(&app, id), "pawn {index} is still at work");
                columns.push((position.x.floor() as i32, position.z.floor() as i32));
            }
            columns.sort_unstable();
            columns.dedup();
            assert_eq!(columns.len(), 3, "the pawns piled onto one spot");
        }

        #[test]
        fn cancelling_a_mark_sends_its_pawn_to_the_pending_one() {
            let (first_mark, second_mark, pending) = (
                [22, GROUND_TOP + 1, 4],
                [22, GROUND_TOP + 1, 28],
                [12, GROUND_TOP + 1, 16],
            );
            let cells = [
                [PAWN_CELL[0], PAWN_CELL[1], 4],
                [PAWN_CELL[0], PAWN_CELL[1], 28],
            ];
            let blocks: Vec<Block> = [first_mark, second_mark, pending]
                .into_iter()
                .map(|mark| (mark, Some(ORE)))
                .collect();
            let mut app = dig_site_with_pawns(&blocks, &cells);
            for mark in [first_mark, second_mark, pending] {
                app.colony.marks.toggle(mark);
            }
            let mut sim = crate::app::dig_site::headless_loop();
            sim.step(&mut app, 30);
            assert_eq!(
                claimant(&app, first_mark),
                Some(PawnId(0)),
                "setup: each walks to its own"
            );
            assert_eq!(
                claimant(&app, second_mark),
                Some(PawnId(1)),
                "setup: each walks to its own"
            );
            assert_eq!(claimant(&app, pending), None, "setup: the third mark waits");
            assert!(
                distance_walked(&app, PawnId(0), cells[0]) > 1.0,
                "setup: the first pawn is under way"
            );

            app.colony.marks.toggle(first_mark);
            let done = run_until(&mut app, GIVE_UP, |app| {
                !block_exists(app, pending) && !block_exists(app, second_mark)
            });

            assert!(done, "the freed pawn never dug the pending block");
            assert!(
                block_exists(&app, first_mark),
                "the cancelled block was dug"
            );
            assert_eq!(
                ore_total(&app),
                2,
                "the pending and the second block, once each"
            );
            assert!(
                pawn_state_of(&app, PawnId(0)).position.z > 12.0,
                "the freed pawn did not go to the pending block"
            );
            assert!(
                pawn_state_of(&app, PawnId(1)).position.z > 24.0,
                "the second pawn left its own block"
            );
            assert!(
                !app.colony.marks.contains(first_mark),
                "the cancelled mark came back"
            );
        }

        /// A crew of two: the first near `near_first`, which it takes; the second near
        /// `near_second`, which it takes; `pending` is left for whoever frees up first.
        fn two_pawns_three_marks(
            near_first: [i32; 3],
            near_second: [i32; 3],
            pending: [i32; 3],
        ) -> App {
            let cells = [PAWN_CELL, [PAWN_CELL[0], PAWN_CELL[1], 24]];
            let blocks: Vec<Block> = [near_first, near_second, pending]
                .into_iter()
                .map(|mark| (mark, Some(ORE)))
                .collect();
            let mut app = dig_site_with_pawns(&blocks, &cells);
            for mark in [near_first, near_second, pending] {
                app.colony.marks.toggle(mark);
            }
            app
        }

        #[test]
        fn a_claimed_block_mined_by_hand_frees_its_pawn() {
            use flagstone_voxel::{BlockChangeReason, BlockPos, WorldEvent};
            let (near_first, near_second, pending) = ([16, 8, 12], [12, 8, 28], [24, 8, 28]);
            let mut app = two_pawns_three_marks(near_first, near_second, pending);
            let solid_before = solid_blocks(&app);
            let mut sim = crate::app::dig_site::headless_loop();
            sim.step(&mut app, 30);
            assert_eq!(claimant(&app, near_second), Some(PawnId(1)), "setup");
            assert_eq!(claimant(&app, pending), None, "setup: the third mark waits");

            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            app.possessed = Some(PawnId(0));
            sim.step(&mut app, 1);
            let position = BlockPos::from_array(near_second);
            let grid = app.light_system.as_mut().expect("light system").grid_mut();
            let chunk_size = grid.chunk_size();
            assert!(grid.mutator().remove(position), "setup: the block stands");
            app.event_bus.publish(WorldEvent::BlockRemoved {
                position,
                old_material_id: 0,
                reason: BlockChangeReason::Player,
            });
            for chunk_pos in flagstone_voxel::chunks_meshed_from(position, chunk_size) {
                app.event_bus.publish(WorldEvent::ChunkMeshDirty {
                    chunk_pos,
                    terrain_dirty: true,
                    structure_dirty: true,
                });
            }
            sim.step(&mut app, 1);
            app.simulation.set_camera_mode(CameraMode::Isometric);
            app.possessed = None;
            let done = run_until(&mut app, GIVE_UP, |app| {
                !block_exists(app, pending) && !block_exists(app, near_first)
            });
            sim.step(&mut app, 120);

            assert!(done, "the pending block was never dug");
            assert!(
                !block_exists(&app, near_second),
                "the hand-mined cell is air"
            );
            assert_eq!(
                solid_blocks(&app),
                solid_before - 3,
                "a block was dug twice"
            );
            assert_eq!(
                ore_total(&app),
                2,
                "the two pawn digs, not the hand-mined block"
            );
            assert!(
                app.crew
                    .get(PawnId(1))
                    .is_some_and(|p| p.inventory.is_empty()),
                "the freed pawn took the block's yield"
            );
            assert!(
                pawn_state_of(&app, PawnId(1)).position.x > 18.0,
                "the freed pawn did not go on to the pending block"
            );
            assert!(!is_mining(&app, PawnId(1)) && !is_mining(&app, PawnId(0)));
        }

        #[test]
        fn hud_data_carries_the_colony_stock() {
            let mut stocked = App::headless();
            stocked.colony.stock.add(7, 3);
            stocked.colony.stock.add(2, 5);
            let mut empty = App::headless();

            let produced = produced_strategy_hud(&mut stocked);

            let expected = painted_strategy_hud(flagstone_ui::overlays::HudData {
                colony_stock: vec![(2, 5), (7, 3)],
                ..strategy_hud_data(&stocked)
            });
            assert!(produced == expected, "not the totals sorted by id");
            assert!(
                produced != produced_strategy_hud(&mut empty),
                "the totals are not on the HUD data"
            );
        }
    }
}
