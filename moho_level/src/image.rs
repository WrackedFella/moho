//! Image decoding for base-colour textures and game UI icons.

use std::path::Path;

use moho_render_api::Rgba8Image;

use crate::LevelError;

/// Decode the PNG or JPEG at `path`, chosen by content, to straight-alpha RGBA8.
///
/// # Errors
/// A [`LevelError`] naming `path`: `Read` when the file cannot be read, `BadImage` for
/// any other format or undecodable bytes.
pub fn load_image(path: &Path) -> Result<Rgba8Image, LevelError> {
    let _ = path;
    panic!("load_image is not implemented")
}
