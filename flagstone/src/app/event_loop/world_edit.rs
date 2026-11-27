//! Publishing the world edits a dug block causes, shared by the player's hand-mining and a
//! pawn's job.

use flagstone_game::inventory::Inventory;
use flagstone_game::pawn::MineOutcome;
use flagstone_voxel::{BlockChangeReason, VoxelGrid, WorldEvent};
use moho_core::EventBus;
use moho_core::events::AudioEvent;
use std::path::Path;

const DIG_VOLUME: f32 = 0.6;

/// Publishes the removal of the block `outcome` dug: `BlockRemoved`, then a remesh of every
/// chunk whose surface the block shapes.
pub(super) fn publish_removal(
    event_bus: &EventBus,
    grid: &VoxelGrid,
    outcome: &MineOutcome,
    reason: BlockChangeReason,
) {
    let dirty_chunks: Vec<_> =
        flagstone_voxel::chunks_meshed_from(outcome.block_pos, grid.chunk_size()).collect();

    event_bus.publish(WorldEvent::BlockRemoved {
        position: outcome.block_pos,
        old_material_id: outcome.old_material_id,
        reason,
    });
    for chunk_pos in dirty_chunks {
        event_bus.publish(WorldEvent::ChunkMeshDirty {
            chunk_pos,
            terrain_dirty: true,
            structure_dirty: true,
        });
    }
}

/// Applies a pawn's dig: the yield goes to the colony `stock`, then the removal and the dig
/// cue (the file at `dig_sound`) at the block's centre are published.
pub(crate) fn apply_pawn_dig(
    event_bus: &EventBus,
    stock: &mut Inventory,
    grid: &VoxelGrid,
    outcome: &MineOutcome,
    dig_sound: &Path,
) {
    if let Some(yield_) = outcome.yield_ {
        stock.add(yield_.resource_id, yield_.amount);
    }
    publish_removal(event_bus, grid, outcome, BlockChangeReason::Mechanic);
    event_bus.publish(AudioEvent::PlaySoundAt {
        path: dig_sound.to_string_lossy().into_owned(),
        volume: DIG_VOLUME,
        position: outcome.block_pos.as_vec3() + glam::Vec3::splat(0.5),
    });
}
