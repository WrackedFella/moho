//! Vertex deformation for terrain smoothing.

/// Edge of a voxel block's top face
#[derive(Debug, Clone, Copy)]
pub enum Edge {
    North, // +Z
    South, // -Z
    East,  // +X
    West,  // -X
}

/// Deform vertices on a specific edge by a given offset
///
/// This creates smooth ramps/transitions by lowering edge vertices
/// when neighboring blocks are at lower heights.
///
/// # Arguments
/// * `verts` - Mutable vertex array to modify
/// * `edge` - Which edge to deform (North, South, East, West)
/// * `offset` - Y-axis offset to apply to edge vertices (typically -0.5 for ramps)
pub fn deform_vertices_on_edge(verts: &mut [[f32; 3]], edge: Edge, offset: f32) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cube_top_face() -> Vec<[f32; 3]> {
        // Simplified cube top vertices for testing
        vec![
            [-0.5, 0.5, 0.5],  // Top NW
            [0.5, 0.5, 0.5],   // Top NE
            [0.5, 0.5, -0.5],  // Top SE
            [-0.5, 0.5, -0.5], // Top SW
            // Add some bottom vertices that shouldn't be affected
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
        ]
    }

    #[test]
    fn test_deform_north_edge() {
        let mut verts = make_cube_top_face();
        let original = verts.clone();

        deform_vertices_on_edge(&mut verts, Edge::North, -0.5);

        // North edge vertices (z > 0.49, y > 0.49) should be lowered
        assert_eq!(verts[0][1], 0.0); // Top NW lowered
        assert_eq!(verts[1][1], 0.0); // Top NE lowered

        // Other edges should remain unchanged
        assert_eq!(verts[2], original[2]); // Top SE unchanged
        assert_eq!(verts[3], original[3]); // Top SW unchanged
        assert_eq!(verts[4], original[4]); // Bottom unchanged
    }

    #[test]
    fn test_deform_south_edge() {
        let mut verts = make_cube_top_face();

        deform_vertices_on_edge(&mut verts, Edge::South, -0.5);

        // South edge vertices (z < -0.49) should be lowered
        assert_eq!(verts[2][1], 0.0); // Top SE lowered
        assert_eq!(verts[3][1], 0.0); // Top SW lowered

        // North edge should remain unchanged
        assert_eq!(verts[0][1], 0.5); // Top NW unchanged
        assert_eq!(verts[1][1], 0.5); // Top NE unchanged
    }

    #[test]
    fn test_deform_east_edge() {
        let mut verts = make_cube_top_face();

        deform_vertices_on_edge(&mut verts, Edge::East, -0.5);

        // East edge vertices (x > 0.49) should be lowered
        assert_eq!(verts[1][1], 0.0); // Top NE lowered
        assert_eq!(verts[2][1], 0.0); // Top SE lowered

        // West edge should remain unchanged
        assert_eq!(verts[0][1], 0.5); // Top NW unchanged
        assert_eq!(verts[3][1], 0.5); // Top SW unchanged
    }

    #[test]
    fn test_deform_west_edge() {
        let mut verts = make_cube_top_face();

        deform_vertices_on_edge(&mut verts, Edge::West, -0.5);

        // West edge vertices (x < -0.49) should be lowered
        assert_eq!(verts[0][1], 0.0); // Top NW lowered
        assert_eq!(verts[3][1], 0.0); // Top SW lowered

        // East edge should remain unchanged
        assert_eq!(verts[1][1], 0.5); // Top NE unchanged
        assert_eq!(verts[2][1], 0.5); // Top SE unchanged
    }

    #[test]
    fn test_deform_custom_offset() {
        let mut verts = make_cube_top_face();

        deform_vertices_on_edge(&mut verts, Edge::North, -0.25);

        // North edge vertices should be lowered by 0.25
        assert_eq!(verts[0][1], 0.25); // Top NW lowered to 0.25
        assert_eq!(verts[1][1], 0.25); // Top NE lowered to 0.25
    }
}
