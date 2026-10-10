//! Level load errors.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// A level failed to load; always names the level file.
#[derive(Debug)]
pub struct LevelError {
    path: PathBuf,
    kind: LevelErrorKind,
}

impl LevelError {
    #[expect(dead_code, reason = "used once load is implemented")]
    pub(crate) fn new(path: &Path, kind: LevelErrorKind) -> Self {
        Self {
            path: path.to_path_buf(),
            kind,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> &LevelErrorKind {
        &self.kind
    }
}

impl fmt::Display for LevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.kind)
    }
}

impl Error for LevelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.kind.source()
    }
}

/// Why a level failed to load.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LevelErrorKind {
    #[error("cannot read file")]
    Read(#[source] std::io::Error),
    /// The parser's error is boxed so no `gltf` type is part of this crate's API.
    #[error("not a glTF file")]
    NotGltf(#[source] Box<dyn Error + Send + Sync>),
    #[error("buffer file `{uri}` cannot be read")]
    MissingBuffer {
        uri: String,
        #[source]
        source: std::io::Error,
    },
    #[error("buffer uri `{uri}` is not a relative file path")]
    UnsupportedBufferUri { uri: String },
    #[error("mesh {mesh}: {reason}")]
    BadMesh { mesh: usize, reason: String },
}
