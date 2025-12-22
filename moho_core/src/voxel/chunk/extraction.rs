//! Face extraction and vertex remapping for optimized chunk meshes.

use super::super::face::FaceDirection;
use super::super::grid::VoxelMesh;
use std::collections::HashMap;

/// Extract only visible faces from a block mesh
///
/// This performs per-face extraction for maximum memory optimization.
/// Culls hidden faces and remaps vertices to minimize memory usage.
///
/// # Arguments
/// * `block_mesh` - Source block mesh with all faces
/// * `visible_faces` - List of faces that should be rendered
///
/// # Returns
/// Tuple of (vertices, normals, ao, geometry_type, indices) containing only visible geometry
pub fn extract_visible_faces(
    block_mesh: &VoxelMesh,
    visible_faces: &[FaceDirection],
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<f32>, Vec<u32>, Vec<u32>) {
    if visible_faces.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }

    // Check if all faces are visible (common case for isolated blocks)
    if visible_faces.len() == 6 {
        // All faces visible, return entire mesh (fast path)
        return (
            block_mesh.vertices.clone(),
            block_mesh.normals.clone(),
            block_mesh.ambient_occlusion.clone(),
            block_mesh.geometry_type.clone(),
            block_mesh.indices.clone(),
        );
    }

    // Some faces culled - extract only visible faces
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut ao = Vec::new();
    let mut geo_type = Vec::new();
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
                ao.push(block_mesh.ambient_occlusion[original_idx as usize]);
                geo_type.push(block_mesh.geometry_type[original_idx as usize]);

                next_vertex_id += 1;
            }
        }
    }

    (vertices, normals, ao, geo_type, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::Cube;

    fn make_full_cube_mesh() -> VoxelMesh {
        // Use real cube mesh from Cube::unit_cube_indexed()
        let (verts, normals, indices) = Cube::unit_cube_indexed();
        let ao = vec![1.0; verts.len()];
        let geometry_type = vec![1; verts.len()]; // Blocky geometry
        let light_level = vec![1.0; verts.len()]; // Full light
        VoxelMesh {
            vertices: verts,
            normals,
            indices,
            ambient_occlusion: ao,
            geometry_type,
            light_level,
        }
    }

    #[test]
    fn test_extract_all_faces() {
        let mesh = make_full_cube_mesh();
        let all_faces = vec![
            FaceDirection::PosX,
            FaceDirection::NegX,
            FaceDirection::PosY,
            FaceDirection::NegY,
            FaceDirection::PosZ,
            FaceDirection::NegZ,
        ];

        let (verts, normals, ao, indices, geo_type) = extract_visible_faces(&mesh, &all_faces);

        // Should return entire mesh (fast path)
        assert_eq!(verts.len(), mesh.vertices.len());
        assert_eq!(normals.len(), mesh.normals.len());
        assert_eq!(indices.len(), mesh.indices.len());
    }

    #[test]
    fn test_extract_no_faces() {
        let mesh = make_full_cube_mesh();
        let no_faces = vec![];

        let (verts, normals, ao, indices, geo_type) = extract_visible_faces(&mesh, &no_faces);

        // Should return empty mesh
        assert_eq!(verts.len(), 0);
        assert_eq!(normals.len(), 0);
        assert_eq!(indices.len(), 0);
    }

    #[test]
    fn test_extract_single_face() {
        let mesh = make_full_cube_mesh();
        let single_face = vec![FaceDirection::PosZ];

        let (verts, normals, ao, indices, geo_type) = extract_visible_faces(&mesh, &single_face);

        // Should extract only one face (6 indices for 2 triangles)
        assert_eq!(indices.len(), 6);
        // Should have 4 unique vertices (remapped from original 24)
        assert_eq!(verts.len(), 4);
        assert_eq!(normals.len(), 4);
    }

    #[test]
    fn test_extract_multiple_faces() {
        let mesh = make_full_cube_mesh();
        let two_faces = vec![FaceDirection::PosZ, FaceDirection::NegZ];

        let (verts, normals, ao, indices, geo_type) = extract_visible_faces(&mesh, &two_faces);

        // Should extract two faces (12 indices for 4 triangles)
        assert_eq!(indices.len(), 12);
        // Should have 8 unique vertices
        assert_eq!(verts.len(), 8);
        assert_eq!(normals.len(), 8);
    }

    #[test]
    fn test_vertex_remapping() {
        let mesh = make_full_cube_mesh();
        let single_face = vec![FaceDirection::PosZ];

        let (verts, _normals, _ao, indices, _geo_type) = extract_visible_faces(&mesh, &single_face);

        // All indices should be in range [0, verts.len())
        for &idx in &indices {
            assert!((idx as usize) < verts.len());
        }

        // Indices should be remapped to 0-based sequential IDs
        let max_idx = indices.iter().copied().max().unwrap();
        assert_eq!(max_idx + 1, verts.len() as u32);
    }

    #[test]
    fn test_extracted_geometry_matches_source() {
        let mesh = make_full_cube_mesh();
        let single_face = vec![FaceDirection::PosZ];

        let (verts, normals, _ao, _indices, _geo_type) = extract_visible_faces(&mesh, &single_face);

        // Extracted vertices should match original (order may differ due to remapping)
        assert_eq!(verts.len(), 4);
        assert_eq!(normals.len(), 4);

        // All extracted vertices should come from original mesh
        for vert in &verts {
            assert!(mesh.vertices.contains(vert));
        }
    }
}
