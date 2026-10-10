//! Buffer resolution: GLB blob or relative file URIs.

use std::path::Path;

use crate::error::LevelErrorKind;

#[expect(clippy::todo, dead_code, reason = "stub until implemented")]
pub(crate) fn resolve(_document: &gltf::Gltf, _dir: &Path) -> Result<Vec<Vec<u8>>, LevelErrorKind> {
    todo!("resolve buffers")
}
