//! Buffer resolution: GLB blob or relative file URIs.

use std::borrow::Cow;
use std::path::Path;

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
            gltf::buffer::Source::Bin => blob
                .map(Cow::Borrowed)
                .ok_or_else(|| LevelErrorKind::UnsupportedBufferUri { uri: String::new() }),
            gltf::buffer::Source::Uri(uri) => {
                if is_absolute_or_scheme(uri) {
                    return Err(LevelErrorKind::UnsupportedBufferUri {
                        uri: uri.to_owned(),
                    });
                }
                std::fs::read(dir.join(percent_decode(uri)))
                    .map(Cow::Owned)
                    .map_err(|source| LevelErrorKind::MissingBuffer {
                        uri: uri.to_owned(),
                        source,
                    })
            }
        })
        .collect()
}

fn is_absolute_or_scheme(uri: &str) -> bool {
    uri.starts_with('/')
        || uri
            .split('/')
            .next()
            .is_some_and(|first| first.contains(':'))
}

fn percent_decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%')
            .then(|| bytes.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        if let Some(byte) = hex {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
