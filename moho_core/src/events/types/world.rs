use crate::events::Event;
use glam::{IVec2, IVec3, Vec3};
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

/// World generation and modification events
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

    /// Chunk mesh generation started
    ChunkMeshGenerating { chunk_pos: IVec3, job_id: u64 },

    /// Chunk mesh generation completed
    ChunkMeshReady { chunk_pos: IVec3, job_id: u64 },

    /// Chunk mesh swapped to GPU
    ChunkMeshSwapped { chunk_pos: IVec3 },

    /// Light levels need recalculation
    LightDirty {
        chunk_pos: IVec3,
        affected_positions: Vec<IVec3>,
    },

    /// Chunk generated (legacy, for compatibility)
    ChunkGenerated { chunk_pos: IVec2 },

    /// Chunk modified (legacy, for compatibility)
    ChunkModified {
        chunk_pos: IVec2,
        voxel_changes: u32,
    },

    /// Biome changed
    BiomeChanged {
        old_biome: BiomeType,
        new_biome: BiomeType,
    },

    /// Cave discovered
    CaveDiscovered { entrance_pos: Vec3 },

    /// Material placed (legacy, prefer BlockPlaced)
    MaterialPlaced {
        position: IVec3,
        material: MaterialType,
    },

    /// World generation started
    GenerationStarted { seed: u64, size: u32 },

    /// World generation progress
    GenerationProgress { percent: f32 },

    /// World generation completed
    GenerationCompleted { seed: u64, total_voxels: u64 },
}

#[derive(Clone, Debug)]
pub enum BiomeType {
    Plains,
    Forest,
    Desert,
    Mountains,
    Ocean,
}

#[derive(Clone, Debug)]
pub enum MaterialType {
    Air,
    Stone,
    Dirt,
    Grass,
    Sand,
    Water,
    Ore(OreType),
}

#[derive(Clone, Debug)]
pub enum OreType {
    Coal,
    Iron,
    Gold,
    Diamond,
}

impl Event for WorldEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn should_record(&self) -> bool {
        // Don't record frequent/internal events
        !matches!(
            self,
            WorldEvent::GenerationProgress { .. }
                | WorldEvent::ChunkMeshGenerating { .. }
                | WorldEvent::ChunkMeshReady { .. }
                | WorldEvent::ChunkMeshSwapped { .. }
        )
    }
}
