//! A strategy-view world for the tests of the pawn's job loop: flat ground with the pawn
//! standing on it, built from real blocks, meshes and physics colliders.

use crate::App;
use crate::app::world_geometry::insert_chunk;
use crate::game_state::GameState;
use flagstone_game::controller::CameraMode;
use flagstone_voxel::{BlockPos, VoxelChunk, VoxelGrid};
use glam::{IVec3, Vec3};
use moho_app::{HeadlessLoop, LoopConfig};

/// The highest layer of ground blocks; the walkable surface is its top face, at `y = 4`.
pub(crate) const GROUND_TOP: i32 = 3;

/// Where the pawn's capsule centre is dropped: its feet a few centimetres above the ground.
pub(crate) const PAWN_START: Vec3 = Vec3::new(8.5, 5.2, 8.5);

/// A playing strategy-view app over smooth ground at x `0..32`, z `0..16`, y `0..=GROUND_TOP`,
/// with `shape` applied to the blocks before they are meshed, and the pawn resting on the
/// ground below [`PAWN_START`] facing +Z, with no marks.
pub(crate) fn strategy_world(shape: impl FnOnce(&mut VoxelGrid)) -> App {
    let mut app = App::headless();
    app.game_state = GameState::Playing;
    app.simulation.set_camera_mode(CameraMode::Isometric);
    let grid = app.light_system.as_mut().expect("light system").grid_mut();
    grid.mutator()
        .fill_region(IVec3::ZERO, IVec3::new(31, GROUND_TOP, 15), 1);
    shape(grid);
    let chunks: Vec<VoxelChunk> = (0..2)
        .map(|x| VoxelChunk::from_grid_lod(grid, IVec3::new(x, 0, 0), 0))
        .collect();
    for chunk in chunks {
        insert_chunk(&mut app, chunk);
    }
    app.physics.spawn_player(PAWN_START);
    let settled = settle_pawn(&mut app);
    app.simulation
        .set_position_yaw_pitch(settled.position, 0.0, 0.0);
    app
}

/// Lets the pawn's body fall the last few centimetres and come to rest on the ground, as a
/// pawn standing for a while has, and returns where it rests. Moves the body through the
/// physics alone: the pawn's job loop is not involved.
fn settle_pawn(app: &mut App) -> moho_physics::CharacterState {
    const DT: f32 = 1.0 / 60.0;
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
    let idle = moho_physics::CharacterCommand::default();
    for _ in 0..120 {
        app.physics.move_character(&idle, &settings, DT);
        app.physics.step(DT);
    }
    let state = pawn_state(app);
    assert!(
        state.grounded,
        "setup: the pawn rests on the ground: {state:?}"
    );
    state
}

/// A loop at the game's tick rate, to step an app one tick at a time.
pub(crate) fn sim() -> HeadlessLoop<App> {
    HeadlessLoop::new(LoopConfig::new(flagstone_game::TICK_HZ))
}

/// The pawn's physics body as of the last tick.
pub(crate) fn pawn_state(app: &App) -> moho_physics::CharacterState {
    let handle = app.physics.player.expect("the pawn was spawned");
    app.physics
        .world
        .as_ref()
        .expect("physics world")
        .character(handle)
        .expect("the pawn exists")
}

/// Whether the grid holds a block at `pos`.
pub(crate) fn solid(app: &App, pos: [i32; 3]) -> bool {
    app.light_system
        .as_ref()
        .expect("light system")
        .grid()
        .is_solid_at(BlockPos::new(pos[0], pos[1], pos[2]))
}

/// Runs ticks until `done` holds after one, for at most `max_ticks`; the number of ticks run,
/// or `None` when `done` never held.
pub(crate) fn run_until(
    app: &mut App,
    sim: &mut HeadlessLoop<App>,
    max_ticks: u32,
    done: impl Fn(&App) -> bool,
) -> Option<u32> {
    for ticks in 1..=max_ticks {
        sim.step(app, 1);
        if done(app) {
            return Some(ticks);
        }
    }
    None
}
