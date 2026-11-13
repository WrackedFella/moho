//! Test fixture: a flat voxel field with collision meshes, and the strategy-view pawn standing
//! on it, for the tests that drive the real tick loop.

use crate::App;
use crate::app::world_geometry::insert_chunk;
use crate::game_state::GameState;
use flagstone_game::controller::CameraMode;
use flagstone_voxel::{BlockPos, VoxelChunk};
use glam::{IVec3, Vec3};
use moho_app::{HeadlessLoop, LoopConfig};

/// Top block layer of the field; feet stand at `GROUND_TOP + 1`.
pub(crate) const GROUND_TOP: i32 = 7;
/// Material of the field and of every block placed on it.
const STONE: u32 = 0;
pub(crate) const PAWN_CELL: [i32; 3] = [12, GROUND_TOP + 1, 12];
/// The capsule centre is this far above the feet.
const CENTRE_HEIGHT: f32 = 1.15;
/// The pawn starts this far above the ground: a character resting exactly on a mesh does not
/// move, and the first walking tick settles it.
const START_CLEARANCE: f32 = 0.05;

/// A block standing on the field, with the resource it carries.
pub(crate) type Block = ([i32; 3], Option<u32>);

/// A one-block-high plateau across the whole field, over the columns `x_range`.
pub(crate) fn rise(x_range: std::ops::RangeInclusive<i32>) -> Vec<Block> {
    x_range
        .flat_map(|x| (0..32).map(move |z| ([x, GROUND_TOP + 1, z], None)))
        .collect()
}

/// A playing strategy view: ground over blocks x, z `0..32` and y `0..=GROUND_TOP`, the
/// `blocks` on top of it, all meshed into the physics world, and the pawn's character
/// standing in `PAWN_CELL` and facing +Z.
pub(crate) fn dig_site(blocks: &[Block]) -> App {
    let mut app = App::headless();
    app.game_state = GameState::Playing;
    let grid = app.light_system.as_mut().expect("light system").grid_mut();
    grid.mutator()
        .fill_region(IVec3::ZERO, IVec3::new(31, GROUND_TOP, 31), STONE);
    for (pos, resource) in blocks {
        grid.mutator()
            .place(BlockPos::from_array(*pos), STONE, *resource);
    }
    let chunks: Vec<VoxelChunk> = (0..2)
        .flat_map(|x| (0..2).map(move |z| IVec3::new(x, 0, z)))
        .map(|pos| VoxelChunk::from_grid_lod(grid, pos, 0))
        .collect();
    for chunk in chunks {
        insert_chunk(&mut app, chunk);
    }
    let start = pawn_start();
    app.simulation.set_camera_mode(CameraMode::Isometric);
    app.simulation.set_position_yaw_pitch(start, 0.0, 0.0);
    app.physics.spawn_player(start);
    app.physics.step(1.0 / 60.0);
    app
}

pub(crate) fn pawn_start() -> Vec3 {
    Vec3::new(
        PAWN_CELL[0] as f32 + 0.5,
        PAWN_CELL[1] as f32 + CENTRE_HEIGHT + START_CLEARANCE,
        PAWN_CELL[2] as f32 + 0.5,
    )
}

pub(crate) fn pawn_state(app: &App) -> moho_physics::CharacterState {
    app.physics
        .world
        .as_ref()
        .expect("physics world")
        .character(app.physics.player.expect("the site spawns the pawn"))
        .expect("the pawn resolves")
}

pub(crate) fn block_exists(app: &App, pos: [i32; 3]) -> bool {
    app.light_system
        .as_ref()
        .expect("light system")
        .grid()
        .is_solid_at(BlockPos::from_array(pos))
}

pub(crate) fn headless_loop() -> HeadlessLoop<App> {
    HeadlessLoop::new(LoopConfig::new(flagstone_game::TICK_HZ))
}

/// Ticks `app` one at a time until `done` holds or `limit` ticks have run; returns whether
/// `done` held. `done` is also checked before the first tick.
pub(crate) fn run_until(app: &mut App, limit: u32, done: impl Fn(&App) -> bool) -> bool {
    let mut sim = headless_loop();
    for _ in 0..limit {
        if done(app) {
            return true;
        }
        sim.step(app, 1);
    }
    done(app)
}
