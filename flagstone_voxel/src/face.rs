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

impl FaceDirection {
    /// The axis the face is perpendicular to: 0 for X, 1 for Y, 2 for Z.
    pub(crate) fn axis(self) -> usize {
        match self {
            Self::PosX | Self::NegX => 0,
            Self::PosY | Self::NegY => 1,
            Self::PosZ | Self::NegZ => 2,
        }
    }

    /// Whether the face looks toward increasing coordinates on its axis.
    pub(crate) fn is_positive(self) -> bool {
        matches!(self, Self::PosX | Self::PosY | Self::PosZ)
    }
}
