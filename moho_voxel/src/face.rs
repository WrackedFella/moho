//! Face directions for voxel rendering.

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
