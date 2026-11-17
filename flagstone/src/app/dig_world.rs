//! A strategy-view world over a smooth floor, with real chunk meshes and physics, for the
//! tests of a pawn digging a marked block.

use crate::App;
use crate::app::world_geometry::insert_chunk;
use crate::game_state::GameState;
use flagstone_game::TICK_HZ;
use flagstone_game::controller::CameraMode;
use flagstone_game::raycast::raycast_surface;
use flagstone_game::selection::PAWN_HALF_EXTENTS;
use flagstone_voxel::{BlockPos, VoxelChunk};
use glam::{IVec3, Vec3};

pub(crate) const DT: f32 = 1.0 / 60.0;

/// The floor spans blocks `0..FLOOR_EDGE` in x and z, two blocks thick.
const FLOOR_EDGE: i32 = 32;

/// The strategy line's walking settings, for settling the pawn before a test starts.
const SETTLE_SETTINGS: moho_physics::MoveSettings = moho_physics::MoveSettings {
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

/// A playing strategy-view app over the floor, plus `blocks` (position, resource) placed on
/// it. With `ridge`, a one-block-high ridge two blocks wide runs across the whole floor at
/// `x = 12..=13`. The floor's chunks are meshed and collidable.
pub(crate) fn dig_world(ridge: bool, blocks: &[(BlockPos, Option<u32>)]) -> App {
    let mut app = App::headless();
    app.game_state = GameState::Playing;
    app.simulation.set_camera_mode(CameraMode::Isometric);
    let grid = app.light_system.as_mut().expect("light system").grid_mut();
    grid.mutator().fill_region(
        IVec3::ZERO,
        IVec3::new(FLOOR_EDGE - 1, 1, FLOOR_EDGE - 1),
        0,
    );
    if ridge {
        grid.mutator()
            .fill_region(IVec3::new(12, 2, 0), IVec3::new(13, 2, FLOOR_EDGE - 1), 0);
    }
    for (pos, resource) in blocks {
        grid.mutator().place(*pos, 0, *resource);
    }
    let chunks: Vec<VoxelChunk> = (0..2)
        .flat_map(|x| (0..2).map(move |z| IVec3::new(x, 0, z)))
        .map(|pos| VoxelChunk::from_grid_lod(grid, pos, 0))
        .collect();
    for chunk in chunks {
        insert_chunk(&mut app, chunk);
    }
    app
}

/// The height of the drawn surface over world `(x, z)`.
pub(crate) fn surface_height(app: &App, x: f32, z: f32) -> f32 {
    let grid = app.light_system.as_ref().expect("light system").grid();
    raycast_surface(grid, Vec3::new(x, 30.0, z), Vec3::NEG_Y, 100.0)
        .expect("the floor is under the point")
        .position
        .y
}

/// Puts the pawn on the floor over `(x, z)`, facing +Z, and lets it settle so it starts
/// grounded and at rest, with the simulation at the character's pose.
pub(crate) fn stand_pawn(app: &mut App, x: f32, z: f32) {
    let centre = Vec3::new(x, surface_height(app, x, z) + PAWN_HALF_EXTENTS.y + 0.05, z);
    app.physics.spawn_player(centre);
    let idle = moho_physics::CharacterCommand::default();
    for _ in 0..60 {
        app.physics.move_character(&idle, &SETTLE_SETTINGS, DT);
        app.physics.step(DT);
    }
    let state = pawn_state(app);
    assert!(state.grounded, "setup: the pawn settles on the floor");
    app.simulation
        .set_position_yaw_pitch(state.position, state.yaw, 0.0);
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

/// Runs `ticks` simulation ticks of the game.
pub(crate) fn run(app: &mut App, ticks: u32) {
    moho_app::HeadlessLoop::<App>::new(moho_app::LoopConfig::new(TICK_HZ)).step(app, ticks);
}

pub(crate) fn is_solid(app: &App, pos: BlockPos) -> bool {
    app.light_system
        .as_ref()
        .expect("light system")
        .grid()
        .is_solid_at(pos)
}

/// The centre of the block at `pos`.
pub(crate) fn centre_of(pos: BlockPos) -> Vec3 {
    pos.as_vec3() + Vec3::splat(0.5)
}
