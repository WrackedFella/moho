//! Face direction lookup tables for voxel rendering.
//!
//! This module provides:
//! - `FaceDirection` enum for cube faces
//! - Lookup tables for efficient face operations (replaces match statements)
//!
//! # Performance
//! Using const lookup tables instead of match statements reduces branching
//! and improves performance in tight rendering loops.

/// Direction of a cube face
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceDirection {
    PosX, // +X (right, east)
    NegX, // -X (left, west)
    PosY, // +Y (top, up)
    NegY, // -Y (bottom, down)
    PosZ, // +Z (front, north)
    NegZ, // -Z (back, south)
}

/// Lookup table for vertex ranges (start_index, count)
/// Standard cube has 24 vertices (4 per face) in order: +X, -X, +Y, -Y, +Z, -Z
const FACE_VERTEX_RANGES: [(usize, usize); 6] = [
    (0, 4),  // PosX: vertices 0-3
    (4, 4),  // NegX: vertices 4-7
    (8, 4),  // PosY: vertices 8-11
    (12, 4), // NegY: vertices 12-15
    (16, 4), // PosZ: vertices 16-19
    (20, 4), // NegZ: vertices 20-23
];

/// Lookup table for index ranges (start_index, count)
/// Standard cube has 36 indices (6 per face) in order: +X, -X, +Y, -Y, +Z, -Z
const FACE_INDEX_RANGES: [(usize, usize); 6] = [
    (0, 6),  // PosX: indices 0-5
    (6, 6),  // NegX: indices 6-11
    (12, 6), // PosY: indices 12-17
    (18, 6), // NegY: indices 18-23
    (24, 6), // PosZ: indices 24-29
    (30, 6), // NegZ: indices 30-35
];

impl FaceDirection {
    /// Get the vertex range for this face in a standard cube mesh
    ///
    /// Returns (start_index, count) for vertices of this face.
    /// Standard cube has 24 vertices (4 per face) in order: +X, -X, +Y, -Y, +Z, -Z
    #[inline]
    pub fn vertex_range(&self) -> (usize, usize) {
        FACE_VERTEX_RANGES[*self as usize]
    }

    /// Get the index range for this face in a standard cube mesh
    ///
    /// Returns (start_index, count) for indices of this face.
    /// Standard cube has 36 indices (6 per face) in order: +X, -X, +Y, -Y, +Z, -Z
    #[inline]
    pub fn index_range(&self) -> (usize, usize) {
        FACE_INDEX_RANGES[*self as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_ranges() {
        assert_eq!(FaceDirection::PosX.vertex_range(), (0, 4));
        assert_eq!(FaceDirection::NegX.vertex_range(), (4, 4));
        assert_eq!(FaceDirection::PosY.vertex_range(), (8, 4));
        assert_eq!(FaceDirection::NegY.vertex_range(), (12, 4));
        assert_eq!(FaceDirection::PosZ.vertex_range(), (16, 4));
        assert_eq!(FaceDirection::NegZ.vertex_range(), (20, 4));
    }

    #[test]
    fn test_index_ranges() {
        assert_eq!(FaceDirection::PosX.index_range(), (0, 6));
        assert_eq!(FaceDirection::NegX.index_range(), (6, 6));
        assert_eq!(FaceDirection::PosY.index_range(), (12, 6));
        assert_eq!(FaceDirection::NegY.index_range(), (18, 6));
        assert_eq!(FaceDirection::PosZ.index_range(), (24, 6));
        assert_eq!(FaceDirection::NegZ.index_range(), (30, 6));
    }
}
