//! Normal recalculation for deformed meshes.

use glam::Vec3;

/// Recalculate normals for a mesh after vertex deformation
///
/// Uses face normals averaged across shared vertices to produce
/// smooth shading results. This is necessary after deforming vertices
/// to ensure proper lighting.
///
/// # Arguments
/// * `verts` - Vertex positions
/// * `indices` - Triangle indices
/// * `normals` - Mutable normal array to update
pub fn recalculate_normals(verts: &[[f32; 3]], indices: &[u32], normals: &mut [[f32; 3]]) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_triangle() -> (Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 3]>) {
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0, 1, 2];
        let normals = vec![[0.0, 0.0, 0.0]; 3]; // Placeholder normals
        (verts, indices, normals)
    }

    #[test]
    fn test_recalculate_normals_triangle() {
        let (verts, indices, mut normals) = make_simple_triangle();

        recalculate_normals(&verts, &indices, &mut normals);

        // All normals should be normalized (length = 1)
        for normal in &normals {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!(
                (length - 1.0).abs() < 0.01,
                "Normal should be normalized, got length {}",
                length
            );
        }

        // For a triangle in XY plane, normal should point along Z axis
        for normal in &normals {
            assert!(
                (normal[2].abs() - 1.0).abs() < 0.01,
                "Normal should point along Z axis"
            );
        }
    }

    #[test]
    fn test_recalculate_normals_quad() {
        // Two triangles forming a quad in XY plane
        let verts = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let indices = vec![
            0, 1, 2, // First triangle
            0, 2, 3, // Second triangle
        ];
        let mut normals = vec![[0.0, 0.0, 0.0]; 4];

        recalculate_normals(&verts, &indices, &mut normals);

        // All normals should be normalized
        for normal in &normals {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!((length - 1.0).abs() < 0.01);
        }

        // All normals should point along +Z (0, 0, 1)
        for normal in &normals {
            assert!((normal[0]).abs() < 0.01);
            assert!((normal[1]).abs() < 0.01);
            assert!((normal[2] - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_recalculate_normals_deformed_mesh() {
        // Simulate deformed mesh (slanted face)
        let verts = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.5, 0.0], // Raised vertex
            [0.0, 1.0, 0.0],
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        let mut normals = vec![[0.0, 0.0, 0.0]; 4];

        recalculate_normals(&verts, &indices, &mut normals);

        // All normals should be normalized
        for normal in &normals {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!((length - 1.0).abs() < 0.01);
        }

        // Normals should still point mostly along +Z but with some Y component
        for normal in &normals {
            assert!(normal[2] > 0.5, "Normal should point mostly along +Z");
        }
    }

    #[test]
    fn test_recalculate_normals_empty_mesh() {
        let verts = vec![];
        let indices = vec![];
        let mut normals = vec![];

        // Should not panic on empty mesh
        recalculate_normals(&verts, &indices, &mut normals);
    }
}
