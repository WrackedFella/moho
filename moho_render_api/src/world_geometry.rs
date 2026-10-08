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
        match self {
            Self::ChannelLengthMismatch {
                channel,
                expected,
                actual,
            } => write!(
                f,
                "channel `{channel}` has {actual} entries, expected {expected}"
            ),
            Self::IndexOutOfRange {
                index,
                vertex_count,
            } => {
                write!(
                    f,
                    "index {index} is past the last of {vertex_count} vertices"
                )
            }
            Self::PartialTriangle { index_count } => {
                write!(f, "index count {index_count} is not a multiple of three")
            }
        }
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
    /// Build a mesh, refusing mismatched channel lengths, indices past the last
    /// vertex, and index counts that are not whole triangles.
    pub fn new(
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        ao: Vec<f32>,
        light_rgb: Vec<[f32; 3]>,
        sky_exposure: Vec<f32>,
        surface: Vec<u32>,
        indices: Vec<u32>,
    ) -> Result<Self, WorldMeshError> {
        let expected = positions.len();
        let channels = [
            ("normals", normals.len()),
            ("ao", ao.len()),
            ("light_rgb", light_rgb.len()),
            ("sky_exposure", sky_exposure.len()),
            ("surface", surface.len()),
        ];
        for (channel, actual) in channels {
            if actual != expected {
                return Err(WorldMeshError::ChannelLengthMismatch {
                    channel,
                    expected,
                    actual,
                });
            }
        }
        if !indices.len().is_multiple_of(3) {
            return Err(WorldMeshError::PartialTriangle {
                index_count: indices.len(),
            });
        }
        if let Some(&index) = indices.iter().find(|&&i| i as usize >= expected) {
            return Err(WorldMeshError::IndexOutOfRange {
                index,
                vertex_count: expected,
            });
        }
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
    fn accessors_return_the_channels_it_was_built_with() {
        let positions = vec![[0.0, 0.5, 1.0], [2.0, 2.5, 3.0], [4.0, 4.5, 5.0]];
        let normals = vec![[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let ao = vec![0.25, 0.5, 0.75];
        let light_rgb = vec![[0.1, 0.2, 0.3], [0.4, 0.5, 0.6], [0.7, 0.8, 0.9]];
        let sky_exposure = vec![0.125, 0.375, 0.625];
        let surface = vec![7, 8, 9];
        let indices = vec![2, 1, 0];

        let mesh = WorldMesh::new(
            positions.clone(),
            normals.clone(),
            ao.clone(),
            light_rgb.clone(),
            sky_exposure.clone(),
            surface.clone(),
            indices.clone(),
        )
        .expect("well-formed triangle");

        assert_eq!(mesh.positions(), positions);
        assert_eq!(mesh.normals(), normals);
        assert_eq!(mesh.ao(), ao);
        assert_eq!(mesh.light_rgb(), light_rgb);
        assert_eq!(mesh.sky_exposure(), sky_exposure);
        assert_eq!(mesh.surface(), surface);
        assert_eq!(mesh.indices(), indices);
    }

    #[test]
    fn error_display_names_the_fault() {
        let mismatch = build(3, 2, vec![0, 1, 2])
            .expect_err("ao short")
            .to_string();
        let range = build(3, 3, vec![0, 1, 7]).expect_err("index 7").to_string();
        let partial = build(3, 3, vec![0, 1, 2, 0])
            .expect_err("four indices")
            .to_string();

        assert!(mismatch.contains("ao"), "got {mismatch}");
        assert!(
            mismatch.contains('2') && mismatch.contains('3'),
            "got {mismatch}"
        );
        assert!(range.contains('7'), "got {range}");
        assert!(range.contains('3'), "got {range}");
        assert!(partial.contains('4'), "got {partial}");
        assert!(partial.contains("three"), "got {partial}");
    }

    #[test]
    fn valid_triangle_is_accepted_and_non_empty() {
        let mesh = build(3, 3, vec![0, 1, 2]).expect("well-formed triangle");

        assert!(!mesh.is_empty());
        assert!(build(0, 0, vec![]).expect("empty mesh is valid").is_empty());
    }
}
