//! World-geometry contract: indexed meshes keyed by a game-chosen id.

use std::fmt;

/// Stable, game-chosen key for one piece of world geometry.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorldMeshId(pub u64);

/// Why a [`WorldMesh`] was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldMeshError {
    /// A per-vertex channel does not have as many entries as `positions`.
    ChannelLengthMismatch {
        channel: &'static str,
        expected: usize,
        actual: usize,
    },
    /// An index points past the last vertex.
    IndexOutOfRange { index: u32, vertex_count: usize },
    /// The index count is not a multiple of three.
    PartialTriangle { index_count: usize },
}

impl fmt::Display for WorldMeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for WorldMeshError {}

/// Owned indexed mesh, one `Vec` per vertex channel.
#[derive(Clone, Debug)]
pub struct WorldMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    ao: Vec<f32>,
    light_rgb: Vec<[f32; 3]>,
    sky_exposure: Vec<f32>,
    surface: Vec<u32>,
    indices: Vec<u32>,
}

impl WorldMesh {
    pub fn new(
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        ao: Vec<f32>,
        light_rgb: Vec<[f32; 3]>,
        sky_exposure: Vec<f32>,
        surface: Vec<u32>,
        indices: Vec<u32>,
    ) -> Result<Self, WorldMeshError> {
        Ok(Self {
            positions,
            normals,
            ao,
            light_rgb,
            sky_exposure,
            surface,
            indices,
        })
    }

    pub fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }

    pub fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }

    pub fn ao(&self) -> &[f32] {
        &self.ao
    }

    pub fn light_rgb(&self) -> &[[f32; 3]] {
        &self.light_rgb
    }

    pub fn sky_exposure(&self) -> &[f32] {
        &self.sky_exposure
    }

    pub fn surface(&self) -> &[u32] {
        &self.surface
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// True when the mesh has no triangles.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(
        vertices: usize,
        ao_len: usize,
        indices: Vec<u32>,
    ) -> Result<WorldMesh, WorldMeshError> {
        WorldMesh::new(
            vec![[0.0; 3]; vertices],
            vec![[0.0, 1.0, 0.0]; vertices],
            vec![1.0; ao_len],
            vec![[1.0; 3]; vertices],
            vec![1.0; vertices],
            vec![0; vertices],
            indices,
        )
    }

    #[test]
    fn mismatched_channel_is_refused() {
        let err = build(3, 2, vec![0, 1, 2]).expect_err("ao is one entry short");

        assert!(
            matches!(
                err,
                WorldMeshError::ChannelLengthMismatch {
                    channel: "ao",
                    expected: 3,
                    actual: 2
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn index_out_of_range_is_refused() {
        let err = build(3, 3, vec![0, 1, 3]).expect_err("index 3 is past the last vertex");

        assert!(
            matches!(
                err,
                WorldMeshError::IndexOutOfRange {
                    index: 3,
                    vertex_count: 3
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn partial_triangle_is_refused() {
        let err = build(3, 3, vec![0, 1, 2, 0]).expect_err("four indices is not whole triangles");

        assert!(
            matches!(err, WorldMeshError::PartialTriangle { index_count: 4 }),
            "got {err:?}"
        );
    }

    #[test]
    fn valid_triangle_is_accepted_and_non_empty() {
        let mesh = build(3, 3, vec![0, 1, 2]).expect("well-formed triangle");

        assert!(!mesh.is_empty());
        assert!(build(0, 0, vec![]).expect("empty mesh is valid").is_empty());
    }
}
