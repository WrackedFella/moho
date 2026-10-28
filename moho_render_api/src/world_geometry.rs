//! World-geometry contract: indexed meshes keyed by a game-chosen id.

use std::fmt;

/// Stable, game-chosen key for one piece of world geometry.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorldMeshId(pub u64);

/// A run of consecutive [`WorldMeshId`]s: `first`, `first + 1`, ... for `len` ids.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WorldMeshRange {
    first: WorldMeshId,
    len: u32,
}

impl WorldMeshRange {
    /// The `len` ids starting at `first`; `None` when they would pass `u64::MAX`.
    pub fn new(first: WorldMeshId, len: u32) -> Option<Self> {
        let _ = (first, len);
        todo!("WorldMeshRange::new")
    }

    /// The ids in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = WorldMeshId> + use<> {
        let _ = self;
        todo!("WorldMeshRange::iter");
        #[allow(unreachable_code)]
        std::iter::empty()
    }

    /// Whether `id` is one of the range's ids.
    pub fn contains(&self, id: WorldMeshId) -> bool {
        let _ = (self, id);
        todo!("WorldMeshRange::contains")
    }

    /// How many ids the range holds.
    pub fn len(&self) -> u32 {
        todo!("WorldMeshRange::len")
    }

    /// True when the range holds no ids.
    pub fn is_empty(&self) -> bool {
        todo!("WorldMeshRange::is_empty")
    }
}

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
    materials: Option<Vec<u32>>,
    uvs: Option<Vec<[f32; 2]>>,
    skin: Option<Skin>,
}

#[derive(Clone, Debug)]
struct Skin {
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
}

type SkinChannels<'a> = (&'a [[u16; 4]], &'a [[f32; 4]]);

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
            materials: None,
            uvs: None,
            skin: None,
        })
    }

    /// Attach a per-vertex material-table index channel (the same index space
    /// as the `material_idx` given to world-mesh upserts). One material per
    /// triangle: all three vertices should agree; a mixed triangle draws in
    /// one of its vertices' materials, unspecified.
    pub fn with_materials(mut self, materials: Vec<u32>) -> Result<Self, WorldMeshError> {
        if materials.len() != self.positions.len() {
            return Err(WorldMeshError::ChannelLengthMismatch {
                channel: "materials",
                expected: self.positions.len(),
                actual: materials.len(),
            });
        }
        self.materials = Some(materials);
        Ok(self)
    }

    /// The per-vertex material channel; `None` when the game never set it.
    pub fn materials(&self) -> Option<&[u32]> {
        self.materials.as_deref()
    }

    /// Attach per-vertex texture coordinates, one `[u, v]` per vertex.
    pub fn with_uvs(mut self, uvs: Vec<[f32; 2]>) -> Result<Self, WorldMeshError> {
        if uvs.len() != self.positions.len() {
            return Err(WorldMeshError::ChannelLengthMismatch {
                channel: "uvs",
                expected: self.positions.len(),
                actual: uvs.len(),
            });
        }
        self.uvs = Some(uvs);
        Ok(self)
    }

    /// The per-vertex texture coordinates; `None` when the game never set them.
    pub fn uvs(&self) -> Option<&[[f32; 2]]> {
        self.uvs.as_deref()
    }

    /// Attach the skin: four joint indices and their weights per vertex. Joint indices
    /// address the palette a placement supplies; weights should sum to one.
    pub fn with_skin(
        mut self,
        joints: Vec<[u16; 4]>,
        weights: Vec<[f32; 4]>,
    ) -> Result<Self, WorldMeshError> {
        let expected = self.positions.len();
        if joints.len() != expected {
            return Err(WorldMeshError::ChannelLengthMismatch {
                channel: "joints",
                expected,
                actual: joints.len(),
            });
        }
        if weights.len() != expected {
            return Err(WorldMeshError::ChannelLengthMismatch {
                channel: "weights",
                expected,
                actual: weights.len(),
            });
        }
        self.skin = Some(Skin { joints, weights });
        Ok(self)
    }

    /// The per-vertex joints and weights; `None` when the game never set a skin.
    pub fn skin(&self) -> Option<SkinChannels<'_>> {
        self.skin
            .as_ref()
            .map(|skin| (skin.joints.as_slice(), skin.weights.as_slice()))
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
    fn range_yields_consecutive_ids_from_first() {
        let range = WorldMeshRange::new(WorldMeshId(40), 3).expect("fits in u64");

        let ids: Vec<WorldMeshId> = range.iter().collect();

        assert_eq!(ids, [WorldMeshId(40), WorldMeshId(41), WorldMeshId(42)]);
        assert_eq!(range.len(), 3);
        assert!(!range.is_empty());
        assert!(range.contains(WorldMeshId(40)));
        assert!(range.contains(WorldMeshId(42)));
        assert!(!range.contains(WorldMeshId(39)), "one before first");
        assert!(!range.contains(WorldMeshId(43)), "one past the last");
    }

    #[test]
    fn range_refuses_ids_past_u64_max() {
        assert!(WorldMeshRange::new(WorldMeshId(u64::MAX), 2).is_none());
        assert!(WorldMeshRange::new(WorldMeshId(u64::MAX - 1), 5).is_none());
        assert!(WorldMeshRange::new(WorldMeshId(u64::MAX - 5), 5).is_some());
        assert!(WorldMeshRange::new(WorldMeshId(u64::MAX - 4), 5).is_none());

        let top = WorldMeshRange::new(WorldMeshId(u64::MAX - 10), 5).expect("ids stay below max");

        assert_eq!(top.iter().last(), Some(WorldMeshId(u64::MAX - 6)));
    }

    #[test]
    fn empty_range_yields_nothing() {
        let range = WorldMeshRange::new(WorldMeshId(7), 0).expect("an empty range always fits");

        assert!(range.is_empty());
        assert_eq!(range.len(), 0);
        assert_eq!(range.iter().count(), 0);
        assert!(!range.contains(WorldMeshId(7)));
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

    #[test]
    fn with_materials_refuses_a_mismatched_channel() {
        let mesh = build(3, 3, vec![0, 1, 2]).expect("well-formed triangle");

        let err = mesh
            .with_materials(vec![1, 1])
            .expect_err("two materials for three vertices");

        assert!(
            matches!(
                err,
                WorldMeshError::ChannelLengthMismatch {
                    channel: "materials",
                    expected: 3,
                    actual: 2
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn with_uvs_refuses_a_mismatched_channel() {
        let mesh = build(3, 3, vec![0, 1, 2]).expect("well-formed triangle");

        let err = mesh
            .with_uvs(vec![[0.0, 0.0], [1.0, 0.0]])
            .expect_err("two uvs for three vertices");

        assert!(
            matches!(
                err,
                WorldMeshError::ChannelLengthMismatch {
                    channel: "uvs",
                    expected: 3,
                    actual: 2
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn with_skin_refuses_mismatched_channels() {
        let mesh = || build(3, 3, vec![0, 1, 2]).expect("well-formed triangle");
        let joints = vec![[0, 1, 0, 0]; 3];
        let weights = vec![[0.5, 0.5, 0.0, 0.0]; 3];
        assert!(mesh().skin().is_none(), "no skin until set");

        let short_joints = mesh()
            .with_skin(vec![[0; 4]; 2], weights.clone())
            .expect_err("two joint sets for three vertices");
        let short_weights = mesh()
            .with_skin(joints.clone(), vec![[1.0, 0.0, 0.0, 0.0]; 4])
            .expect_err("four weight sets for three vertices");
        let skinned = mesh()
            .with_skin(joints.clone(), weights.clone())
            .expect("one per vertex");

        assert!(
            matches!(
                short_joints,
                WorldMeshError::ChannelLengthMismatch {
                    channel: "joints",
                    expected: 3,
                    actual: 2
                }
            ),
            "got {short_joints:?}"
        );
        assert!(
            matches!(
                short_weights,
                WorldMeshError::ChannelLengthMismatch {
                    channel: "weights",
                    expected: 3,
                    actual: 4
                }
            ),
            "got {short_weights:?}"
        );
        assert_eq!(skinned.skin(), Some((&joints[..], &weights[..])));
        assert_eq!(skinned.indices(), [0, 1, 2]);
    }

    #[test]
    fn uvs_are_none_until_set_and_returned_as_given() {
        let mesh = build(3, 3, vec![0, 1, 2]).expect("well-formed triangle");
        assert!(mesh.uvs().is_none());

        let mesh = mesh
            .with_uvs(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]])
            .expect("one per vertex");

        assert_eq!(mesh.uvs(), Some(&[[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]][..]));
        assert_eq!(mesh.indices(), [0, 1, 2]);
    }

    #[test]
    fn materials_are_none_until_set_and_returned_as_given() {
        let mesh = build(3, 3, vec![0, 1, 2]).expect("well-formed triangle");
        assert!(mesh.materials().is_none());

        let mesh = mesh.with_materials(vec![4, 4, 4]).expect("one per vertex");

        assert_eq!(mesh.materials(), Some(&[4, 4, 4][..]));
        assert_eq!(mesh.indices(), [0, 1, 2]);
    }
}
