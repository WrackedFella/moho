//! Voxel system for 3D block-based world representation.
//!
//! This module provides a complete voxel system with:
//! - Grid data structure for storing voxel blocks
//! - Face culling for rendering optimization
//! - Mesh generation for voxel rendering
//! - Terrain smoothing algorithms
//! - Chunk-based organization
//! - State management for async mesh generation
//! - Block modification API with automatic state management
//! - Background mesh generation job queue
//!
//! # Module Organization
//! - `grid` - Core data structures (VoxelGrid, VoxelBlock)
//! - `face` - Face direction and culling logic
//! - `mesh` - Mesh generation (MeshGenerator)
//! - `chunk` - Chunk optimization (VoxelChunk)
//! - `state` - Chunk state tracking for async operations
//! - `modification` - High-level block modification API
//! - `jobs` - Background mesh generation job queue
//!
//! # Examples
//! ```ignore
//! use moho_core::voxel::{VoxelGrid, BlockPos, BlockModifier};
//! use moho_core::events::EventBus;
//! use std::sync::Arc;
//!
//! let grid = VoxelGrid::new(16);
//! let event_bus = Arc::new(EventBus::new());
//! let mut modifier = BlockModifier::new(grid, event_bus);
//!
//! let pos = BlockPos::new(0, 0, 0);
//! modifier.set_block(pos, 0, None);
//! ```

mod biome;
mod chunk;
pub mod streaming;
mod face;
mod grid;
mod jobs;
mod light_jobs;
mod light_propagation;
mod light_sky;
mod light_storage;
mod light_system;
mod mesh;
mod modification;
mod state;

// Re-export core types from submodules
pub use biome::{BiomeMap, BiomeParams, BiomeType, OreLayout};
pub use chunk::VoxelChunk;
pub use face::{FaceDirection, get_visible_faces};
pub use grid::{
    BlockCategory, BlockData, BlockPos, MaterialLighting, MaterialRegistry, ResourceData,
    ResourceRegistry, VoxelBlock, VoxelGrid,
};
pub use light_sky::{dirty_chunks_below, ensure_chunk_sky_ready, recompute_sky_exposure};
pub use light_storage::{CHUNK_SIZE as LIGHT_CHUNK_SIZE, ChunkLight};
pub use jobs::{
    CancellationToken, MeshGeneratorFn, MeshJob, MeshJobQueue, MeshJobQueueBuilder, MeshJobResult,
    MeshJobType, create_hybrid_generator,
};
pub use light_jobs::{
    LightCancellationToken, LightFrameBudget, LightJobQueue, LightJobStats, LightUpdateJob,
    LightUpdateOp, LightUpdateResult,
};
pub use light_propagation::{LightChannel, LightPropagator};
pub use light_system::{LightSystem, LightSystemStats};
pub use mesh::{BlockyMeshGenerator, ChunkContent, HybridMeshGenerator, MarchingCubes, VoxelMesh};
pub use modification::{BlockModifier, ModificationResult, VoxelMutator};
pub use streaming::StreamingConfig;
pub use state::{ChunkState, DirtyFlags, GenerationState, JobId};
