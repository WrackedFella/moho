//! Face direction and culling logic for voxel rendering.
//!
//! This module provides:
//! - `FaceDirection` enum for cube faces
//! - Lookup tables for efficient face operations (replaces match statements)
//! - Face culling logic to hide faces between adjacent solid blocks
//!
//! # Performance
//! Using const lookup tables instead of match statements reduces branching
//! and improves performance in tight rendering loops.

use super::grid::{BlockPos, VoxelGrid};
use glam::IVec3;

/// Direction of a cube face for face culling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceDirection {
    PosX, // +X (right, east)
    NegX, // -X (left, west)
    PosY, // +Y (top, up)
    NegY, // -Y (bottom, down)
    PosZ, // +Z (front, north)
    NegZ, // -Z (back, south)
}

/// Lookup table for face offsets (replaces match in offset())
/// Order: PosX, NegX, PosY, NegY, PosZ, NegZ
const FACE_OFFSETS: [IVec3; 6] = [
    IVec3::new(1, 0, 0),  // PosX
    IVec3::new(-1, 0, 0), // NegX
    IVec3::new(0, 1, 0),  // PosY
    IVec3::new(0, -1, 0), // NegY
    IVec3::new(0, 0, 1),  // PosZ
    IVec3::new(0, 0, -1), // NegZ
];

impl FaceDirection {
    /// Get the offset vector for this face direction
    ///
    /// Uses a const lookup table for O(1) performance without branching.
    ///
    /// # Examples
    /// ```ignore
    /// use moho_core::voxel::FaceDirection;
    ///
    /// let offset = FaceDirection::PosX.offset();
    /// assert_eq!(offset, IVec3::new(1, 0, 0));
    /// ```
    #[inline]
    pub fn offset(&self) -> IVec3 {
        FACE_OFFSETS[*self as usize]
    }

    /// Get all six face directions
    ///
    /// Returns an array containing all possible face directions.
    /// Useful for iterating over all faces of a cube.
    pub fn all() -> [FaceDirection; 6] {
        [
            FaceDirection::PosX,
            FaceDirection::NegX,
            FaceDirection::PosY,
            FaceDirection::NegY,
            FaceDirection::PosZ,
            FaceDirection::NegZ,
        ]
    }

    /// Check if a face should be rendered (face culling optimization)
    ///
    /// Returns `false` if the neighbor block is solid (face is hidden).
    /// Returns `true` if no neighbor exists (air or out of bounds) - render the face.
    ///
    /// This is the core face culling algorithm that can reduce triangle count by 80-90%.
    ///
    /// # Arguments
    /// * `grid` - The voxel grid to query
    /// * `pos` - Position of the block whose face we're checking
    ///
    /// # Examples
    /// ```ignore
    /// use moho_core::voxel::{VoxelGrid, BlockPos, FaceDirection};
    ///
    /// let mut grid = VoxelGrid::new(16);
    /// let pos = BlockPos::new(0, 0, 0);
    /// grid.place_block(pos, 0, None);
    ///
    /// // No neighbor to the east (+X), so face should render
    /// assert!(FaceDirection::PosX.should_render_face(&grid, pos));
    ///
    /// // Add a neighbor
    /// let neighbor_pos = BlockPos::new(1, 0, 0);
    /// grid.place_block(neighbor_pos, 0, None);
    ///
    /// // Now the face is hidden by the neighbor
    /// assert!(!FaceDirection::PosX.should_render_face(&grid, pos));
    /// ```
    #[inline]
    pub fn should_render_face(&self, grid: &VoxelGrid, pos: BlockPos) -> bool {
        let neighbor_pos = pos + self.offset();

        // If neighbor exists (solid block), don't render this face (it's hidden)
        // If no neighbor (air or out of bounds), render the face
        !grid.is_solid_at(neighbor_pos)
    }
}

/// Get list of visible faces for a block (for face culling)
///
/// Returns only the faces that should be rendered based on neighbors.
/// This is a convenience function that checks all six faces.
///
/// # Arguments
/// * `grid` - The voxel grid
/// * `pos` - Position of the block
///
/// # Returns
/// Vector of face directions that are visible and should be rendered
pub fn get_visible_faces(grid: &VoxelGrid, pos: BlockPos) -> Vec<FaceDirection> {
    FaceDirection::all()
        .iter()
        .filter(|&&dir| dir.should_render_face(grid, pos))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::grid::VoxelGrid;

    #[test]
    fn test_face_offsets() {
        assert_eq!(FaceDirection::PosX.offset(), IVec3::new(1, 0, 0));
        assert_eq!(FaceDirection::NegX.offset(), IVec3::new(-1, 0, 0));
        assert_eq!(FaceDirection::PosY.offset(), IVec3::new(0, 1, 0));
        assert_eq!(FaceDirection::NegY.offset(), IVec3::new(0, -1, 0));
        assert_eq!(FaceDirection::PosZ.offset(), IVec3::new(0, 0, 1));
        assert_eq!(FaceDirection::NegZ.offset(), IVec3::new(0, 0, -1));
    }

    #[test]
    fn test_should_render_face_no_neighbor() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);
        grid.place_block(pos, 0, None);

        // All faces should be visible (no neighbors)
        assert!(FaceDirection::PosX.should_render_face(&grid, pos));
        assert!(FaceDirection::NegX.should_render_face(&grid, pos));
        assert!(FaceDirection::PosY.should_render_face(&grid, pos));
        assert!(FaceDirection::NegY.should_render_face(&grid, pos));
        assert!(FaceDirection::PosZ.should_render_face(&grid, pos));
        assert!(FaceDirection::NegZ.should_render_face(&grid, pos));
    }

    #[test]
    fn test_should_render_face_with_neighbor() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);
        grid.place_block(pos, 0, None);

        // Add neighbor to the east (+X)
        let neighbor_pos = BlockPos::new(1, 0, 0);
        grid.place_block(neighbor_pos, 0, None);

        // PosX face should be hidden
        assert!(!FaceDirection::PosX.should_render_face(&grid, pos));

        // Other faces still visible
        assert!(FaceDirection::NegX.should_render_face(&grid, pos));
        assert!(FaceDirection::PosY.should_render_face(&grid, pos));
        assert!(FaceDirection::NegY.should_render_face(&grid, pos));
        assert!(FaceDirection::PosZ.should_render_face(&grid, pos));
        assert!(FaceDirection::NegZ.should_render_face(&grid, pos));
    }

    #[test]
    fn test_get_visible_faces_with_neighbors() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(10, 10, 10);
        grid.place_block(pos, 0, None);

        // Add two neighbors
        grid.place_block(BlockPos::new(11, 10, 10), 0, None);
        grid.place_block(BlockPos::new(10, 11, 10), 0, None);

        let visible = get_visible_faces(&grid, pos);
        assert_eq!(visible.len(), 4); // 4 faces visible (2 hidden)

        // Verify the correct faces are visible
        assert!(visible.contains(&FaceDirection::NegX));
        assert!(visible.contains(&FaceDirection::NegY));
        assert!(visible.contains(&FaceDirection::PosZ));
        assert!(visible.contains(&FaceDirection::NegZ));

        // Verify the correct faces are hidden
        assert!(!visible.contains(&FaceDirection::PosX));
        assert!(!visible.contains(&FaceDirection::PosY));
    }

    #[test]
    fn test_get_visible_faces_fully_surrounded() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(5, 5, 5);
        grid.place_block(pos, 0, None);

        // Surround completely
        for dir in FaceDirection::all() {
            let neighbor_pos = pos + dir.offset();
            grid.place_block(neighbor_pos, 0, None);
        }

        let visible = get_visible_faces(&grid, pos);
        assert_eq!(visible.len(), 0); // No faces visible
    }
}
