//! Hybrid mesh generation for chunks containing both smooth and blocky geometry.
//!
//! This module detects mixed chunks and generates appropriate meshes for each
//! geometry type, then concatenates them into a unified mesh for rendering.

use super::super::grid::{BlockPos, VoxelGrid, VoxelMesh};
use super::blocky::BlockyMeshGenerator;
use super::marching_cubes::MarchingCubes;
use glam::IVec3;

/// Classifies the content of a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkContent {
    /// All blocks are empty (no mesh needed)
    Empty,
    /// All blocks are smooth terrain
    AllSmooth,
    /// All blocks are blocky structures
    AllBlocky,
    /// Contains both smooth and blocky blocks
    Mixed,
}

/// Hybrid mesh generator supporting both smooth and blocky geometry
pub struct HybridMeshGenerator;

impl HybridMeshGenerator {
    /// Analyze chunk content to determine mesh generation strategy
    ///
    /// # Arguments
    /// * `grid` - The voxel grid
    /// * `chunk_pos` - Chunk position in chunk coordinates
    /// * `chunk_size` - Size of chunks (typically 16)
    ///
    /// # Returns
    /// Classification of chunk content and counts of each type
    pub fn analyze_chunk(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
    ) -> (ChunkContent, usize, usize) {
        let base_pos = chunk_pos * chunk_size;
        let mut smooth_count = 0;
        let mut blocky_count = 0;

        // Scan all blocks in chunk
        for x in 0..chunk_size {
            for y in 0..chunk_size {
                for z in 0..chunk_size {
                    let pos = base_pos + IVec3::new(x, y, z);
                    if let Some(block) = grid.get_block(&pos) {
                        if block.is_smooth() {
                            smooth_count += 1;
                        } else {
                            blocky_count += 1;
                        }
                    }
                }
            }
        }

        let content = if smooth_count == 0 && blocky_count == 0 {
            ChunkContent::Empty
        } else if blocky_count == 0 {
            ChunkContent::AllSmooth
        } else if smooth_count == 0 {
            ChunkContent::AllBlocky
        } else {
            ChunkContent::Mixed
        };

        (content, smooth_count, blocky_count)
    }

    /// Generate a mesh for a chunk, automatically selecting appropriate strategy
    ///
    /// # Arguments
    /// * `grid` - The voxel grid
    /// * `chunk_pos` - Chunk position in chunk coordinates
    /// * `chunk_size` - Size of chunks (typically 16)
    ///
    /// # Returns
    /// A VoxelMesh combining smooth and blocky geometry as appropriate
    pub fn generate_chunk_mesh(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
    ) -> VoxelMesh {
        let (content, _smooth_count, _blocky_count) =
            Self::analyze_chunk(grid, chunk_pos, chunk_size);

        match content {
            ChunkContent::Empty => VoxelMesh::empty(),

            ChunkContent::AllSmooth => {
                Self::generate_smooth_mesh(grid, chunk_pos, chunk_size)
            }

            ChunkContent::AllBlocky => {
                Self::generate_blocky_mesh(grid, chunk_pos, chunk_size)
            }

            ChunkContent::Mixed => {
                Self::generate_mixed_mesh(grid, chunk_pos, chunk_size)
            }
        }
    }

    /// Generate mesh using Marching Cubes for smooth terrain
    fn generate_smooth_mesh(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
    ) -> VoxelMesh {
        // Create density field from blocks
        let density_field = Self::create_density_field_for_chunk(grid, chunk_pos, chunk_size);

        // Generate mesh using Marching Cubes
        let mut mesh = MarchingCubes::generate_mesh(&density_field, chunk_size as usize);

        // Transform vertices to world space
        let base_pos = chunk_pos * chunk_size;
        for vertex in mesh.vertices.iter_mut() {
            vertex[0] += base_pos.x as f32;
            vertex[1] += base_pos.y as f32;
            vertex[2] += base_pos.z as f32;
        }

        mesh
    }

    /// Generate mesh using greedy meshing for blocky structures
    fn generate_blocky_mesh(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
    ) -> VoxelMesh {
        let mut mesh = VoxelMesh::empty();
        let base_pos = chunk_pos * chunk_size;

        // Generate mesh for each block in chunk
        for x in 0..chunk_size {
            for y in 0..chunk_size {
                for z in 0..chunk_size {
                    let pos = base_pos + IVec3::new(x, y, z);
                    if grid.has_block_at(&pos) {
                        let block_mesh = BlockyMeshGenerator::generate_mesh(grid, pos);
                        Self::append_mesh(&mut mesh, &block_mesh, pos);
                    }
                }
            }
        }

        mesh
    }

    /// Generate mesh for mixed chunk (smooth + blocky)
    fn generate_mixed_mesh(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
    ) -> VoxelMesh {
        let mut final_mesh = VoxelMesh::empty();
        let base_pos = chunk_pos * chunk_size;

        // Generate smooth terrain mesh (only for smooth blocks)
        let smooth_density = Self::create_selective_density_field(
            grid,
            chunk_pos,
            chunk_size,
            true, // only smooth blocks
        );
        let smooth_mesh = MarchingCubes::generate_mesh(&smooth_density, chunk_size as usize);

        // Append smooth mesh with world space transform
        for i in 0..smooth_mesh.vertices.len() {
            let mut vertex = smooth_mesh.vertices[i];
            vertex[0] += base_pos.x as f32;
            vertex[1] += base_pos.y as f32;
            vertex[2] += base_pos.z as f32;
            final_mesh.vertices.push(vertex);
            final_mesh.normals.push(smooth_mesh.normals[i]);
            final_mesh.ambient_occlusion.push(smooth_mesh.ambient_occlusion[i]);
            final_mesh.geometry_type.push(smooth_mesh.geometry_type[i]);
        }

        let smooth_index_offset = smooth_mesh.vertices.len() as u32;
        for &index in &smooth_mesh.indices {
            final_mesh.indices.push(index);
        }

        // Generate blocky structure meshes (only for blocky blocks)
        for x in 0..chunk_size {
            for y in 0..chunk_size {
                for z in 0..chunk_size {
                    let pos = base_pos + IVec3::new(x, y, z);
                    if let Some(block) = grid.get_block(&pos) {
                        if !block.is_smooth() {
                            let block_mesh = BlockyMeshGenerator::generate_mesh(grid, pos);
                            Self::append_mesh_with_offset(
                                &mut final_mesh,
                                &block_mesh,
                                pos,
                                smooth_index_offset,
                            );
                        }
                    }
                }
            }
        }

        final_mesh
    }

    /// Create density field for entire chunk (all smooth blocks)
    fn create_density_field_for_chunk(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
    ) -> [[[f32; 18]; 18]; 18] {
        Self::create_selective_density_field(grid, chunk_pos, chunk_size, false)
    }

    /// Create density field with optional filtering
    ///
    /// # Arguments
    /// * `only_smooth` - If true, only include smooth blocks in density
    fn create_selective_density_field(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
        only_smooth: bool,
    ) -> [[[f32; 18]; 18]; 18] {
        let mut field = [[[0.0f32; 18]; 18]; 18];
        let base_pos = chunk_pos * chunk_size;

        // Sample with padding (-1 to chunk_size+1)
        for x in 0..18 {
            for y in 0..18 {
                for z in 0..18 {
                    let world_pos = base_pos + IVec3::new(x as i32 - 1, y as i32 - 1, z as i32 - 1);
                    
                    let is_solid = if let Some(block) = grid.get_block(&world_pos) {
                        if only_smooth {
                            block.is_smooth()
                        } else {
                            true
                        }
                    } else {
                        false
                    };

                    field[x][y][z] = if is_solid { 1.0 } else { 0.0 };
                }
            }
        }

        field
    }

    /// Append a block mesh to the chunk mesh with world space transform
    fn append_mesh(target: &mut VoxelMesh, source: &VoxelMesh, block_pos: BlockPos) {
        let base_index = target.vertices.len() as u32;

        // Add vertices with world space offset
        for &vertex in &source.vertices {
            target.vertices.push([
                vertex[0] + block_pos.x as f32,
                vertex[1] + block_pos.y as f32,
                vertex[2] + block_pos.z as f32,
            ]);
        }

        // Copy normals, AO, and geometry type
        target.normals.extend_from_slice(&source.normals);
        target.ambient_occlusion.extend_from_slice(&source.ambient_occlusion);
        target.geometry_type.extend_from_slice(&source.geometry_type);

        // Add indices with offset
        for &index in &source.indices {
            target.indices.push(base_index + index);
        }
    }

    /// Append mesh with additional index offset (for mixed meshes)
    fn append_mesh_with_offset(
        target: &mut VoxelMesh,
        source: &VoxelMesh,
        block_pos: BlockPos,
        index_offset: u32,
    ) {
        let base_index = target.vertices.len() as u32;

        // Add vertices with world space offset
        for &vertex in &source.vertices {
            target.vertices.push([
                vertex[0] + block_pos.x as f32,
                vertex[1] + block_pos.y as f32,
                vertex[2] + block_pos.z as f32,
            ]);
        }

        // Copy normals, AO, and geometry type
        target.normals.extend_from_slice(&source.normals);
        target.ambient_occlusion.extend_from_slice(&source.ambient_occlusion);
        target.geometry_type.extend_from_slice(&source.geometry_type);

        // Add indices with combined offset
        for &index in &source.indices {
            target.indices.push(index_offset + base_index + index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::grid::VoxelBlock;

    #[test]
    fn test_analyze_empty_chunk() {
        let grid = VoxelGrid::new(16);
        let (content, smooth, blocky) = HybridMeshGenerator::analyze_chunk(&grid, IVec3::ZERO, 16);

        assert_eq!(content, ChunkContent::Empty);
        assert_eq!(smooth, 0);
        assert_eq!(blocky, 0);
    }

    #[test]
    fn test_analyze_all_smooth_chunk() {
        let mut grid = VoxelGrid::new(16);
        
        // Add smooth blocks (material ID < 100)
        for x in 0..4 {
            for y in 0..4 {
                for z in 0..4 {
                    let pos = IVec3::new(x, y, z);
                    grid.set_block(pos, VoxelBlock::new(pos, 0)); // Material 0 is smooth
                }
            }
        }

        let (content, smooth, blocky) = HybridMeshGenerator::analyze_chunk(&grid, IVec3::ZERO, 16);

        assert_eq!(content, ChunkContent::AllSmooth);
        assert_eq!(smooth, 64);
        assert_eq!(blocky, 0);
    }

    #[test]
    fn test_analyze_all_blocky_chunk() {
        let mut grid = VoxelGrid::new(16);
        
        // Add blocky blocks (material ID >= 100)
        for x in 0..4 {
            for y in 0..4 {
                for z in 0..4 {
                    let pos = IVec3::new(x, y, z);
                    grid.set_block(pos, VoxelBlock::new(pos, 100)); // Material 100 is blocky
                }
            }
        }

        let (content, smooth, blocky) = HybridMeshGenerator::analyze_chunk(&grid, IVec3::ZERO, 16);

        assert_eq!(content, ChunkContent::AllBlocky);
        assert_eq!(smooth, 0);
        assert_eq!(blocky, 64);
    }

    #[test]
    fn test_analyze_mixed_chunk() {
        let mut grid = VoxelGrid::new(16);
        
        // Add mix of smooth and blocky blocks
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    let pos = IVec3::new(x, y, z);
                    grid.set_block(pos, VoxelBlock::new(pos, 0)); // Smooth
                }
            }
        }

        for x in 2..4 {
            for y in 2..4 {
                for z in 2..4 {
                    let pos = IVec3::new(x, y, z);
                    grid.set_block(pos, VoxelBlock::new(pos, 100)); // Blocky
                }
            }
        }

        let (content, smooth, blocky) = HybridMeshGenerator::analyze_chunk(&grid, IVec3::ZERO, 16);

        assert_eq!(content, ChunkContent::Mixed);
        assert_eq!(smooth, 8);
        assert_eq!(blocky, 8);
    }

    #[test]
    fn test_generate_empty_chunk() {
        let grid = VoxelGrid::new(16);
        let mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, IVec3::ZERO, 16);

        assert_eq!(mesh.vertices.len(), 0);
        assert_eq!(mesh.indices.len(), 0);
    }
}
