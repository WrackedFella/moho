//! Image decoding for base-colour textures and game UI icons, and reading a model's glTF
//! images from its buffers or the files beside it.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use moho_render_api::Rgba8Image;

use crate::{LevelError, LevelErrorKind, buffers};

/// Decode the PNG or JPEG at `path`, chosen by content, to straight-alpha RGBA8.
///
/// # Errors
/// A [`LevelError`] naming `path`: `Read` when the file cannot be read, `BadImage` for
/// any other format or undecodable bytes.
pub fn load_image(path: &Path) -> Result<Rgba8Image, LevelError> {
    let fail = |kind| LevelError::new(path, kind);
    let bytes = std::fs::read(path).map_err(|e| fail(LevelErrorKind::Read(e)))?;
    let label = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned();
    decode_labelled(&bytes, label).map_err(fail)
}

/// Where a model's glTF images are read from: its resolved buffers and the directory
/// relative URIs resolve against.
pub(crate) struct ImageSources<'a> {
    pub(crate) buffers: &'a [Cow<'a, [u8]>],
    pub(crate) dir: &'a Path,
}

impl ImageSources<'_> {
    /// Read and decode `image`; returns the canonical path of a file image (`None` when
    /// embedded) with its pixels.
    pub(crate) fn load(
        &self,
        image: &gltf::Image<'_>,
    ) -> Result<(Option<PathBuf>, Rgba8Image), LevelErrorKind> {
        let ImageBytes {
            source,
            bytes,
            label,
        } = self.read(image)?;
        Ok((source, decode_labelled(&bytes, label)?))
    }

    /// The encoded bytes of `image`, from its buffer view or its file.
    fn read<'b>(&'b self, image: &gltf::Image<'_>) -> Result<ImageBytes<'b>, LevelErrorKind> {
        match image.source() {
            gltf::image::Source::View { view, .. } => {
                let label = format!("#{}", image.index());
                let range = view.offset()..view.offset() + view.length();
                let bytes = self
                    .buffers
                    .get(view.buffer().index())
                    .and_then(|buffer| buffer.get(range))
                    .ok_or_else(|| LevelErrorKind::BadImage {
                        image: label.clone(),
                        reason: "buffer view runs past the end of its buffer".into(),
                    })?;
                Ok(ImageBytes {
                    source: None,
                    bytes: Cow::Borrowed(bytes),
                    label,
                })
            }
            gltf::image::Source::Uri { uri, .. } => {
                let file = buffers::percent_decode(uri);
                if !buffers::is_relative_file(&file) {
                    return Err(LevelErrorKind::BadImage {
                        image: uri.to_owned(),
                        reason: "uri is not a relative file path".into(),
                    });
                }
                let missing = |source| LevelErrorKind::MissingImage {
                    uri: uri.to_owned(),
                    source,
                };
                let path = self.dir.join(file);
                let bytes = std::fs::read(&path).map_err(missing)?;
                let canonical = path.canonicalize().map_err(missing)?;
                Ok(ImageBytes {
                    source: Some(canonical),
                    bytes: Cow::Owned(bytes),
                    label: uri.to_owned(),
                })
            }
        }
    }
}

/// One image's encoded bytes, before decoding.
struct ImageBytes<'b> {
    /// Canonical path of a file image; `None` when embedded.
    source: Option<PathBuf>,
    bytes: Cow<'b, [u8]>,
    /// What errors name the image by: its URI, or `#<index>` when embedded.
    label: String,
}

/// [`decode`], failing as `BadImage` named `label`.
fn decode_labelled(bytes: &[u8], label: String) -> Result<Rgba8Image, LevelErrorKind> {
    decode(bytes).map_err(|reason| LevelErrorKind::BadImage {
        image: label,
        reason,
    })
}

/// Decode PNG or JPEG `bytes`, whatever their colour type or bit depth, to RGBA8. The
/// format comes from the bytes alone, never from a file name or declared mime type.
///
/// # Errors
/// Why the bytes are not a decodable PNG or JPEG.
fn decode(bytes: &[u8]) -> Result<Rgba8Image, String> {
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
    // Reuses the decoder's allocation; copies only if its capacity is not whole pixels.
    let pixels = bytemuck::try_cast_vec(rgba.into_raw())
        .unwrap_or_else(|(_, raw)| raw.as_chunks::<4>().0.to_vec());
    Rgba8Image::new(size, pixels).map_err(|e| e.to_string())
}
