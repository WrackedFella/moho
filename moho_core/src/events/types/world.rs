use crate::events::Event;
use glam::IVec3;
use std::any::Any;

/// Reason a block was modified (for gameplay/analytics)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockChangeReason {
    /// Player placed or removed the block
    Player,
    /// World generation created this block
    WorldGen,
    /// Explosion destroyed or modified blocks
    Explosion,
    /// Physics simulation (falling blocks, water flow)
    Physics,
    /// Game mechanic (growth, decay, etc.)
    Mechanic,
    /// Unknown or unspecified reason
    Unknown,
}

/// World block modification events.
///
/// Only variants that are actually published are defined here.
/// Add new variants as systems that produce them are implemented.
#[derive(Clone, Debug)]
pub enum WorldEvent {
    /// Single block placed
    BlockPlaced {
        position: IVec3,
        material_id: u32,
        reason: BlockChangeReason,
    },

    /// Single block removed
    BlockRemoved {
        position: IVec3,
        old_material_id: u32,
        reason: BlockChangeReason,
    },

    /// Batch of blocks modified (for bulk operations like explosions)
    BlocksBatchModified {
        chunk_pos: IVec3,
        positions: Vec<IVec3>,
        reason: BlockChangeReason,
    },

    /// Chunk mesh needs regeneration
    ChunkMeshDirty {
        chunk_pos: IVec3,
        terrain_dirty: bool,
        structure_dirty: bool,
    },
}

impl Event for WorldEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
