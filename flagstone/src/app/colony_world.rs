//! A strategy-view world for tests of the pawn's colony jobs: flat ground with real meshes and
//! physics under a playing app.

use crate::App;
use crate::app::world_geometry::insert_chunk;
use crate::game_state::GameState;
use flagstone_game::controller::CameraMode;
use flagstone_voxel::{BlockPos, VoxelChunk};
use glam::{IVec3, Vec3};

/// The ground fills y `0..GROUND_TOP`, so its top face is at `y == GROUND_TOP`.
pub(crate) const GROUND_TOP: i32 = 8;
/// The ground spans x and z `0..SIDE`.
pub(crate) const SIDE: i32 = 32;
/// The pawn's capsule centre above its feet.
const CENTRE_ABOVE_FEET: f32 = 1.2;

/// A playing strategy-view app over the ground with `edits` placed over it (a block with a
/// resource, or `None` for a plain one), the pawn standing with its feet at `feet`.
pub(crate) fn colony_app(edits: &[(BlockPos, Option<u32>)], feet: Vec3) -> App {
    let mut app = App::headless();
    app.game_state = GameState::Playing;
    app.simulation.set_camera_mode(CameraMode::Isometric);
    let grid = app.light_system.as_mut().expect("light system").grid_mut();
    grid.mutator().fill_region(
        IVec3::ZERO,
        IVec3::new(SIDE - 1, GROUND_TOP - 1, SIDE - 1),
        0,
    );
    for &(pos, resource) in edits {
        grid.mutator().place(pos, 0, resource);
    }
    let chunks: Vec<VoxelChunk> = (0..SIDE / 16)
        .flat_map(|x| (0..SIDE / 16).map(move |z| IVec3::new(x, 0, z)))
        .map(|pos| VoxelChunk::from_grid_lod(grid, pos, 0))
        .collect();
    for chunk in chunks {
        insert_chunk(&mut app, chunk);
    }
    let centre = feet + Vec3::Y * CENTRE_ABOVE_FEET;
    app.simulation.set_position_yaw_pitch(centre, 0.0, 0.0);
    app.physics.spawn_player(centre);
    app
}

/// The same world with `target` marked for mining.
pub(crate) fn marked_app(target: BlockPos, resource: Option<u32>, feet: Vec3) -> App {
    let mut app = colony_app(&[(target, resource)], feet);
    app.colony.marks.toggle(target.to_array());
    app
}

/// The pawn's physics character.
pub(crate) fn character(app: &App) -> moho_physics::CharacterState {
    app.physics
        .world
        .as_ref()
        .expect("physics world")
        .character(app.physics.player.expect("the pawn has a character"))
        .expect("the character resolves")
}

pub(crate) fn run(app: &mut App, ticks: u32) {
    moho_app::HeadlessLoop::new(moho_app::LoopConfig::new(flagstone_game::TICK_HZ))
        .step(app, ticks);
}

pub(crate) fn is_solid(app: &App, pos: BlockPos) -> bool {
    app.light_system
        .as_ref()
        .expect("light system")
        .grid()
        .is_solid_at(pos)
}

/// Ticks the app one at a time until `pos` is no longer solid; the tick count, or `None`
/// when it is still there after `max_ticks`.
pub(crate) fn run_until_removed(app: &mut App, pos: BlockPos, max_ticks: u32) -> Option<u32> {
    for tick in 1..=max_ticks {
        run(app, 1);
        if !is_solid(app, pos) {
            return Some(tick);
        }
    }
    None
}
