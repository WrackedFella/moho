use crate::events::Event;
use glam::{IVec2, IVec3, Vec3};
use std::any::Any;

/// World generation and modification events
#[derive(Clone, Debug)]
pub enum WorldEvent {
    /// Chunk generated
    ChunkGenerated { chunk_pos: IVec2 },

    /// Chunk modified
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

    /// Material placed
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
        // Don't record frequent progress updates
        !matches!(self, WorldEvent::GenerationProgress { .. })
    }
}
