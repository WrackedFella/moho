//! Scene walk and primitive conversion.

use crate::{LevelMesh, error::LevelErrorKind};

#[expect(clippy::todo, dead_code, reason = "stub until implemented")]
pub(crate) fn build(
    _document: &gltf::Document,
    _buffers: &[Vec<u8>],
) -> Result<Vec<LevelMesh>, LevelErrorKind> {
    todo!("walk scene")
}
