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
//! - `chunk` - Chunk optimization (VoxelChunk, TerrainSmoother)
//! - `state` - Chunk state tracking for async operations
//! - `modification` - High-level block modification API
//! - `jobs` - Background mesh generation job queue
//!
//! # Examples
//! ```ignore
//! use moho_core::voxel::{VoxelGrid, VoxelBlock, BlockPos, BlockModifier};
//! use moho_core::events::EventBus;
//! use std::sync::Arc;
//!
//! let grid = VoxelGrid::new(16);
//! let event_bus = Arc::new(EventBus::new());
//! let mut modifier = BlockModifier::new(grid, event_bus);
//!
//! let pos = BlockPos::new(0, 0, 0);
//! modifier.set_block(pos, VoxelBlock::new(pos, 0));
//! ```

mod chunk;
mod face;
mod grid;
mod jobs;
mod mesh;
mod modification;
mod state;

// Re-export core types from submodules
pub use chunk::{TerrainSmoother, VoxelChunk};
pub use face::{FaceDirection, get_visible_faces};
pub use grid::{
    BlockPos, MaterialRegistry, ResourceData, ResourceRegistry, VoxelBlock, VoxelGrid, VoxelMesh,
};
pub use jobs::{
    CancellationToken, MeshGeneratorFn, MeshJob, MeshJobQueue, MeshJobQueueBuilder, MeshJobResult,
    MeshJobType,
};
pub use mesh::MeshGenerator;
pub use modification::{BlockModifier, ModificationResult};
pub use state::{ChunkState, DirtyFlags, GenerationState, JobId};
