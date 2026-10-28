//! A loaded level that owns the ids its meshes were registered under.

use std::sync::Arc;

use moho_render_api::{WorldMeshId, WorldMeshRange};

use crate::{AgentProfile, Level, NavBuildError, NavMesh};

/// A [`Level`] with the [`WorldMeshRange`] its meshes are registered under and, once built,
/// its navigation mesh.
#[derive(Debug)]
pub struct LoadedLevel {
    level: Level,
    ids: WorldMeshRange,
    nav: Option<Arc<NavMesh>>,
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
        })
    }

    pub fn level(&self) -> &Level {
        &self.level
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
