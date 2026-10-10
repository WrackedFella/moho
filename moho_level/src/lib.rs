//! Level loading: glTF files become world meshes the engine can draw and collide with.

mod buffers;
mod error;
mod mesh;

use std::path::Path;

pub use error::{LevelError, LevelErrorKind};
use moho_render_api::WorldMesh;

/// A loaded level: one entry per triangle primitive reached from the default scene.
#[derive(Debug)]
pub struct Level {
    pub meshes: Vec<LevelMesh>,
}

/// One world mesh with node transforms already applied.
#[derive(Debug)]
pub struct LevelMesh {
    pub mesh: WorldMesh,
    /// Name of the glTF material, if the primitive has a named one.
    pub material: Option<String>,
}

/// Load a `.gltf` or `.glb` level from `path`.
///
/// # Errors
/// Returns a [`LevelError`] naming `path` when the file cannot be read, is not
/// glTF, references an unreadable buffer, or holds a malformed mesh.
#[expect(clippy::todo, reason = "stub until implemented")]
pub fn load(_path: &Path) -> Result<Level, LevelError> {
    todo!("load glTF level")
}
