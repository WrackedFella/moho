//! A loaded level that owns the ids its meshes were registered under.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use moho_render_api::{WorldMeshId, WorldMeshRange};

use crate::{AgentProfile, Level, LevelError, LevelErrorKind, LevelHash, NavBuildError, NavMesh};

/// A [`Level`] with the [`WorldMeshRange`] its meshes are registered under and, once built,
/// its navigation mesh.
#[derive(Debug)]
pub struct LoadedLevel {
    level: Level,
    ids: WorldMeshRange,
    nav: Option<Arc<NavMesh>>,
    source: Option<(PathBuf, LevelHash)>,
}

impl LoadedLevel {
    /// Mesh `i` of `level` gets id `first + i`; `None` when those ids would pass `u64::MAX`.
    pub fn new(level: Level, first: WorldMeshId) -> Option<Self> {
        let len = u32::try_from(level.meshes.len()).ok()?;
        let ids = WorldMeshRange::new(first, len)?;
        Some(Self {
            level,
            ids,
            nav: None,
            source: None,
        })
    }

    pub fn level(&self) -> &Level {
        &self.level
    }

    /// Load the level at `path`, pinned to `pin` when given, and register its meshes from `first`.
    ///
    /// # Errors
    /// As [`crate::load_pinned`] when `pin` is given, otherwise as [`crate::load`];
    /// [`LevelErrorKind::IdRangeOverflow`](crate::LevelErrorKind::IdRangeOverflow) when the ids
    /// would pass `u64::MAX`.
    pub fn open(
        path: &Path,
        pin: Option<&LevelHash>,
        first: WorldMeshId,
    ) -> Result<Self, LevelError> {
        let (level, hash) = crate::load_hashed(path, pin)?;
        let mut loaded = Self::new(level, first)
            .ok_or_else(|| LevelError::new(path, LevelErrorKind::IdRangeOverflow))?;
        loaded.source = Some((path.to_path_buf(), hash));
        Ok(loaded)
    }

    /// The file this level was opened from; `None` for a handle made with [`Self::new`].
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.source.as_ref().map(|(path, _)| path.as_path())
    }

    /// The content hash of the file this level was opened from; `None` for [`Self::new`].
    #[must_use]
    pub fn hash(&self) -> Option<&LevelHash> {
        self.source.as_ref().map(|(_, hash)| hash)
    }

    /// The ids of every mesh of the level.
    pub fn ids(&self) -> WorldMeshRange {
        self.ids
    }

    /// The id of mesh `index`; `None` past the last mesh.
    pub fn mesh_id(&self, index: usize) -> Option<WorldMeshId> {
        self.ids.iter().nth(index)
    }

    /// Build and keep the navigation mesh of the level for `profile`.
    ///
    /// # Errors
    /// [`NavBuildError`] when generation fails or the profile is not a walkable agent.
    pub fn build_nav(&mut self, profile: AgentProfile) -> Result<Arc<NavMesh>, NavBuildError> {
        let nav = Arc::new(NavMesh::build(
            self.level.meshes.iter().map(|m| &m.mesh),
            profile,
        )?);
        self.nav = Some(Arc::clone(&nav));
        Ok(nav)
    }

    /// Drop the level, its markers and its navigation reference, and return the ids the
    /// caller must remove from every store it registered them in.
    #[must_use]
    pub fn unload(self) -> WorldMeshRange {
        self.ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LevelMesh;
    use moho_render_api::WorldMesh;

    fn floor(y: f32) -> WorldMesh {
        WorldMesh::new(
            vec![
                [-10.0, y, -10.0],
                [10.0, y, -10.0],
                [10.0, y, 10.0],
                [-10.0, y, 10.0],
            ],
            vec![[0.0, 1.0, 0.0]; 4],
            vec![1.0; 4],
            vec![[0.0; 3]; 4],
            vec![1.0; 4],
            vec![0; 4],
            vec![0, 2, 1, 0, 3, 2],
        )
        .expect("valid floor mesh")
    }

    fn level(meshes: usize) -> Level {
        Level {
            meshes: (0..meshes)
                .map(|i| LevelMesh {
                    mesh: floor(i as f32 * 5.0),
                    material: None,
                })
                .collect(),
            markers: Vec::new(),
        }
    }

    const PROFILE: AgentProfile = AgentProfile {
        radius: 0.3,
        height: 1.8,
        max_climb: 0.4,
        max_slope: 0.8,
    };

    #[test]
    fn mesh_ids_follow_mesh_order_from_first() {
        let loaded = LoadedLevel::new(level(3), WorldMeshId(100)).expect("ids fit");

        let by_index: Vec<Option<WorldMeshId>> = (0..4).map(|i| loaded.mesh_id(i)).collect();

        assert_eq!(
            by_index,
            [
                Some(WorldMeshId(100)),
                Some(WorldMeshId(101)),
                Some(WorldMeshId(102)),
                None
            ],
            "mesh i is first + i; past the last there is no id"
        );
        assert_eq!(
            loaded.ids(),
            WorldMeshRange::new(WorldMeshId(100), 3).expect("fits")
        );
        assert_eq!(loaded.level().meshes.len(), 3);
    }

    #[test]
    fn new_refuses_ids_past_u64_max() {
        assert!(LoadedLevel::new(level(2), WorldMeshId(u64::MAX)).is_none());
        assert!(LoadedLevel::new(level(2), WorldMeshId(u64::MAX - 10)).is_some());
    }

    #[test]
    fn unload_returns_the_ids_it_owned() {
        let loaded = LoadedLevel::new(level(4), WorldMeshId(7)).expect("ids fit");
        let owned = loaded.ids();

        let returned = loaded.unload();

        assert_eq!(returned, owned);
        let ids: Vec<WorldMeshId> = returned.iter().collect();
        assert_eq!(
            ids,
            [
                WorldMeshId(7),
                WorldMeshId(8),
                WorldMeshId(9),
                WorldMeshId(10)
            ]
        );
    }

    fn write_empty_level(dir: &tempfile::TempDir, json: &str) -> std::path::PathBuf {
        let path = dir.path().join("empty.gltf");
        std::fs::write(&path, json).expect("write level");
        path
    }

    const EMPTY_LEVEL: &str = r#"{"asset":{"version":"2.0"}}"#;

    #[test]
    fn open_records_its_path_and_hash() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write_empty_level(&dir, EMPTY_LEVEL);

        let loaded = LoadedLevel::open(&path, None, WorldMeshId(5)).expect("opens");

        assert_eq!(loaded.path(), Some(path.as_path()));
        let expected = LevelHash::of_sources([EMPTY_LEVEL.as_bytes()]);
        assert_eq!(loaded.hash(), Some(&expected));
    }

    #[test]
    fn new_records_no_path_or_hash() {
        let loaded = LoadedLevel::new(level(1), WorldMeshId(0)).expect("ids fit");

        assert!(loaded.path().is_none());
        assert!(loaded.hash().is_none());
    }

    #[test]
    fn open_with_a_pin_of_a_different_hash_is_a_mismatch() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write_empty_level(&dir, EMPTY_LEVEL);
        let other = LevelHash::of_sources([b"something else".as_slice()]);

        let err = LoadedLevel::open(&path, Some(&other), WorldMeshId(0)).expect_err("refused");

        assert_eq!(err.path(), path.as_path());
        match err.kind() {
            crate::LevelErrorKind::HashMismatch { expected, found } => {
                assert_eq!(*expected, other);
                assert_eq!(*found, LevelHash::of_sources([EMPTY_LEVEL.as_bytes()]));
            }
            kind => panic!("expected a hash mismatch, got {kind:?}"),
        }
    }

    #[test]
    fn nav_is_freed_when_the_level_unloads() {
        let mut loaded = LoadedLevel::new(level(1), WorldMeshId(0)).expect("ids fit");
        let nav = loaded.build_nav(PROFILE).expect("nav builds");
        let weak = Arc::downgrade(&nav);
        drop(nav);
        assert!(weak.upgrade().is_some(), "the handle keeps its nav alive");

        let _ids = loaded.unload();

        assert!(weak.upgrade().is_none(), "unload released the last clone");
    }
}
