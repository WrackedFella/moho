//! Navigation over static levels: a walkable-area mesh built from world meshes for one
//! agent profile, and path queries over it.

mod build;
mod query;

use glam::Vec3;
use moho_render_api::WorldMesh;

/// The walking agent a navmesh is built for. Lengths in metres, slope in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgentProfile {
    /// Distance the agent keeps from walls.
    pub radius: f32,
    /// Floor-to-ceiling room the agent needs.
    pub height: f32,
    /// Tallest step the agent walks up or down.
    pub max_climb: f32,
    /// Steepest floor angle the agent walks on.
    pub max_slope: f32,
}

/// A convex walkable polygon: world-space corners, and for edge `i` (corner `i` to corner
/// `i + 1`, wrapping) the polygon on the other side, if any.
#[derive(Clone, Debug, PartialEq)]
struct Polygon {
    vertices: Vec<Vec3>,
    neighbours: Vec<Option<usize>>,
}

/// How far from a polygon a query point may sit and still snap onto it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SnapLimits {
    radius: f32,
    below: f32,
    above: f32,
}

/// The walkable area of a static level for one agent profile. Plain data; `Default` is empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavMesh {
    polygons: Vec<Polygon>,
    limits: SnapLimits,
}

impl NavMesh {
    /// Builds the walkable area of `meshes` for `profile`.
    ///
    /// # Errors
    /// [`NavBuildError`] when generation fails or the profile is not a walkable agent; an
    /// empty mesh list gives an empty navmesh.
    pub fn build<'a>(
        meshes: impl IntoIterator<Item = &'a WorldMesh>,
        profile: AgentProfile,
    ) -> Result<Self, NavBuildError> {
        let polygons = build::polygons(meshes, profile)?;
        if polygons.is_empty() {
            return Ok(Self::default());
        }
        Ok(Self {
            polygons,
            limits: SnapLimits {
                radius: profile.radius,
                below: profile.height,
                above: profile.max_climb,
            },
        })
    }

    /// Corners from `from` to `to`, both snapped onto the mesh, start and goal included;
    /// `None` when either end is off the mesh or no route joins them.
    ///
    /// A point snaps to the polygon whose surface lies at most the agent's height below it
    /// and at most its climb above it, nearest on the ground plane, then with the smallest
    /// vertical gap, then the lowest polygon index.
    pub fn path(&self, from: Vec3, to: Vec3) -> Option<Vec<Vec3>> {
        query::path(self, from, to)
    }
}

/// Why a navmesh could not be built.
#[derive(Debug, thiserror::Error)]
#[error("navmesh build failed: {0}")]
pub struct NavBuildError(String);
