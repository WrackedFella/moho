//! A strategy-view world for the colony's job tests: a flat floor meshed into physics and
//! the pawn standing on it, ticked through the same `Game::tick` the binary runs.

use crate::App;
use crate::app::event_loop::event_processor::{lod_for_chunk, lod_player_chunk};
use crate::game_state::GameState;
use flagstone_game::TICK_HZ;
use flagstone_game::controller::CameraMode;
use flagstone_game::selection::PAWN_HALF_EXTENTS;
use flagstone_voxel::{BlockPos, VoxelChunk};
use moho_app::{Game, GameClock, LoopConfig, TickContext};

/// Floor columns run `0..FLOOR_SIZE` in x and z.
const FLOOR_SIZE: i32 = 32;
/// The floor is blocks `y = 0..=FLOOR_TOP`; its surface is one above.
pub(crate) const FLOOR_TOP: i32 = 3;
/// Where the pawn starts, horizontally.
pub(crate) const PAWN_START: (f32, f32) = (8.5, 8.5);

/// The floor block in column `(x, z)`.
pub(crate) fn floor_block(x: i32, z: i32) -> BlockPos {
    BlockPos::new(x, FLOOR_TOP, z)
}

/// A playing strategy-view app over a floor, the pawn settled on it facing +Z with nothing marked.
pub(crate) fn colony_app() -> App {
    let mut app = App::headless();
    app.game_state = GameState::Playing;
    app.simulation.set_camera_mode(CameraMode::Isometric);
    let grid = app
        .light_system
        .as_mut()
        .expect("App starts with a light system")
        .grid_mut();
    for x in 0..FLOOR_SIZE {
        for z in 0..FLOOR_SIZE {
            for y in 0..=FLOOR_TOP {
                grid.mutator().place(BlockPos::new(x, y, z), 1, None);
            }
        }
    }
    let surface = FLOOR_TOP as f32 + 1.0;
    let standing = glam::Vec3::new(
        PAWN_START.0,
        surface + PAWN_HALF_EXTENTS.y + 0.1,
        PAWN_START.1,
    );
    app.simulation.set_position_yaw_pitch(standing, 0.0, 0.0);
    mesh_the_world(&mut app);
    app.physics.spawn_player(standing);
    let settings = moho_physics::MoveSettings {
        max_speed: 4.0,
        ground_accel: 360.0,
        friction: 60.0,
        stop_speed: 1.0,
        air_accel: 360.0,
        air_speed_cap: 4.0,
        gravity: None,
        jump_speed: 8.0,
        max_velocity: 90.0,
        autohop: true,
        crouch_ticks: 0,
        prone_ticks: 0,
        lean_ticks: 0,
    };
    let dt = 1.0 / TICK_HZ as f32;
    for _ in 0..60 {
        app.physics
            .move_character(&moho_physics::CharacterCommand::default(), &settings, dt);
        app.physics.step(dt);
    }
    let settled = pawn_state(&app);
    assert!(
        settled.grounded && (settled.position.y - (surface + PAWN_HALF_EXTENTS.y)).abs() < 0.05,
        "the pawn did not settle on the floor: {settled:?}"
    );
    app.simulation
        .set_position_yaw_pitch(settled.position, settled.yaw, settled.pitch);
    app
}

/// Hands every floor chunk's mesh to physics, so blocks placed since are stood on.
pub(crate) fn mesh_the_world(app: &mut App) {
    let player_chunk = lod_player_chunk(app.simulation.position());
    for cx in 0..2 {
        for cz in 0..2 {
            let pos = glam::IVec3::new(cx, 0, cz);
            let chunk = VoxelChunk::from_grid_lod(
                app.light_system.as_ref().expect("light system").grid(),
                pos,
                lod_for_chunk(pos, player_chunk),
            );
            crate::app::world_geometry::insert_chunk(app, chunk);
        }
    }
}

/// Replaces the block at `pos` with one carrying `resource`.
pub(crate) fn place(app: &mut App, pos: BlockPos, resource: Option<u32>) {
    app.light_system
        .as_mut()
        .expect("light system")
        .grid_mut()
        .mutator()
        .place(pos, 1, resource);
}

/// A one-block rise across the whole floor at `x_from..=x_to`, so no way round it.
pub(crate) fn raise_a_rise(app: &mut App, x_from: i32, x_to: i32) {
    for x in x_from..=x_to {
        for z in 0..FLOOR_SIZE {
            place(app, BlockPos::new(x, FLOOR_TOP + 1, z), None);
        }
    }
    mesh_the_world(app);
}

pub(crate) fn is_solid(app: &App, pos: BlockPos) -> bool {
    app.light_system
        .as_ref()
        .expect("light system")
        .grid()
        .is_solid_at(pos)
}

pub(crate) fn pawn_state(app: &App) -> moho_physics::CharacterState {
    let handle = app.physics.player.expect("the pawn was spawned");
    app.physics
        .world
        .as_ref()
        .expect("physics world")
        .character(handle)
        .expect("the pawn exists")
}

/// One tick of the game, as the loop would run it.
pub(crate) fn tick(app: &mut App) {
    let command = Game::command(app);
    let mut clock = GameClock::default();
    let mut ctx = TickContext {
        tick: 0,
        tick_length: LoopConfig::new(TICK_HZ).tick_length(),
        clock: &mut clock,
    };
    Game::tick(app, &mut ctx, &command);
}

/// Ticks until nothing is marked, up to `limit`; whether it got there.
pub(crate) fn tick_until_unmarked(app: &mut App, limit: u32) -> bool {
    for _ in 0..limit {
        if app.colony.marks.iter().next().is_none() {
            return true;
        }
        tick(app);
    }
    app.colony.marks.iter().next().is_none()
}
