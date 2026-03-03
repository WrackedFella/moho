//! Chunk-based voxel organization for rendering optimization.
//!
//! This module provides:
//! - VoxelChunk: Merged mesh for multiple blocks with face culling
//! - TerrainSmoother: Algorithm for generating smooth terrain transitions
//! - Chunk mesh extraction with visibility optimization

mod extraction;

use super::face::get_visible_faces;
use super::grid::{BlockPos, VoxelGrid};
use super::mesh::MeshGenerator;
use glam::IVec3;
use std::collections::HashMap;

#[cfg(test)]
use super::face::FaceDirection;

/// Terrain smoothing algorithm
#[derive(Debug)]
pub struct TerrainSmoother;

impl TerrainSmoother {
    /// Apply smoothing pass to entire terrain
    ///
    /// Converts cubes to ramps where there are single-block height differences.
    /// Blocks with neighbors exactly 1 block lower get smooth ramp meshes instead
    /// of sharp cube edges.
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
    ///
    /// Returns true if any neighbor is exactly 1 block lower than current height.
    fn needs_smoothing(neighbor_heights: &[Option<i32>; 4], current_height: i32) -> bool {
        // If any neighbor is exactly 1 block lower, we need smoothing
        neighbor_heights
            .iter()
            .any(|&h| h == Some(current_height - 1))
    }
}

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
    indices: Vec<u32>,
    material_id: u32,
    mesh_handle: Option<u32>,
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
            indices,
            material_id,
            mesh_handle: None,
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
            0,
        )
    }

    // --- Getters ---

    pub fn chunk_pos(&self) -> IVec3 {
        self.chunk_pos
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
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub fn material_id(&self) -> u32 {
        self.material_id
    }
}

impl VoxelChunk {
    /// Generate optimized chunk mesh with face culling from a VoxelGrid
    ///
    /// # Arguments
    /// * `grid` - Source voxel grid containing blocks
    /// * `chunk_pos` - Chunk coordinates to generate mesh for
    ///
    /// # Returns
    /// A `VoxelChunk` with merged geometry and face culling applied
    pub fn from_grid(grid: &VoxelGrid, chunk_pos: IVec3) -> Self {
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut ambient_occlusion = Vec::new();
        let mut geometry_type = Vec::new();
        let mut light_level = Vec::new();
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
            let (block_verts, block_normals, block_ao, block_geo_type, block_indices) =
                extraction::extract_visible_faces(&block.mesh_data, &visible_faces);

            if block_verts.is_empty() {
                continue; // No geometry to add
            }

            // Get block's light level (max of sky and block light, normalized to 0-1)
            let block_light = block.light_level() as f32 / 15.0;

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

            // Copy normals, AO, geometry type, and light level
            normals.extend_from_slice(&block_normals);
            ambient_occlusion.extend_from_slice(&block_ao);
            geometry_type.extend_from_slice(&block_geo_type);

            // Fill light level for all vertices of this block
            light_level.resize(light_level.len() + vert_count as usize, block_light);

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
            ambient_occlusion,
            geometry_type,
            light_level,
            indices,
            material_id,
            mesh_handle: None, // Mesh not yet uploaded to renderer
        }
    }

    /// Generate chunk mesh using hybrid mesh generation (Phase 1).
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

        // Determine primary material from blocks in chunk
        let blocks = grid.get_chunk_blocks(chunk_pos);
        let mut material_counts: HashMap<u32, usize> = HashMap::new();

        for block in &blocks {
            *material_counts.entry(block.material_id).or_insert(0) += 1;
        }

        let material_id = material_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(id, _)| id)
            .unwrap_or(0);

        VoxelChunk {
            chunk_pos,
            vertices: mesh.vertices,
            normals: mesh.normals,
            ambient_occlusion: mesh.ambient_occlusion,
            geometry_type: mesh.geometry_type,
            light_level: mesh.light_level,
            indices: mesh.indices,
            material_id,
            mesh_handle: None,
        }
    }

    /// Check if chunk has any geometry
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Get approximate memory usage of this chunk in bytes
    pub fn memory_size(&self) -> usize {
        self.vertices.len() * std::mem::size_of::<[f32; 3]>()
            + self.normals.len() * std::mem::size_of::<[f32; 3]>()
            + self.ambient_occlusion.len() * std::mem::size_of::<f32>()
            + self.geometry_type.len() * std::mem::size_of::<u32>()
            + self.light_level.len() * std::mem::size_of::<f32>()
            + self.indices.len() * std::mem::size_of::<u32>()
    }

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
            object_type: 3u32, // Object type for voxel chunks
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terrain_smoother_needs_smoothing() {
        let current_height = 10;

        // No smoothing needed - all neighbors at same height
        let neighbor_heights = [Some(10), Some(10), Some(10), Some(10)];
        assert!(!TerrainSmoother::needs_smoothing(
            &neighbor_heights,
            current_height
        ));

        // Smoothing needed - north neighbor 1 block lower
        let neighbor_heights = [Some(9), Some(10), Some(10), Some(10)];
        assert!(TerrainSmoother::needs_smoothing(
            &neighbor_heights,
            current_height
        ));

        // No smoothing - neighbor too low (2+ blocks difference)
        let neighbor_heights = [Some(8), Some(10), Some(10), Some(10)];
        assert!(!TerrainSmoother::needs_smoothing(
            &neighbor_heights,
            current_height
        ));

        // No smoothing - neighbor higher
        let neighbor_heights = [Some(11), Some(10), Some(10), Some(10)];
        assert!(!TerrainSmoother::needs_smoothing(
            &neighbor_heights,
            current_height
        ));
    }

    #[test]
    fn test_chunk_empty() {
        let chunk = VoxelChunk::empty(IVec3::ZERO);

        assert!(chunk.is_empty());
        assert!(!chunk.has_geometry());
        assert!(!chunk.is_uploaded());
    }

    #[test]
    fn test_chunk_with_geometry() {
        let chunk = VoxelChunk::new(
            IVec3::ZERO,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            vec![1.0, 1.0, 1.0],
            vec![1, 1, 1],
            vec![1.0, 1.0, 1.0],
            vec![0, 1, 2],
            0,
        );

        assert!(!chunk.is_empty());
        assert!(chunk.has_geometry());
        assert!(!chunk.is_uploaded());
    }

    #[test]
    fn test_chunk_mesh_handle() {
        let mut chunk = VoxelChunk::new(
            IVec3::ZERO,
            vec![[0.0, 0.0, 0.0]],
            vec![[0.0, 1.0, 0.0]],
            vec![1.0],
            vec![1],
            vec![1.0],
            vec![0],
            0,
        );

        assert_eq!(chunk.get_mesh_handle(), None);

        chunk.set_mesh_handle(42);
        assert_eq!(chunk.get_mesh_handle(), Some(42));
        assert!(chunk.is_uploaded());
    }

    #[test]
    fn test_chunk_memory_size() {
        let chunk = VoxelChunk::new(
            IVec3::ZERO,
            vec![[0.0, 0.0, 0.0]; 100],
            vec![[0.0, 1.0, 0.0]; 100],
            vec![1.0; 100],
            vec![1; 100],
            vec![1.0; 100],
            vec![0; 150],
            0,
        );

        let expected = 100 * 12 + 100 * 12 + 100 * 4 + 100 * 4 + 100 * 4 + 150 * 4; // verts + normals + ao + geo_type + light + indices
        assert_eq!(chunk.memory_size(), expected);
    }

    #[test]
    fn test_extract_all_visible_faces() {
        let mesh = MeshGenerator::cube_mesh();
        let all_faces = vec![
            FaceDirection::PosX,
            FaceDirection::NegX,
            FaceDirection::PosY,
            FaceDirection::NegY,
            FaceDirection::PosZ,
            FaceDirection::NegZ,
        ];

        let (verts, normals, ao, geo_type, indices) =
            extraction::extract_visible_faces(&mesh, &all_faces);

        // Should return complete mesh
        assert_eq!(verts.len(), mesh.vertices.len());
        assert_eq!(normals.len(), mesh.normals.len());
        assert_eq!(ao.len(), mesh.ambient_occlusion.len());
        assert_eq!(geo_type.len(), mesh.geometry_type.len());
        assert_eq!(indices.len(), mesh.indices.len());
    }

    #[test]
    fn test_extract_no_visible_faces() {
        let mesh = MeshGenerator::cube_mesh();
        let no_faces = vec![];

        let (verts, normals, ao, geo_type, indices) =
            extraction::extract_visible_faces(&mesh, &no_faces);

        // Should return empty mesh
        assert!(verts.is_empty());
        assert!(normals.is_empty());
        assert!(ao.is_empty());
        assert!(geo_type.is_empty());
        assert!(indices.is_empty());
    }

    #[test]
    fn test_extract_partial_faces() {
        let mesh = MeshGenerator::cube_mesh();
        let some_faces = vec![FaceDirection::PosY, FaceDirection::NegY];

        let (verts, _normals, _ao, _geo_type, indices) =
            extraction::extract_visible_faces(&mesh, &some_faces);

        // Should return subset of mesh (2 faces out of 6)
        assert!(!verts.is_empty());
        assert!(!indices.is_empty());
        assert!(verts.len() < mesh.vertices.len());
        assert!(indices.len() < mesh.indices.len());
    }
}
