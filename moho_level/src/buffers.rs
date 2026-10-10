//! Buffer resolution: GLB blob or relative file URIs.

use std::borrow::Cow;
use std::path::{Component, Path};

use crate::error::LevelErrorKind;

/// Load every buffer of `document`, in index order; the GLB `blob` is borrowed.
pub(crate) fn resolve<'b>(
    document: &gltf::Document,
    blob: Option<&'b [u8]>,
    dir: &Path,
) -> Result<Vec<Cow<'b, [u8]>>, LevelErrorKind> {
    document
        .buffers()
        .map(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => blob.map(Cow::Borrowed).ok_or_else(|| {
                LevelErrorKind::NotGltf(
                    format!(
                        "buffer {} has no uri and the file has no GLB blob",
                        buffer.index()
                    )
                    .into(),
                )
            }),
            gltf::buffer::Source::Uri(uri) => {
                let file = percent_decode(uri);
                if !is_relative_file(&file) {
                    return Err(LevelErrorKind::UnsupportedBufferUri {
                        uri: uri.to_owned(),
                    });
                }
                std::fs::read(dir.join(file))
                    .map(Cow::Owned)
                    .map_err(|source| LevelErrorKind::MissingBuffer {
                        uri: uri.to_owned(),
                        source,
                    })
            }
        })
        .collect()
}

/// Checked after decoding, so an escaped `/` or scheme cannot slip past.
fn is_relative_file(file: &str) -> bool {
    let has_scheme = file
        .split('/')
        .next()
        .is_some_and(|first| first.contains(':'));
    let rooted = Path::new(file)
        .components()
        .any(|c| matches!(c, Component::Prefix(_) | Component::RootDir));
    !has_scheme && !rooted
}

fn percent_decode(uri: &str) -> String {
    let mut out = Vec::with_capacity(uri.len());
    let mut rest = uri.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        let escaped = (byte == b'%')
            .then(|| tail.split_first_chunk::<2>())
            .flatten()
            .and_then(|(hex, after)| {
                let hex = std::str::from_utf8(hex).ok()?;
                Some((u8::from_str_radix(hex, 16).ok()?, after))
            });
        let (decoded, after) = escaped.unwrap_or((byte, tail));
        out.push(decoded);
        rest = after;
    }
    String::from_utf8_lossy(&out).into_owned()
}
