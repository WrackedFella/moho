//! Voxel system for 3D block-based world representation.
//!
//! This module provides a complete voxel system with:
//! - Grid data structure for storing voxel blocks
//! - Face culling for rendering optimization
//! - Mesh generation for voxel rendering
//! - Terrain smoothing algorithms
//! - Chunk-based organization
//!
//! # Module Organization
//! - `grid` - Core data structures (VoxelGrid, VoxelBlock)
//! - `face` - Face direction and culling logic
//! - `mesh` - Mesh generation (MeshGenerator)
//! - `chunk` - Chunk optimization (VoxelChunk, TerrainSmoother)
//!
//! # Examples
//! ```ignore
//! use moho_core::voxel::{VoxelGrid, VoxelBlock, BlockPos};
//!
//! let mut grid = VoxelGrid::new(16); // 16x16x16 chunks
//! let pos = BlockPos::new(0, 0, 0);
//! let block = VoxelBlock::new(pos, 0); // material_id = 0 (grass)
//! grid.set_block(pos, block);
//! ```

mod chunk;
mod face;
mod grid;
mod mesh;

// Re-export core types from submodules
pub use chunk::{TerrainSmoother, VoxelChunk};
pub use face::{FaceDirection, get_visible_faces};
pub use grid::{
    BlockPos, MaterialRegistry, ResourceData, ResourceRegistry, VoxelBlock, VoxelGrid, VoxelMesh,
};
pub use mesh::MeshGenerator;
