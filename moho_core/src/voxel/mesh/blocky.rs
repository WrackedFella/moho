//! Blocky mesh generation with per-vertex ambient occlusion.
//!
//! Generates cube-based meshes for blocks classified as "blocky" (crafted materials
//! like planks, bricks, etc.). Each vertex has ambient occlusion computed based on
//! the occupancy of its 4 neighboring corner blocks.

use crate::voxel::grid::{BlockPos, VoxelGrid, VoxelMesh};
use glam::IVec3;

/// Generates cube meshes with per-vertex ambient occlusion for blocky materials
pub struct BlockyMeshGenerator;

impl BlockyMeshGenerator {
    /// Generate a cube mesh with ambient occlusion for a single block
    ///
    /// # Arguments
    /// * `grid` - The voxel grid (needed to query neighbors for AO)
    /// * `position` - The block position in the grid
    ///
    /// # Returns
    /// A `VoxelMesh` with 24 vertices (4 per face), normals, and AO values
    pub fn generate_mesh(grid: &VoxelGrid, position: BlockPos) -> VoxelMesh {
        let mut mesh = VoxelMesh::empty();

        // Face directions and their vertex configurations
        // For each face, we need to determine which 4 corner neighbors to check for AO
        
        // +X face (right): check neighbors in +X direction
        Self::add_face(
            &mut mesh,
            grid,
            position,
            FaceDirection::PosX,
        );

        // -X face (left): check neighbors in -X direction
        Self::add_face(
            &mut mesh,
            grid,
            position,
            FaceDirection::NegX,
        );

        // +Y face (top): check neighbors in +Y direction
        Self::add_face(
            &mut mesh,
            grid,
            position,
            FaceDirection::PosY,
        );

        // -Y face (bottom): check neighbors in -Y direction
        Self::add_face(
            &mut mesh,
            grid,
            position,
            FaceDirection::NegY,
        );

        // +Z face (front): check neighbors in +Z direction
        Self::add_face(
            &mut mesh,
            grid,
            position,
            FaceDirection::PosZ,
        );

        // -Z face (back): check neighbors in -Z direction
        Self::add_face(
            &mut mesh,
            grid,
            position,
            FaceDirection::NegZ,
        );

        mesh
    }

    /// Add a single face to the mesh with AO computed for each vertex
    fn add_face(
        mesh: &mut VoxelMesh,
        grid: &VoxelGrid,
        position: BlockPos,
        direction: FaceDirection,
    ) {
        let base_index = mesh.vertices.len() as u32;
        
        // Get face geometry (vertices and normal)
        let (verts, normal) = direction.vertices_and_normal();
        
        // Add vertices
        for vert in verts.iter() {
            mesh.vertices.push(*vert);
        }
        
        // Add normals (all 4 vertices share the same normal)
        for _ in 0..4 {
            mesh.normals.push(normal);
        }
        
        // Compute AO for each vertex based on corner neighbors
        let ao_values = direction.compute_ao(grid, position);
        mesh.ambient_occlusion.extend_from_slice(&ao_values);
        
        // Mark as blocky geometry (0 = smooth, 1 = blocky)
        for _ in 0..4 {
            mesh.geometry_type.push(1);
        }
        
        // Get block's light level (max of sky and block light, normalized to 0-1)
        let light = if let Some(block) = grid.get_block(&position) {
            block.light_level() as f32 / 15.0
        } else {
            1.0 // Default to full light if block not found
        };
        
        // Apply same light level to all 4 vertices of this face
        for _ in 0..4 {
            mesh.light_level.push(light);
        }
        
        // Add indices (2 triangles per face)
        mesh.indices.push(base_index);
        mesh.indices.push(base_index + 1);
        mesh.indices.push(base_index + 2);
        mesh.indices.push(base_index);
        mesh.indices.push(base_index + 2);
        mesh.indices.push(base_index + 3);
    }
}

/// Face direction with associated geometry and AO computation logic
#[derive(Debug, Clone, Copy)]
enum FaceDirection {
    PosX, // Right
    NegX, // Left
    PosY, // Top
    NegY, // Bottom
    PosZ, // Front
    NegZ, // Back
}

impl FaceDirection {
    /// Get the 4 vertices and normal for this face
    fn vertices_and_normal(&self) -> ([[f32; 3]; 4], [f32; 3]) {
        match self {
            FaceDirection::PosX => (
                [
                    [0.5, -0.5, -0.5], // bottom-back
                    [0.5, 0.5, -0.5],  // top-back
                    [0.5, 0.5, 0.5],   // top-front
                    [0.5, -0.5, 0.5],  // bottom-front
                ],
                [1.0, 0.0, 0.0],
            ),
            FaceDirection::NegX => (
                [
                    [-0.5, -0.5, 0.5],  // bottom-front
                    [-0.5, 0.5, 0.5],   // top-front
                    [-0.5, 0.5, -0.5],  // top-back
                    [-0.5, -0.5, -0.5], // bottom-back
                ],
                [-1.0, 0.0, 0.0],
            ),
            FaceDirection::PosY => (
                [
                    [-0.5, 0.5, -0.5], // back-left
                    [-0.5, 0.5, 0.5],  // front-left
                    [0.5, 0.5, 0.5],   // front-right
                    [0.5, 0.5, -0.5],  // back-right
                ],
                [0.0, 1.0, 0.0],
            ),
            FaceDirection::NegY => (
                [
                    [-0.5, -0.5, 0.5],  // front-left
                    [0.5, -0.5, 0.5],   // front-right
                    [0.5, -0.5, -0.5],  // back-right
                    [-0.5, -0.5, -0.5], // back-left
                ],
                [0.0, -1.0, 0.0],
            ),
            FaceDirection::PosZ => (
                [
                    [-0.5, -0.5, 0.5], // bottom-left
                    [0.5, -0.5, 0.5],  // bottom-right
                    [0.5, 0.5, 0.5],   // top-right
                    [-0.5, 0.5, 0.5],  // top-left
                ],
                [0.0, 0.0, 1.0],
            ),
            FaceDirection::NegZ => (
                [
                    [0.5, -0.5, -0.5],  // bottom-right
                    [-0.5, -0.5, -0.5], // bottom-left
                    [-0.5, 0.5, -0.5],  // top-left
                    [0.5, 0.5, -0.5],   // top-right
                ],
                [0.0, 0.0, -1.0],
            ),
        }
    }

    /// Compute ambient occlusion for each of the 4 vertices on this face
    ///
    /// AO is computed using the 4-corner method: for each vertex, check the 3 blocks
    /// that form its corner (2 edge neighbors + 1 diagonal).
    ///
    /// AO formula: 1.0 - (side1 + side2 + corner * side1 * side2) * 0.2
    /// This gives darker values when more neighbors are present.
    fn compute_ao(&self, grid: &VoxelGrid, pos: BlockPos) -> [f32; 4] {
        match self {
            FaceDirection::PosX => {
                // Right face: vertices see +X direction
                let v0 = Self::vertex_ao(grid, pos, IVec3::new(1, -1, -1), // bottom-back
                    IVec3::new(1, 0, -1), IVec3::new(1, -1, 0), IVec3::new(1, -1, -1));
                let v1 = Self::vertex_ao(grid, pos, IVec3::new(1, 1, -1),  // top-back
                    IVec3::new(1, 1, 0), IVec3::new(1, 0, -1), IVec3::new(1, 1, -1));
                let v2 = Self::vertex_ao(grid, pos, IVec3::new(1, 1, 1),   // top-front
                    IVec3::new(1, 0, 1), IVec3::new(1, 1, 0), IVec3::new(1, 1, 1));
                let v3 = Self::vertex_ao(grid, pos, IVec3::new(1, -1, 1),  // bottom-front
                    IVec3::new(1, -1, 0), IVec3::new(1, 0, 1), IVec3::new(1, -1, 1));
                [v0, v1, v2, v3]
            }
            FaceDirection::NegX => {
                // Left face: vertices see -X direction
                let v0 = Self::vertex_ao(grid, pos, IVec3::new(-1, -1, 1),
                    IVec3::new(-1, 0, 1), IVec3::new(-1, -1, 0), IVec3::new(-1, -1, 1));
                let v1 = Self::vertex_ao(grid, pos, IVec3::new(-1, 1, 1),
                    IVec3::new(-1, 1, 0), IVec3::new(-1, 0, 1), IVec3::new(-1, 1, 1));
                let v2 = Self::vertex_ao(grid, pos, IVec3::new(-1, 1, -1),
                    IVec3::new(-1, 0, -1), IVec3::new(-1, 1, 0), IVec3::new(-1, 1, -1));
                let v3 = Self::vertex_ao(grid, pos, IVec3::new(-1, -1, -1),
                    IVec3::new(-1, -1, 0), IVec3::new(-1, 0, -1), IVec3::new(-1, -1, -1));
                [v0, v1, v2, v3]
            }
            FaceDirection::PosY => {
                // Top face: vertices see +Y direction
                let v0 = Self::vertex_ao(grid, pos, IVec3::new(-1, 1, -1),
                    IVec3::new(-1, 1, 0), IVec3::new(0, 1, -1), IVec3::new(-1, 1, -1));
                let v1 = Self::vertex_ao(grid, pos, IVec3::new(-1, 1, 1),
                    IVec3::new(0, 1, 1), IVec3::new(-1, 1, 0), IVec3::new(-1, 1, 1));
                let v2 = Self::vertex_ao(grid, pos, IVec3::new(1, 1, 1),
                    IVec3::new(1, 1, 0), IVec3::new(0, 1, 1), IVec3::new(1, 1, 1));
                let v3 = Self::vertex_ao(grid, pos, IVec3::new(1, 1, -1),
                    IVec3::new(0, 1, -1), IVec3::new(1, 1, 0), IVec3::new(1, 1, -1));
                [v0, v1, v2, v3]
            }
            FaceDirection::NegY => {
                // Bottom face: vertices see -Y direction
                let v0 = Self::vertex_ao(grid, pos, IVec3::new(-1, -1, 1),
                    IVec3::new(0, -1, 1), IVec3::new(-1, -1, 0), IVec3::new(-1, -1, 1));
                let v1 = Self::vertex_ao(grid, pos, IVec3::new(1, -1, 1),
                    IVec3::new(1, -1, 0), IVec3::new(0, -1, 1), IVec3::new(1, -1, 1));
                let v2 = Self::vertex_ao(grid, pos, IVec3::new(1, -1, -1),
                    IVec3::new(0, -1, -1), IVec3::new(1, -1, 0), IVec3::new(1, -1, -1));
                let v3 = Self::vertex_ao(grid, pos, IVec3::new(-1, -1, -1),
                    IVec3::new(-1, -1, 0), IVec3::new(0, -1, -1), IVec3::new(-1, -1, -1));
                [v0, v1, v2, v3]
            }
            FaceDirection::PosZ => {
                // Front face: vertices see +Z direction
                let v0 = Self::vertex_ao(grid, pos, IVec3::new(-1, -1, 1),
                    IVec3::new(-1, 0, 1), IVec3::new(0, -1, 1), IVec3::new(-1, -1, 1));
                let v1 = Self::vertex_ao(grid, pos, IVec3::new(1, -1, 1),
                    IVec3::new(0, -1, 1), IVec3::new(1, 0, 1), IVec3::new(1, -1, 1));
                let v2 = Self::vertex_ao(grid, pos, IVec3::new(1, 1, 1),
                    IVec3::new(1, 0, 1), IVec3::new(0, 1, 1), IVec3::new(1, 1, 1));
                let v3 = Self::vertex_ao(grid, pos, IVec3::new(-1, 1, 1),
                    IVec3::new(0, 1, 1), IVec3::new(-1, 0, 1), IVec3::new(-1, 1, 1));
                [v0, v1, v2, v3]
            }
            FaceDirection::NegZ => {
                // Back face: vertices see -Z direction
                let v0 = Self::vertex_ao(grid, pos, IVec3::new(1, -1, -1),
                    IVec3::new(0, -1, -1), IVec3::new(1, 0, -1), IVec3::new(1, -1, -1));
                let v1 = Self::vertex_ao(grid, pos, IVec3::new(-1, -1, -1),
                    IVec3::new(-1, 0, -1), IVec3::new(0, -1, -1), IVec3::new(-1, -1, -1));
                let v2 = Self::vertex_ao(grid, pos, IVec3::new(-1, 1, -1),
                    IVec3::new(0, 1, -1), IVec3::new(-1, 0, -1), IVec3::new(-1, 1, -1));
                let v3 = Self::vertex_ao(grid, pos, IVec3::new(1, 1, -1),
                    IVec3::new(1, 0, -1), IVec3::new(0, 1, -1), IVec3::new(1, 1, -1));
                [v0, v1, v2, v3]
            }
        }
    }

    /// Compute AO value for a single vertex using the 4-corner method
    ///
    /// # Arguments
    /// * `grid` - The voxel grid
    /// * `pos` - Base block position
    /// * `_corner` - Corner offset (unused, kept for documentation)
    /// * `side1` - First edge neighbor offset
    /// * `side2` - Second edge neighbor offset
    /// * `diagonal` - Diagonal corner neighbor offset
    ///
    /// # Returns
    /// AO value in range [0.0, 1.0] where 1.0 = no occlusion, 0.0 = fully occluded
    fn vertex_ao(
        grid: &VoxelGrid,
        pos: BlockPos,
        _corner: IVec3,
        side1: IVec3,
        side2: IVec3,
        diagonal: IVec3,
    ) -> f32 {
        // Check if each neighbor position is occupied
        let s1 = grid.is_block_occupied(pos + side1) as u8;
        let s2 = grid.is_block_occupied(pos + side2) as u8;
        let d = grid.is_block_occupied(pos + diagonal) as u8;

        // AO formula: darken based on number of occupied neighbors
        // If both sides are occupied, diagonal doesn't matter (maximum occlusion)
        let occlusion = if s1 == 1 && s2 == 1 {
            3.0 // Both sides block -> maximum darkening
        } else {
            (s1 + s2 + d) as f32
        };

        // Convert to [0.0, 1.0] range with darkening factor
        // 0 neighbors = 1.0 (bright), 3 neighbors = 0.4 (dark)
        1.0 - (occlusion * 0.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::grid::VoxelBlock;

    #[test]
    fn test_isolated_block_has_no_occlusion() {
        let mut grid = VoxelGrid::new(32);
        let pos = IVec3::new(16, 16, 16);
        grid.set_block(pos, VoxelBlock::new(pos, 100)); // Blocky material ID

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        // All vertices should have full brightness (1.0) since no neighbors
        for ao in mesh.ambient_occlusion {
            assert_eq!(ao, 1.0, "Isolated block should have no occlusion");
        }
    }

    #[test]
    fn test_corner_block_has_occlusion() {
        let mut grid = VoxelGrid::new(32);
        let pos = IVec3::new(16, 16, 16);
        grid.set_block(pos, VoxelBlock::new(pos, 100));
        
        // Add neighbors to create occlusion (forms a corner)
        grid.set_block(pos + IVec3::new(1, 0, 0), VoxelBlock::new(pos + IVec3::new(1, 0, 0), 100));
        grid.set_block(pos + IVec3::new(0, 1, 0), VoxelBlock::new(pos + IVec3::new(0, 1, 0), 100));
        grid.set_block(pos + IVec3::new(1, 1, 0), VoxelBlock::new(pos + IVec3::new(1, 1, 0), 100));

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        // Some vertices should have reduced AO (< 1.0) due to neighbors
        let has_occlusion = mesh.ambient_occlusion.iter().any(|&ao| ao < 1.0);
        assert!(has_occlusion, "Block with neighbors should have some occlusion");
    }

    #[test]
    fn test_mesh_structure() {
        let mut grid = VoxelGrid::new(32);
        let pos = IVec3::new(16, 16, 16);
        grid.set_block(pos, VoxelBlock::new(pos, 100));

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        // 6 faces * 4 vertices = 24 vertices
        assert_eq!(mesh.vertices.len(), 24, "Should have 24 vertices");
        assert_eq!(mesh.normals.len(), 24, "Should have 24 normals");
        assert_eq!(mesh.ambient_occlusion.len(), 24, "Should have 24 AO values");
        
        // 6 faces * 2 triangles * 3 indices = 36 indices
        assert_eq!(mesh.indices.len(), 36, "Should have 36 indices");
    }
}
