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
//! - `mod` (this file) - Mesh generation and high-level APIs
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

mod face;
mod grid;

// Re-export core types from submodules
pub use face::{FaceDirection, get_visible_faces};
pub use grid::{
    BlockPos, MaterialRegistry, ResourceData, ResourceRegistry, VoxelBlock, VoxelGrid, VoxelMesh,
};

use glam::{IVec3, Vec3};
use std::collections::HashMap;

/// Mesh generation for voxel blocks
pub struct MeshGenerator;

impl MeshGenerator {
    /// Generate standard cube mesh (baseline)
    pub fn cube_mesh() -> VoxelMesh {
        let (verts, normals, indices) = crate::actors::Cube::unit_cube_indexed();
        VoxelMesh {
            vertices: verts,
            normals,
            indices,
        }
    }

    /// Generate mesh with top vertices deformed based on neighbor heights
    /// This creates smooth transitions/ramps automatically
    pub fn smoothed_mesh(
        position: BlockPos,
        neighbor_heights: [Option<i32>; 4], // [N, S, E, W]
    ) -> VoxelMesh {
        let current_height = position.y;
        let (mut verts, mut normals, indices) = crate::actors::Cube::unit_cube_indexed();

        // Check each direction for height differences
        let [north_h, south_h, east_h, west_h] = neighbor_heights;

        // If neighbor is lower by 1, deform that edge down to create ramp
        if north_h == Some(current_height - 1) {
            // Deform north edge (z = +0.5 in cube coordinates)
            Self::deform_vertices_on_edge(&mut verts, Edge::North, -0.5);
        }
        if south_h == Some(current_height - 1) {
            Self::deform_vertices_on_edge(&mut verts, Edge::South, -0.5);
        }
        if east_h == Some(current_height - 1) {
            Self::deform_vertices_on_edge(&mut verts, Edge::East, -0.5);
        }
        if west_h == Some(current_height - 1) {
            Self::deform_vertices_on_edge(&mut verts, Edge::West, -0.5);
        }

        // Recalculate normals for deformed faces
        Self::recalculate_normals(&verts, &indices, &mut normals);

        VoxelMesh {
            vertices: verts,
            normals,
            indices,
        }
    }

    fn deform_vertices_on_edge(verts: &mut [[f32; 3]], edge: Edge, offset: f32) {
        // Find vertices on the specified edge and adjust their Y coordinate
        for vert in verts.iter_mut() {
            let matches_edge = match edge {
                Edge::North => vert[2] > 0.49 && vert[1] > 0.49, // z=+0.5, y=+0.5 (top north)
                Edge::South => vert[2] < -0.49 && vert[1] > 0.49, // z=-0.5, y=+0.5 (top south)
                Edge::East => vert[0] > 0.49 && vert[1] > 0.49,  // x=+0.5, y=+0.5 (top east)
                Edge::West => vert[0] < -0.49 && vert[1] > 0.49, // x=-0.5, y=+0.5 (top west)
            };

            if matches_edge {
                vert[1] += offset; // Move down by offset
            }
        }
    }

    fn recalculate_normals(verts: &[[f32; 3]], indices: &[u32], normals: &mut [[f32; 3]]) {
        // Reset normals to zero
        for normal in normals.iter_mut() {
            *normal = [0.0, 0.0, 0.0];
        }

        // For each triangle, calculate face normal and accumulate
        for i in (0..indices.len()).step_by(3) {
            let i0 = indices[i] as usize;
            let i1 = indices[i + 1] as usize;
            let i2 = indices[i + 2] as usize;

            let v0 = Vec3::from(verts[i0]);
            let v1 = Vec3::from(verts[i1]);
            let v2 = Vec3::from(verts[i2]);

            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            let normal = edge1.cross(edge2).normalize();

            // Accumulate normals at vertices
            let n_arr = normal.to_array();
            for &idx in &[i0, i1, i2] {
                normals[idx][0] += n_arr[0];
                normals[idx][1] += n_arr[1];
                normals[idx][2] += n_arr[2];
            }
        }

        // Normalize accumulated normals
        for normal in normals.iter_mut() {
            let n = Vec3::from(*normal).normalize();
            *normal = n.to_array();
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Edge {
    North, // +Z
    South, // -Z
    East,  // +X
    West,  // -X
}

/// Terrain smoothing algorithm
pub struct TerrainSmoother;

impl TerrainSmoother {
    /// Apply smoothing pass to entire terrain
    /// Converts cubes to ramps where there are single-block height differences
    pub fn smooth_terrain(grid: &mut VoxelGrid) {
        // Collect positions first to avoid borrow issues
        let positions: Vec<BlockPos> = grid.iter_blocks().map(|b| b.position).collect();

        for pos in positions {
            let neighbor_heights = grid.get_neighbor_heights(pos);

            if Self::needs_smoothing(&neighbor_heights, pos.y) {
                // Generate smoothed mesh for this block
                let smoothed = MeshGenerator::smoothed_mesh(pos, neighbor_heights);

                if let Some(block) = grid.get_block_mut(&pos) {
                    block.mesh_data = smoothed;
                }
            } else {
                // Keep as standard cube
                if let Some(block) = grid.get_block_mut(&pos) {
                    block.mesh_data = MeshGenerator::cube_mesh();
                }
            }
        }
    }

    /// Determine if a block needs smoothing based on neighbor heights
    fn needs_smoothing(neighbor_heights: &[Option<i32>; 4], current_height: i32) -> bool {
        // If any neighbor is exactly 1 block lower, we need smoothing
        neighbor_heights
            .iter()
            .any(|&h| h == Some(current_height - 1))
    }
}

/// Represents a chunk of voxel terrain with merged, optimized mesh
/// All blocks in the chunk are combined into a single mesh with face culling applied
#[derive(Clone)]
pub struct VoxelChunk {
    pub chunk_pos: IVec3,         // Chunk coordinates
    pub vertices: Vec<[f32; 3]>,  // Merged mesh vertices (world space)
    pub normals: Vec<[f32; 3]>,   // Merged mesh normals
    pub indices: Vec<u32>,        // Merged mesh indices
    pub material_id: u32,         // Primary material ID
    pub mesh_handle: Option<u32>, // Renderer mesh handle (None = not uploaded)
}

impl VoxelChunk {
    /// Generate optimized chunk mesh with face culling from a VoxelGrid
    pub fn from_grid(grid: &VoxelGrid, chunk_pos: IVec3) -> Self {
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let mut vertex_offset = 0u32;

        let blocks = grid.get_chunk_blocks(chunk_pos);

        // Track material usage to determine primary material
        let mut material_counts: HashMap<u32, usize> = HashMap::new();

        for block in &blocks {
            *material_counts.entry(block.material_id).or_insert(0) += 1;
        }

        // Use most common material as primary material
        let material_id = material_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(id, _)| id)
            .unwrap_or(0);

        for block in blocks {
            // Get visible faces for this block (face culling)
            let visible_faces = get_visible_faces(grid, block.position);

            if visible_faces.is_empty() {
                continue; // Block is completely surrounded, skip it
            }

            // Generate mesh for this block with only visible faces
            let (block_verts, block_normals, block_indices) =
                Self::extract_visible_faces(&block.mesh_data, &visible_faces);

            if block_verts.is_empty() {
                continue; // No geometry to add
            }

            // Transform vertices to world position
            let world_pos = block.world_position();
            let vert_count = block_verts.len() as u32;
            for vert in block_verts {
                vertices.push([
                    vert[0] + world_pos.x,
                    vert[1] + world_pos.y,
                    vert[2] + world_pos.z,
                ]);
            }

            // Copy normals
            normals.extend_from_slice(&block_normals);

            // Offset indices to account for merged vertices
            for idx in block_indices {
                indices.push(idx + vertex_offset);
            }
            vertex_offset += vert_count;
        }

        VoxelChunk {
            chunk_pos,
            vertices,
            normals,
            indices,
            material_id,
            mesh_handle: None, // Mesh not yet uploaded to renderer
        }
    }

    /// Extract only visible faces from a block mesh
    /// Returns (vertices, normals, indices) for the visible faces only
    fn extract_visible_faces(
        block_mesh: &VoxelMesh,
        visible_faces: &[FaceDirection],
    ) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        if visible_faces.is_empty() {
            return (Vec::new(), Vec::new(), Vec::new());
        }

        // For full optimization, we would extract only the specified faces
        // For MVP, if any face is visible, include the whole block mesh
        // TODO: Implement per-face extraction for maximum optimization

        // Check if all faces are visible (common case for isolated blocks)
        if visible_faces.len() == 6 {
            // All faces visible, return entire mesh
            return (
                block_mesh.vertices.clone(),
                block_mesh.normals.clone(),
                block_mesh.indices.clone(),
            );
        }

        // Some faces culled - extract only visible faces
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let mut vertex_map: HashMap<u32, u32> = HashMap::new();
        let mut next_vertex_id = 0u32;

        for &face in visible_faces {
            let (idx_start, idx_count) = face.index_range();
            let face_indices = &block_mesh.indices[idx_start..idx_start + idx_count];

            // Add indices and track which vertices we need
            for &original_idx in face_indices {
                if let Some(&new_idx) = vertex_map.get(&original_idx) {
                    indices.push(new_idx);
                } else {
                    // First time seeing this vertex, add it
                    let new_idx = next_vertex_id;
                    vertex_map.insert(original_idx, new_idx);
                    indices.push(new_idx);

                    vertices.push(block_mesh.vertices[original_idx as usize]);
                    normals.push(block_mesh.normals[original_idx as usize]);

                    next_vertex_id += 1;
                }
            }
        }

        (vertices, normals, indices)
    }

    /// Check if chunk has any geometry
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Get approximate memory usage of this chunk in bytes
    pub fn memory_size(&self) -> usize {
        self.vertices.len() * std::mem::size_of::<[f32; 3]>()
            + self.normals.len() * std::mem::size_of::<[f32; 3]>()
            + self.indices.len() * std::mem::size_of::<u32>()
    }
}

/// Implement Renderable trait for VoxelChunk to integrate with renderer
impl crate::actors::Renderable for VoxelChunk {
    fn to_instance_with_material(&self, material_index: u32) -> crate::actors::InstanceGpu {
        // Chunk mesh is already in world space, so use identity transform
        let model = glam::Mat4::IDENTITY;
        let cols = model.to_cols_array();
        let mut mat = [[0f32; 4]; 4];
        mat[0] = [cols[0], cols[1], cols[2], cols[3]];
        mat[1] = [cols[4], cols[5], cols[6], cols[7]];
        mat[2] = [cols[8], cols[9], cols[10], cols[11]];
        mat[3] = [cols[12], cols[13], cols[14], cols[15]];

        crate::actors::InstanceGpu {
            model: mat,
            material: material_index,
            object_type: 3u32, // New object type for voxel chunks
            padding: [0u32; 2],
        }
    }
}

/// Implement CustomMesh trait to provide direct access to mesh geometry
impl crate::actors::CustomMesh for VoxelChunk {
    fn vertices(&self) -> &[[f32; 3]] {
        &self.vertices
    }

    fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }

    fn indices(&self) -> &[u32] {
        &self.indices
    }

    fn transform(&self) -> glam::Mat4 {
        // Chunk mesh vertices are already in world space
        glam::Mat4::IDENTITY
    }

    fn material_index(&self) -> u32 {
        0 // Default to Lambertian material
    }
}

impl VoxelChunk {
    /// Check if this chunk has geometry to render
    pub fn has_geometry(&self) -> bool {
        !self.vertices.is_empty() && !self.indices.is_empty()
    }

    /// Check if mesh is already uploaded to renderer
    pub fn is_uploaded(&self) -> bool {
        self.mesh_handle.is_some()
    }

    /// Set the renderer mesh handle
    pub fn set_mesh_handle(&mut self, handle: u32) {
        self.mesh_handle = Some(handle);
    }

    /// Get the renderer mesh handle (if uploaded)
    pub fn get_mesh_handle(&self) -> Option<u32> {
        self.mesh_handle
    }
}
