//! A strategy-view world for the tests of the pawn's mining job: smooth ground with real
//! meshes and colliders, one block marked for digging, and a pawn standing on the ground.

use crate::App;
use crate::app::world_geometry::insert_chunk;
use crate::game_state::GameState;
use flagstone_game::controller::CameraMode;
use flagstone_voxel::VoxelChunk;
use glam::{IVec3, Vec3};
use std::ops::RangeInclusive;

/// Top face of the ground: the height the pawn's feet stand at.
pub(crate) const GROUND_TOP: f32 = 4.0;

/// Half the capsule's height: the capsule centre stands this far above the feet.
const CENTRE_ABOVE_FEET: f32 = 1.15;

/// Where things are in a test world.
pub(crate) struct Layout {
    /// Where the pawn's feet start, on the ground.
    pub start: Vec3,
    /// Columns of `x` that stand one block above the rest of the ground, across every row.
    pub plateau: Option<RangeInclusive<i32>>,
    /// The block to dig.
    pub block: IVec3,
    /// Whether the block is iron ore (resource 1) or plain.
    pub ore: bool,
    /// Whether the block carries a mining mark.
    pub marked: bool,
}

/// Ground over blocks x `0..24`, y `0..4`, z `0..16`, meshed and collidable, with the pawn
/// settled on it in the strategy view and the game playing.
pub(crate) fn world(layout: &Layout) -> App {
    let mut app = App::headless();
    app.game_state = GameState::Playing;
    app.simulation.set_camera_mode(CameraMode::Isometric);
    let grid = app.light_system.as_mut().expect("light system").grid_mut();
    grid.mutator()
        .fill_region(IVec3::ZERO, IVec3::new(23, 3, 15), 0);
    if let Some(plateau) = &layout.plateau {
        grid.mutator().fill_region(
            IVec3::new(*plateau.start(), 4, 0),
            IVec3::new(*plateau.end(), 4, 15),
            0,
        );
    }
    grid.mutator()
        .place(layout.block, 0, layout.ore.then_some(1));
    let chunks: Vec<VoxelChunk> = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]
        .into_iter()
        .map(|pos| VoxelChunk::from_grid_lod(grid, pos, 0))
        .collect();
    for chunk in chunks {
        insert_chunk(&mut app, chunk);
    }
    if layout.marked {
        app.colony.marks.toggle(layout.block.to_array());
    }
    settle_pawn(&mut app, layout.start);
    app
}

fn settle_pawn(app: &mut App, feet: Vec3) {
    app.physics
        .spawn_player(feet + Vec3::Y * (CENTRE_ABOVE_FEET + 0.02));
    let idle = moho_physics::CharacterCommand::default();
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
    let dt = 1.0 / flagstone_game::TICK_HZ as f32;
    for _ in 0..120 {
        app.physics.move_character(&idle, &settings, dt);
        app.physics.step(dt);
    }
    let state = pawn_state(app);
    assert!(state.grounded, "the pawn settles on the ground: {state:?}");
    app.simulation
        .set_position_yaw_pitch(state.position, state.yaw, 0.0);
}

/// The pawn's character as the physics world holds it: `position` is the capsule centre.
pub(crate) fn pawn_state(app: &App) -> moho_physics::CharacterState {
    app.physics
        .world
        .as_ref()
        .expect("physics world")
        .character(app.physics.player.expect("the pawn has a character"))
        .expect("the character resolves")
}

/// Runs `n` ticks of the game loop.
pub(crate) fn tick(app: &mut App, n: u32) {
    moho_app::HeadlessLoop::new(moho_app::LoopConfig::new(flagstone_game::TICK_HZ)).step(app, n);
}

pub(crate) fn is_solid(app: &App, block: IVec3) -> bool {
    app.light_system
        .as_ref()
        .expect("light system")
        .grid()
        .is_solid_at(block)
}

/// Runs ticks until `block` is no longer solid, at most `limit`; the tick it went at.
pub(crate) fn run_until_dug(app: &mut App, block: IVec3, limit: u32) -> Option<u32> {
    (1..=limit).find(|_| {
        tick(app, 1);
        !is_solid(app, block)
    })
}
