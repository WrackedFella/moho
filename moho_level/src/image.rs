//! Image decoding for base-colour textures and game UI icons.

use std::path::Path;

use moho_render_api::Rgba8Image;

use crate::{LevelError, LevelErrorKind};

/// Decode the PNG or JPEG at `path`, chosen by content, to straight-alpha RGBA8.
///
/// # Errors
/// A [`LevelError`] naming `path`: `Read` when the file cannot be read, `BadImage` for
/// any other format or undecodable bytes.
pub fn load_image(path: &Path) -> Result<Rgba8Image, LevelError> {
    let fail = |kind| LevelError::new(path, kind);
    let bytes = std::fs::read(path).map_err(|e| fail(LevelErrorKind::Read(e)))?;
    decode(&bytes).map_err(|reason| {
        let image = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned();
        fail(LevelErrorKind::BadImage { image, reason })
    })
}

/// Decode PNG or JPEG `bytes`, whatever their colour type or bit depth, to RGBA8. The
/// format comes from the bytes alone, never from a file name or declared mime type.
///
/// # Errors
/// Why the bytes are not a decodable PNG or JPEG.
pub(crate) fn decode(bytes: &[u8]) -> Result<Rgba8Image, String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err(format!(
            "{format:?} images are unsupported, only PNG and JPEG"
        ));
    }
    let rgba = image::load_from_memory_with_format(bytes, format)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let size = [rgba.width(), rgba.height()];
    let pixels = rgba.into_raw().as_chunks::<4>().0.to_vec();
    Rgba8Image::new(size, pixels).map_err(|e| e.to_string())
}
