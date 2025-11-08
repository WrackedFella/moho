//! Mesh generation for voxel blocks.
//!
//! This module provides mesh generation algorithms for converting voxel blocks
//! into renderable geometry:
//! - Standard cube meshes (baseline)
//! - Smoothed meshes with terrain adaptation
//! - Normal recalculation for deformed geometry

mod deform;
mod normals;

use super::grid::{BlockPos, VoxelMesh};
use crate::actors::Cube;
use deform::Edge;

/// Mesh generation for voxel blocks
pub struct MeshGenerator;

impl MeshGenerator {
    /// Generate standard cube mesh (baseline)
    ///
    /// Returns a unit cube mesh with vertices in the range [-0.5, 0.5]
    /// suitable for rendering at block positions.
    pub fn cube_mesh() -> VoxelMesh {
        let (verts, normals, indices) = Cube::unit_cube_indexed();
        VoxelMesh {
            vertices: verts,
            normals,
            indices,
        }
    }

    /// Generate mesh with top vertices deformed based on neighbor heights
    ///
    /// This creates smooth transitions/ramps automatically when neighbors
    /// are at different heights. The deformation is applied to top edges
    /// that border lower neighbors.
    ///
    /// # Arguments
    /// * `position` - Block position in world grid
    /// * `neighbor_heights` - Heights of neighbors in order [North, South, East, West]
    ///
    /// # Returns
    /// A `VoxelMesh` with deformed top edges and recalculated normals
    pub fn smoothed_mesh(
        position: BlockPos,
        neighbor_heights: [Option<i32>; 4], // [N, S, E, W]
    ) -> VoxelMesh {
        let current_height = position.y;
        let (mut verts, mut normals, indices) = Cube::unit_cube_indexed();

        // Check each direction for height differences
        let [north_h, south_h, east_h, west_h] = neighbor_heights;

        // If neighbor is lower by 1, deform that edge down to create ramp
        if north_h == Some(current_height - 1) {
            deform::deform_vertices_on_edge(&mut verts, Edge::North, -0.5);
        }
        if south_h == Some(current_height - 1) {
            deform::deform_vertices_on_edge(&mut verts, Edge::South, -0.5);
        }
        if east_h == Some(current_height - 1) {
            deform::deform_vertices_on_edge(&mut verts, Edge::East, -0.5);
        }
        if west_h == Some(current_height - 1) {
            deform::deform_vertices_on_edge(&mut verts, Edge::West, -0.5);
        }

        // Recalculate normals for deformed faces
        normals::recalculate_normals(&verts, &indices, &mut normals);

        VoxelMesh {
            vertices: verts,
            normals,
            indices,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;

    #[test]
    fn test_cube_mesh_generation() {
        let mesh = MeshGenerator::cube_mesh();

        // Standard cube has 24 vertices (4 per face, 6 faces)
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.normals.len(), 24);

        // 36 indices (6 per face, 6 faces)
        assert_eq!(mesh.indices.len(), 36);

        // Verify vertices are in unit cube range
        for vert in &mesh.vertices {
            for &coord in vert {
                assert!(coord >= -0.51 && coord <= 0.51);
            }
        }
    }

    #[test]
    fn test_smoothed_mesh_no_deformation() {
        let pos = IVec3::new(0, 10, 0);
        let neighbor_heights = [
            Some(10), // North at same height
            Some(10), // South at same height
            Some(10), // East at same height
            Some(10), // West at same height
        ];

        let mesh = MeshGenerator::smoothed_mesh(pos, neighbor_heights);
        let cube_mesh = MeshGenerator::cube_mesh();

        // Should be identical to cube mesh (no deformation)
        assert_eq!(mesh.vertices.len(), cube_mesh.vertices.len());
        assert_eq!(mesh.indices.len(), cube_mesh.indices.len());
    }

    #[test]
    fn test_smoothed_mesh_north_deformation() {
        let pos = IVec3::new(0, 10, 0);
        let neighbor_heights = [
            Some(9),  // North is 1 block lower
            Some(10), // South at same height
            Some(10), // East at same height
            Some(10), // West at same height
        ];

        let mesh = MeshGenerator::smoothed_mesh(pos, neighbor_heights);

        // Mesh should be deformed (different from standard cube)
        assert_eq!(mesh.vertices.len(), 24);

        // Check that some north edge vertices (z > 0.49, y > 0) were deformed
        let deformed_verts = mesh
            .vertices
            .iter()
            .filter(|v| v[2] > 0.49) // North edge (z = +0.5)
            .filter(|v| v[1] < 0.49) // Top vertices should be lowered
            .count();

        assert!(deformed_verts > 0, "North edge should be deformed");
    }

    #[test]
    fn test_smoothed_mesh_multiple_deformations() {
        let pos = IVec3::new(0, 10, 0);
        let neighbor_heights = [
            Some(9),  // North is 1 block lower
            Some(9),  // South is 1 block lower
            Some(10), // East at same height
            Some(10), // West at same height
        ];

        let mesh = MeshGenerator::smoothed_mesh(pos, neighbor_heights);

        // Check both north and south edges are deformed
        let north_deformed = mesh
            .vertices
            .iter()
            .filter(|v| v[2] > 0.49 && v[1] < 0.49)
            .count();
        let south_deformed = mesh
            .vertices
            .iter()
            .filter(|v| v[2] < -0.49 && v[1] < 0.49)
            .count();

        assert!(north_deformed > 0, "North edge should be deformed");
        assert!(south_deformed > 0, "South edge should be deformed");
    }

    #[test]
    fn test_normals_recalculated() {
        let pos = IVec3::new(0, 10, 0);
        let neighbor_heights = [Some(9), None, None, None];

        let mesh = MeshGenerator::smoothed_mesh(pos, neighbor_heights);

        // All normals should be normalized (length ≈ 1)
        for normal in &mesh.normals {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!((length - 1.0).abs() < 0.01, "Normal should be normalized");
        }
    }
}
