//! A strategy-view world of real blocks, chunk meshes and a physics pawn, for the tests that
//! watch the colony work.

use crate::App;
use crate::game_state::GameState;
use flagstone_game::controller::CameraMode;
use flagstone_voxel::{BlockPos, WorldEvent};

/// Blocks along each side of the floor.
pub(crate) const SIDE: i32 = 32;
/// The floor's surface height: the top face of its highest block.
pub(crate) const FLOOR_TOP: i32 = 4;
/// Material of every block the fixtures place.
pub(crate) const STONE: u32 = 1;
/// Resource the ore blocks carry.
pub(crate) const ORE: u32 = 7;
/// The capsule centre's height above the feet.
pub(crate) const CENTRE_HEIGHT: f32 = 1.15;

/// Where the pawn's capsule centre starts: standing on the floor, facing +Z.
pub(crate) fn pawn_start() -> glam::Vec3 {
    glam::Vec3::new(6.5, FLOOR_TOP as f32 + CENTRE_HEIGHT + 0.05, 16.5)
}

/// A playing strategy view over a flat floor, meshed, with the pawn standing on it.
pub(crate) fn flat_world() -> App {
    let mut app = App::headless();
    app.game_state = GameState::Playing;
    app.simulation.set_camera_mode(CameraMode::Isometric);
    let grid = app
        .light_system
        .as_mut()
        .expect("App starts with a light system")
        .grid_mut();
    for x in 0..SIDE {
        for z in 0..SIDE {
            for y in FLOOR_TOP - 2..FLOOR_TOP {
                grid.mutator().place(BlockPos::new(x, y, z), STONE, None);
            }
        }
    }
    remesh(&mut app);
    app.physics.spawn_player(pawn_start());
    app.simulation
        .set_position_yaw_pitch(pawn_start(), 0.0, 0.0);
    app
}

/// Places a block, as world generation would, and meshes it.
pub(crate) fn place(app: &mut App, pos: BlockPos, resource: Option<u32>) {
    app.light_system
        .as_mut()
        .expect("App starts with a light system")
        .grid_mut()
        .mutator()
        .place(pos, STONE, resource);
    remesh(app);
}

/// Raises a one-block rise across the whole floor: every column with `x` in `xs` gets a block
/// on top of the floor.
pub(crate) fn raise_a_rise(app: &mut App, xs: std::ops::RangeInclusive<i32>) {
    let grid = app
        .light_system
        .as_mut()
        .expect("App starts with a light system")
        .grid_mut();
    for x in xs {
        for z in 0..SIDE {
            grid.mutator()
                .place(BlockPos::new(x, FLOOR_TOP, z), STONE, None);
        }
    }
    remesh(app);
}

/// Marks the block as the player's designation would.
pub(crate) fn mark(app: &mut App, pos: BlockPos) {
    app.colony.marks.toggle(pos.to_array());
}

/// Whether the grid still holds a solid block at `pos`.
pub(crate) fn is_solid(app: &App, pos: BlockPos) -> bool {
    app.light_system
        .as_ref()
        .expect("App starts with a light system")
        .grid()
        .is_solid_at(pos)
}

/// The pawn's physics state.
pub(crate) fn pawn_state(app: &App) -> moho_physics::CharacterState {
    let handle = app.physics.player.expect("the pawn was spawned");
    app.physics
        .world
        .as_ref()
        .expect("physics world")
        .character(handle)
        .expect("the pawn exists")
}

/// Meshes every chunk of the floor into the scene and the physics world.
fn remesh(app: &mut App) {
    let chunk = app
        .light_system
        .as_ref()
        .expect("App starts with a light system")
        .grid()
        .chunk_size();
    for cx in 0..SIDE / chunk {
        for cz in 0..SIDE / chunk {
            app.event_bus.publish(WorldEvent::ChunkMeshDirty {
                chunk_pos: glam::IVec3::new(cx, 0, cz),
                terrain_dirty: true,
                structure_dirty: true,
            });
        }
    }
    crate::app::event_loop::event_processor::EventProcessor::new().process_world_events(app);
}
