//! Test fixture: a meshed block world with a physics pawn and one marked block, driven
//! tick by tick through the real game loop.

use crate::App;
use crate::app::event_loop::EventProcessor;
use crate::game_state::GameState;
use flagstone_game::controller::CameraMode;
use flagstone_voxel::{BlockPos, WorldEvent};
use moho_app::{HeadlessLoop, LoopConfig};

/// Y of the floor blocks' top face, which is where a pawn's feet rest.
pub(crate) const GROUND_Y: f32 = 70.0;
/// The capsule centre (the eye) above the feet.
pub(crate) const EYE_HEIGHT: f32 = 1.15;
/// Ticks a walk and dig may take before a test calls the pawn stuck: 20 s of game time.
pub(crate) const BUDGET_TICKS: u32 = 1200;

const FLOOR_BLOCK_Y: i32 = 69;
const PLATEAU_X: std::ops::RangeInclusive<i32> = 5..=7;

/// The feet position of a pawn standing on the floor at column `(x, z)`.
pub(crate) fn feet_at(x: f32, z: f32) -> glam::Vec3 {
    glam::Vec3::new(x, GROUND_Y, z)
}

/// A playing strategy-view app on a 16 x 16 floor of blocks (chunk `(0, 4, 0)`), meshed and
/// collidable, with its physics pawn standing at `feet` and `block` placed (with `resource`)
/// and marked. With `plateau`, a one-block-high wall spans the floor across `x` 5 to 7.
pub(crate) fn colony_app(
    feet: glam::Vec3,
    plateau: bool,
    block: [i32; 3],
    resource: Option<u32>,
) -> App {
    let mut app = App::headless();
    app.game_state = GameState::Playing;
    app.simulation.set_camera_mode(CameraMode::Isometric);
    let centre = feet + glam::Vec3::Y * EYE_HEIGHT;
    app.simulation.set_position_yaw_pitch(centre, 0.0, 0.0);
    let grid = app
        .light_system
        .as_mut()
        .expect("App starts with a light system")
        .grid_mut();
    for x in 0..16 {
        for z in 0..16 {
            grid.mutator()
                .place(BlockPos::new(x, FLOOR_BLOCK_Y, z), 1, None);
            if plateau && PLATEAU_X.contains(&x) {
                grid.mutator()
                    .place(BlockPos::new(x, FLOOR_BLOCK_Y + 1, z), 1, None);
            }
        }
    }
    grid.mutator().place(BlockPos::from(block), 1, resource);
    app.event_bus.publish(WorldEvent::ChunkMeshDirty {
        chunk_pos: glam::IVec3::new(0, 4, 0),
        terrain_dirty: true,
        structure_dirty: true,
    });
    EventProcessor::new().process_world_events(&mut app);
    app.physics.spawn_player(centre + glam::Vec3::Y * 0.05);
    app.colony.marks.toggle(block);
    app
}

pub(crate) fn ticker() -> HeadlessLoop<App> {
    HeadlessLoop::new(LoopConfig::new(flagstone_game::TICK_HZ))
}

pub(crate) fn is_solid(app: &App, block: [i32; 3]) -> bool {
    app.light_system
        .as_ref()
        .expect("light system")
        .grid()
        .is_solid_at(BlockPos::from(block))
}

/// The physics pawn's state: its position is the capsule centre, the eye.
pub(crate) fn character(app: &App) -> moho_physics::CharacterState {
    app.physics
        .world
        .as_ref()
        .expect("physics world")
        .character(app.physics.player.expect("the pawn has a character"))
        .expect("the character resolves")
}

/// Ticks until `block` is gone from the grid, calling `observe` with the tick number (from 0)
/// after each tick; returns the ticks run, or `None` when `BUDGET_TICKS` pass first.
pub(crate) fn run_until_removed(
    app: &mut App,
    sim: &mut HeadlessLoop<App>,
    block: [i32; 3],
    mut observe: impl FnMut(&App, u32),
) -> Option<u32> {
    for tick in 0..BUDGET_TICKS {
        sim.step(app, 1);
        observe(app, tick);
        if !is_solid(app, block) {
            return Some(tick + 1);
        }
    }
    None
}
