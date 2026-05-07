//! Hybrid mesh generation for chunks containing both smooth and blocky geometry.
//!
//! This module detects mixed chunks and generates appropriate meshes for each
//! geometry type, then concatenates them into a unified mesh for rendering.

use super::super::grid::{BlockPos, VoxelGrid};
use super::VoxelMesh;
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
                    match grid.is_smooth_at(pos) {
                        Some(true) => smooth_count += 1,
                        Some(false) => blocky_count += 1,
                        None => {}
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
    pub fn generate_chunk_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
        let (content, _smooth_count, _blocky_count) =
            Self::analyze_chunk(grid, chunk_pos, chunk_size);

        match content {
            ChunkContent::Empty => VoxelMesh::empty(),

            ChunkContent::AllSmooth => Self::generate_smooth_mesh(grid, chunk_pos, chunk_size),

            ChunkContent::AllBlocky => Self::generate_blocky_mesh(grid, chunk_pos, chunk_size),

            ChunkContent::Mixed => Self::generate_mixed_mesh(grid, chunk_pos, chunk_size),
        }
    }

    /// Generate mesh using Marching Cubes for smooth terrain
    fn generate_smooth_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
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
    fn generate_blocky_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
        let mut mesh = VoxelMesh::empty();
        let base_pos = chunk_pos * chunk_size;

        // Generate mesh for each block in chunk
        for x in 0..chunk_size {
            for y in 0..chunk_size {
                for z in 0..chunk_size {
                    let pos = base_pos + IVec3::new(x, y, z);
                    if grid.is_solid_at(pos) {
                        let block_mesh = BlockyMeshGenerator::generate_mesh(grid, pos);
                        Self::append_mesh(&mut mesh, &block_mesh, pos);
                    }
                }
            }
        }

        mesh
    }

    /// Generate mesh for mixed chunk (smooth + blocky)
    fn generate_mixed_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
        let mut final_mesh = VoxelMesh::empty();
        let base_pos = chunk_pos * chunk_size;

        // Generate smooth terrain mesh (only for smooth blocks)
        let smooth_density = Self::create_selective_density_field(
            grid, chunk_pos, chunk_size, true, // only smooth blocks
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
            final_mesh
                .ambient_occlusion
                .push(smooth_mesh.ambient_occlusion[i]);
            final_mesh.geometry_type.push(smooth_mesh.geometry_type[i]);
            final_mesh.light_level.push(smooth_mesh.light_level[i]);
            final_mesh.block_light_rgb.push(smooth_mesh.block_light_rgb[i]);
            final_mesh.sky_exposed.push(smooth_mesh.sky_exposed[i]);
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
                    if grid.is_smooth_at(pos) == Some(false) {
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
        for (x, plane) in field.iter_mut().enumerate() {
            for (y, row) in plane.iter_mut().enumerate() {
                for (z, col) in row.iter_mut().enumerate() {
                    let world_pos = base_pos + IVec3::new(x as i32 - 1, y as i32 - 1, z as i32 - 1);

                    let is_solid = match grid.is_smooth_at(world_pos) {
                        Some(smooth) => !only_smooth || smooth,
                        None => false,
                    };

                    *col = if is_solid { 1.0 } else { 0.0 };
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

        // Copy normals, AO, geometry type, and light data
        target.normals.extend_from_slice(&source.normals);
        target
            .ambient_occlusion
            .extend_from_slice(&source.ambient_occlusion);
        target
            .geometry_type
            .extend_from_slice(&source.geometry_type);
        target.light_level.extend_from_slice(&source.light_level);
        target.block_light_rgb.extend_from_slice(&source.block_light_rgb);
        target.sky_exposed.extend_from_slice(&source.sky_exposed);

        // Add indices with offset
        for &index in &source.indices {
            target.indices.push(base_index + index);
        }
    }

    /// Generate a coarse (LOD 1) blocky mesh by sampling every 2 blocks.
    ///
    /// Each 2×2×2 block region is treated as a single coarse voxel. If any block in
    /// the region is solid the cell is solid, giving an 8³ effective resolution for a
    /// 16³ chunk. Face culling works against adjacent coarse cells (including cells in
    /// neighbouring chunks). Skirt quads are appended along the four vertical chunk
    /// edges to prevent seam cracks at LOD boundaries.
    pub fn generate_coarse_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
        const STRIDE: i32 = 2;
        let coarse_size = chunk_size / STRIDE; // 8 for chunk_size == 16
        let base = chunk_pos * chunk_size;
        let mut mesh = VoxelMesh::empty();

        // Returns true if any block in the STRIDE³ region rooted at the given coarse
        // cell coordinates is solid. Intentionally samples into adjacent chunks via
        // `grid.is_solid_at` so inter-chunk face culling works correctly.
        let cell_solid = |cx: i32, cy: i32, cz: i32| -> bool {
            let wx = base.x + cx * STRIDE;
            let wy = base.y + cy * STRIDE;
            let wz = base.z + cz * STRIDE;
            for dx in 0..STRIDE {
                for dy in 0..STRIDE {
                    for dz in 0..STRIDE {
                        if grid.is_solid_at(IVec3::new(wx + dx, wy + dy, wz + dz)) {
                            return true;
                        }
                    }
                }
            }
            false
        };

        for cx in 0..coarse_size {
            for cy in 0..coarse_size {
                for cz in 0..coarse_size {
                    if !cell_solid(cx, cy, cz) {
                        continue;
                    }

                    let wx = base.x + cx * STRIDE;
                    let wy = base.y + cy * STRIDE;
                    let wz = base.z + cz * STRIDE;

                    // +X
                    if !cell_solid(cx + 1, cy, cz) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx + STRIDE, wy,          wz          ],
                            [wx + STRIDE, wy + STRIDE, wz          ],
                            [wx + STRIDE, wy + STRIDE, wz + STRIDE ],
                            [wx + STRIDE, wy,          wz + STRIDE ],
                            [1.0, 0.0, 0.0],
                        );
                        if cx == coarse_size - 1 {
                            Self::emit_coarse_quad(
                                &mut mesh,
                                [wx + STRIDE, wy - STRIDE, wz          ],
                                [wx + STRIDE, wy,          wz          ],
                                [wx + STRIDE, wy,          wz + STRIDE ],
                                [wx + STRIDE, wy - STRIDE, wz + STRIDE ],
                                [1.0, 0.0, 0.0],
                            );
                        }
                    }
                    // -X
                    if !cell_solid(cx - 1, cy, cz) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx, wy,          wz + STRIDE ],
                            [wx, wy + STRIDE, wz + STRIDE ],
                            [wx, wy + STRIDE, wz          ],
                            [wx, wy,          wz          ],
                            [-1.0, 0.0, 0.0],
                        );
                        if cx == 0 {
                            Self::emit_coarse_quad(
                                &mut mesh,
                                [wx, wy - STRIDE, wz + STRIDE ],
                                [wx, wy,          wz + STRIDE ],
                                [wx, wy,          wz          ],
                                [wx, wy - STRIDE, wz          ],
                                [-1.0, 0.0, 0.0],
                            );
                        }
                    }
                    // +Y
                    if !cell_solid(cx, cy + 1, cz) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx,          wy + STRIDE, wz          ],
                            [wx,          wy + STRIDE, wz + STRIDE ],
                            [wx + STRIDE, wy + STRIDE, wz + STRIDE ],
                            [wx + STRIDE, wy + STRIDE, wz          ],
                            [0.0, 1.0, 0.0],
                        );
                    }
                    // -Y
                    if !cell_solid(cx, cy - 1, cz) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx,          wy, wz + STRIDE ],
                            [wx + STRIDE, wy, wz + STRIDE ],
                            [wx + STRIDE, wy, wz          ],
                            [wx,          wy, wz          ],
                            [0.0, -1.0, 0.0],
                        );
                    }
                    // +Z
                    if !cell_solid(cx, cy, cz + 1) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx,          wy,          wz + STRIDE ],
                            [wx + STRIDE, wy,          wz + STRIDE ],
                            [wx + STRIDE, wy + STRIDE, wz + STRIDE ],
                            [wx,          wy + STRIDE, wz + STRIDE ],
                            [0.0, 0.0, 1.0],
                        );
                        if cz == coarse_size - 1 {
                            Self::emit_coarse_quad(
                                &mut mesh,
                                [wx,          wy - STRIDE, wz + STRIDE ],
                                [wx + STRIDE, wy - STRIDE, wz + STRIDE ],
                                [wx + STRIDE, wy,          wz + STRIDE ],
                                [wx,          wy,          wz + STRIDE ],
                                [0.0, 0.0, 1.0],
                            );
                        }
                    }
                    // -Z
                    if !cell_solid(cx, cy, cz - 1) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx + STRIDE, wy,          wz],
                            [wx,          wy,          wz],
                            [wx,          wy + STRIDE, wz],
                            [wx + STRIDE, wy + STRIDE, wz],
                            [0.0, 0.0, -1.0],
                        );
                        if cz == 0 {
                            Self::emit_coarse_quad(
                                &mut mesh,
                                [wx + STRIDE, wy - STRIDE, wz],
                                [wx,          wy - STRIDE, wz],
                                [wx,          wy,          wz],
                                [wx + STRIDE, wy,          wz],
                                [0.0, 0.0, -1.0],
                            );
                        }
                    }
                }
            }
        }

        mesh
    }

    /// Emit a single quad (2 triangles) into a VoxelMesh with no AO and full brightness.
    fn emit_coarse_quad(
        mesh: &mut VoxelMesh,
        v0: [i32; 3],
        v1: [i32; 3],
        v2: [i32; 3],
        v3: [i32; 3],
        normal: [f32; 3],
    ) {
        let base = mesh.vertices.len() as u32;
        for v in [v0, v1, v2, v3] {
            mesh.vertices.push([v[0] as f32, v[1] as f32, v[2] as f32]);
            mesh.normals.push(normal);
            mesh.ambient_occlusion.push(1.0);
            mesh.geometry_type.push(1);
            mesh.light_level.push(1.0);
            mesh.block_light_rgb.push([0.0, 0.0, 0.0]);
            mesh.sky_exposed.push(1.0);
        }
        mesh.indices.push(base);
        mesh.indices.push(base + 1);
        mesh.indices.push(base + 2);
        mesh.indices.push(base);
        mesh.indices.push(base + 2);
        mesh.indices.push(base + 3);
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

        // Copy normals, AO, geometry type, and light data
        target.normals.extend_from_slice(&source.normals);
        target
            .ambient_occlusion
            .extend_from_slice(&source.ambient_occlusion);
        target
            .geometry_type
            .extend_from_slice(&source.geometry_type);
        target.light_level.extend_from_slice(&source.light_level);
        target.block_light_rgb.extend_from_slice(&source.block_light_rgb);
        target.sky_exposed.extend_from_slice(&source.sky_exposed);

        // Add indices with combined offset
        for &index in &source.indices {
            target.indices.push(index_offset + base_index + index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    grid.place_block(pos, 0, None); // Material 0 is smooth
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
                    grid.place_block(pos, 100, None); // Material 100 is blocky
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
                    grid.place_block(pos, 0, None); // Smooth
                }
            }
        }

        for x in 2..4 {
            for y in 2..4 {
                for z in 2..4 {
                    let pos = IVec3::new(x, y, z);
                    grid.place_block(pos, 100, None); // Blocky
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
