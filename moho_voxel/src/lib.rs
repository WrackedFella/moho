//! Voxel system for 3D block-based world representation.
//!
//! This module provides a complete voxel system with:
//! - Grid data structure for storing voxel blocks
//! - Mesh generation for voxel rendering
//! - Terrain smoothing algorithms
//! - Chunk-based organization
//!
//! # Module Organization
//! - `grid` - Core data structures (VoxelGrid, BlockData)
//! - `face` - Face directions (FaceDirection)
//! - `mesh` - Mesh generation (MeshGenerator)
//! - `chunk` - Chunk optimization (VoxelChunk)
//! - `modification` - Named block mutations (VoxelMutator)
//! - `materials` - Surface materials (MaterialType)
//! - `events` - Block and chunk events (WorldEvent, BlockChangeReason)

mod chunk;
mod chunk_store;
mod events;
mod face;
mod grid;
mod light_jobs;
mod light_propagation;
mod light_sky;
mod light_storage;
mod light_system;
mod materials;
mod mesh;
mod modification;
pub mod streaming;

// Re-export core types from submodules
pub use chunk::VoxelChunk;
pub use chunk_store::ChunkStore;
pub use events::{BlockChangeReason, WorldEvent};
pub use face::FaceDirection;
pub use grid::{BlockData, BlockPos, MaterialLighting, MaterialRegistry, ResourceData, VoxelGrid};
pub use light_jobs::{
    JobId, LightCancellationToken, LightFrameBudget, LightJobQueue, LightJobStats, LightUpdateJob,
    LightUpdateOp, LightUpdateResult,
};
pub use light_propagation::{LightChannel, LightPropagator};
pub use light_sky::{dirty_chunks_below, ensure_chunk_sky_ready, recompute_sky_exposure};
pub use light_storage::{CHUNK_SIZE as LIGHT_CHUNK_SIZE, ChunkLight};
pub use light_system::{LightSystem, LightSystemStats};
pub use materials::MaterialType;
pub use mesh::{BlockyMeshGenerator, ChunkContent, HybridMeshGenerator, MarchingCubes, VoxelMesh};
pub use modification::VoxelMutator;
pub use streaming::StreamingConfig;
