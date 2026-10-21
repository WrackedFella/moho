//! Navigation over static levels: a walkable-area mesh built from world meshes for one
//! agent profile, and path queries over it.

use glam::Vec3;
use moho_render_api::WorldMesh;

/// The walking agent a navmesh is built for. Lengths in metres, slope in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgentProfile {
    pub radius: f32,
    pub height: f32,
    pub max_climb: f32,
    pub max_slope: f32,
}

/// The walkable area of a static level for one agent profile. Plain data; `Default` is empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavMesh {
    polygons: Vec<Vec<Vec3>>,
}

impl NavMesh {
    /// # Errors
    /// [`NavBuildError`] when generation fails; an empty mesh list gives an empty navmesh.
    pub fn build<'a>(
        _meshes: impl IntoIterator<Item = &'a WorldMesh>,
        _profile: AgentProfile,
    ) -> Result<Self, NavBuildError> {
        Ok(Self::default())
    }

    /// Corners from `from` to `to`, both snapped onto the mesh, start and goal included;
    /// `None` when either end is off the mesh or no route joins them.
    pub fn path(&self, _from: Vec3, _to: Vec3) -> Option<Vec<Vec3>> {
        None
    }
}

/// Why a navmesh could not be built.
#[derive(Debug, thiserror::Error)]
#[error("navmesh build failed: {0}")]
pub struct NavBuildError(String);
