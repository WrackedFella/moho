//! Chunk-based voxel organization for rendering optimization.
//!
//! Provides `VoxelChunk`: a merged mesh for a 16³ region of the world,
//! generated via `from_grid_hybrid` using the hybrid Marching-Cubes + blocky pipeline.

use super::grid::{BlockData, VoxelGrid};
use glam::IVec3;
use std::collections::HashMap;

/// Represents a chunk of voxel terrain with merged, optimized mesh
///
/// All blocks in the chunk are combined into a single mesh with face culling applied.
/// This dramatically reduces draw calls and improves rendering performance.
///
/// # Face Culling
/// Faces between adjacent solid blocks are automatically removed, reducing
/// vertex count by up to 80% for typical terrain.
///
/// # World Space Vertices
/// Chunk vertices are already transformed to world space, so the chunk
/// uses an identity transform when rendering.
#[derive(Clone, Debug)]
pub struct VoxelChunk {
    chunk_pos: IVec3,
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    ambient_occlusion: Vec<f32>,
    geometry_type: Vec<u32>,
    light_level: Vec<f32>,
    /// Per-vertex RGB block-light, each channel 0..=1 (raw value / 15).
    block_light_rgb: Vec<[f32; 3]>,
    /// Per-vertex sky-exposure factor (0.0 = underground, 1.0 = open sky).
    sky_exposed: Vec<f32>,
    indices: Vec<u32>,
    material_id: u32,
    /// LOD tier: 0 = full 16³ hybrid, 1 = coarse 8³ blocky
    lod: u8,
}

impl VoxelChunk {
    /// Construct a new VoxelChunk from raw mesh data.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        chunk_pos: IVec3,
        vertices: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        ambient_occlusion: Vec<f32>,
        geometry_type: Vec<u32>,
        light_level: Vec<f32>,
        block_light_rgb: Vec<[f32; 3]>,
        sky_exposed: Vec<f32>,
        indices: Vec<u32>,
        material_id: u32,
    ) -> Self {
        Self {
            chunk_pos,
            vertices,
            normals,
            ambient_occlusion,
            geometry_type,
            light_level,
            block_light_rgb,
            sky_exposed,
            indices,
            material_id,
            lod: 0,
        }
    }

    /// Create an empty chunk at the given position (useful for tests).
    pub fn empty(chunk_pos: IVec3) -> Self {
        Self::new(
            chunk_pos,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
        )
    }

    /// Generate chunk mesh at the given LOD tier.
    ///
    /// - `lod == 0`: full 16³ hybrid mesh (Marching Cubes + blocky), same as `from_grid_hybrid`
    /// - `lod == 1`: coarse 8³ blocky mesh (1 sample per 2-block cell), lower quality, cheaper
    pub fn from_grid_lod(grid: &VoxelGrid, chunk_pos: IVec3, lod: u8) -> Self {
        use super::mesh::HybridMeshGenerator;
        if lod == 0 {
            return Self::from_grid_hybrid(grid, chunk_pos);
        }

        let chunk_size = grid.chunk_size();
        let mesh = HybridMeshGenerator::generate_coarse_mesh(grid, chunk_pos, chunk_size);
        let blocks = grid.chunk_block_data(chunk_pos);
        let material_id = primary_material_id(&blocks);

        VoxelChunk {
            chunk_pos,
            vertices: mesh.vertices,
            normals: mesh.normals,
            ambient_occlusion: mesh.ambient_occlusion,
            geometry_type: mesh.geometry_type,
            light_level: mesh.light_level,
            block_light_rgb: mesh.block_light_rgb,
            sky_exposed: mesh.sky_exposed,
            indices: mesh.indices,
            material_id,
            lod,
        }
    }

    // --- Getters ---

    pub fn chunk_pos(&self) -> IVec3 {
        self.chunk_pos
    }
    pub fn lod(&self) -> u8 {
        self.lod
    }
    pub fn vertices(&self) -> &[[f32; 3]] {
        &self.vertices
    }
    pub fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }
    pub fn ambient_occlusion(&self) -> &[f32] {
        &self.ambient_occlusion
    }
    pub fn geometry_type(&self) -> &[u32] {
        &self.geometry_type
    }
    pub fn light_level(&self) -> &[f32] {
        &self.light_level
    }
    pub fn block_light_rgb(&self) -> &[[f32; 3]] {
        &self.block_light_rgb
    }
    pub fn sky_exposed(&self) -> &[f32] {
        &self.sky_exposed
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub fn material_id(&self) -> u32 {
        self.material_id
    }
}

impl VoxelChunk {
    /// Generate chunk mesh using the hybrid pipeline (Marching Cubes for smooth terrain,
    /// greedy meshing for blocky structures).
    ///
    /// Automatically detects smooth terrain vs blocky structures and generates
    /// appropriate meshes with ambient occlusion.
    ///
    /// # Arguments
    /// * `grid` - Source voxel grid containing blocks
    /// * `chunk_pos` - Chunk coordinates to generate mesh for
    ///
    /// # Returns
    /// A `VoxelChunk` with hybrid mesh generation applied
    pub fn from_grid_hybrid(grid: &VoxelGrid, chunk_pos: IVec3) -> Self {
        use super::mesh::HybridMeshGenerator;

        let chunk_size = grid.chunk_size();
        let mesh = HybridMeshGenerator::generate_chunk_mesh(grid, chunk_pos, chunk_size);

        let blocks = grid.chunk_block_data(chunk_pos);
        let material_id = primary_material_id(&blocks);

        VoxelChunk {
            chunk_pos,
            vertices: mesh.vertices,
            normals: mesh.normals,
            ambient_occlusion: mesh.ambient_occlusion,
            geometry_type: mesh.geometry_type,
            light_level: mesh.light_level,
            block_light_rgb: mesh.block_light_rgb,
            sky_exposed: mesh.sky_exposed,
            indices: mesh.indices,
            material_id,
            lod: 0,
        }
    }

    /// Check if chunk has any geometry
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Check if this chunk has geometry to render
    pub fn has_geometry(&self) -> bool {
        !self.vertices.is_empty() && !self.indices.is_empty()
    }
}

fn primary_material_id(blocks: &[BlockData]) -> u32 {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for block in blocks {
        *counts.entry(block.material_id).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, c)| c)
        .map_or(0, |(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk_with(vertices: usize, indices: Vec<u32>) -> VoxelChunk {
        VoxelChunk::new(
            IVec3::ZERO,
            vec![[0.0, 0.0, 0.0]; vertices],
            vec![[0.0, 0.0, 1.0]; vertices],
            vec![1.0; vertices],
            vec![1; vertices],
            vec![1.0; vertices],
            vec![[1.0, 1.0, 1.0]; vertices],
            vec![1.0; vertices],
            indices,
            0,
        )
    }

    #[test]
    fn test_chunk_with_geometry() {
        let no_indices = chunk_with(3, vec![]);
        let both = chunk_with(3, vec![0, 1, 2]);
        let neither = VoxelChunk::empty(IVec3::ZERO);

        assert!(!no_indices.has_geometry());
        assert!(!no_indices.is_empty());
        assert!(both.has_geometry());
        assert!(!both.is_empty());
        assert!(!neither.has_geometry());
        assert!(neither.is_empty());
    }

    #[test]
    fn test_from_grid_lod_stamps_lod_field() {
        let mut grid = VoxelGrid::new(16);
        grid.place_block(IVec3::new(1, 1, 1), 0, None);

        let fine = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0);
        let coarse = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 1);

        assert_eq!(fine.lod(), 0);
        assert_eq!(coarse.lod(), 1);
        assert!(coarse.has_geometry());
        let is_even_integer = |c: f32| c.fract() == 0.0 && c % 2.0 == 0.0;
        assert!(
            coarse
                .vertices()
                .iter()
                .flatten()
                .copied()
                .all(is_even_integer)
        );
        assert!(fine.vertices().iter().flatten().any(|c| c.fract() != 0.0));
    }

    fn lit_mixed_grid() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for pos in [
            IVec3::new(1, 1, 1),
            IVec3::new(2, 1, 1),
            IVec3::new(2, 2, 1),
        ] {
            grid.place_block(pos, 0, None);
        }
        for pos in [IVec3::new(8, 8, 8), IVec3::new(9, 8, 8)] {
            grid.place_block(pos, 100, None);
        }
        grid.set_block_light_rgb(IVec3::new(8, 8, 8), [6, 3, 1]);
        grid.set_sky_exposed(IVec3::new(8, 8, 8), true);
        grid.set_block_light_rgb(IVec3::new(9, 8, 8), [3, 9, 12]);
        grid
    }

    #[test]
    fn accessors_return_the_mesh_arrays() {
        let grid = lit_mixed_grid();
        let pos = IVec3::ZERO;
        let mesh = crate::mesh::HybridMeshGenerator::generate_chunk_mesh(&grid, pos, 16);

        let chunk = VoxelChunk::from_grid_hybrid(&grid, pos);

        assert!(mesh.vertices.len() > 1);
        assert_ne!(mesh.light_level, mesh.sky_exposed);
        assert_ne!(mesh.ambient_occlusion, mesh.light_level);
        assert_ne!(mesh.ambient_occlusion, mesh.sky_exposed);
        assert_eq!(chunk.chunk_pos(), pos);
        assert_eq!(chunk.vertices(), mesh.vertices.as_slice());
        assert_eq!(chunk.normals(), mesh.normals.as_slice());
        assert_eq!(chunk.ambient_occlusion(), mesh.ambient_occlusion.as_slice());
        assert_eq!(chunk.geometry_type(), mesh.geometry_type.as_slice());
        assert_eq!(chunk.light_level(), mesh.light_level.as_slice());
        assert_eq!(chunk.block_light_rgb(), mesh.block_light_rgb.as_slice());
        assert_eq!(chunk.sky_exposed(), mesh.sky_exposed.as_slice());
        assert_eq!(chunk.indices(), mesh.indices.as_slice());
    }

    #[test]
    fn accessors_return_the_constructor_arrays() {
        let chunk = VoxelChunk::new(
            IVec3::new(1, 2, 3),
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            vec![0.25, 0.5, 0.75],
            vec![3, 4, 5],
            vec![0.1, 0.2, 0.3],
            vec![[0.1, 0.2, 0.3], [0.4, 0.5, 0.6], [0.7, 0.8, 0.9]],
            vec![0.9, 0.8, 0.7],
            vec![0, 1, 2],
            42,
        );

        assert_eq!(chunk.chunk_pos(), IVec3::new(1, 2, 3));
        assert_eq!(chunk.normals()[1], [0.0, 1.0, 0.0]);
        assert_eq!(chunk.normals().len(), 3);
        assert_eq!(chunk.ambient_occlusion(), &[0.25, 0.5, 0.75]);
        assert_eq!(chunk.geometry_type(), &[3, 4, 5]);
        assert_eq!(chunk.light_level(), &[0.1, 0.2, 0.3]);
        assert_eq!(chunk.block_light_rgb()[2], [0.7, 0.8, 0.9]);
        assert_eq!(chunk.block_light_rgb().len(), 3);
        assert_eq!(chunk.sky_exposed(), &[0.9, 0.8, 0.7]);
        assert_eq!(chunk.indices(), &[0, 1, 2]);
        assert_eq!(chunk.material_id(), 42);
        assert_eq!(chunk.lod(), 0);
    }

    #[test]
    fn primary_material_is_the_most_common() {
        // The `+=` to `*=` mutant zeroes every count, leaving the pick to random HashMap order.
        for _ in 0..16 {
            let mut grid = VoxelGrid::new(16);
            for x in 0..3 {
                grid.place_block(IVec3::new(x, 0, 0), 7, None);
            }
            grid.place_block(IVec3::new(0, 0, 4), 2, None);
            grid.place_block(IVec3::new(1, 0, 4), 9, None);
            grid.place_block(IVec3::new(2, 0, 4), 11, None);
            grid.place_block(IVec3::new(3, 0, 4), 12, None);

            let hybrid = VoxelChunk::from_grid_hybrid(&grid, IVec3::ZERO);
            let coarse = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 1);

            assert_eq!(hybrid.material_id(), 7);
            assert_eq!(coarse.material_id(), 7);
        }
    }
}
