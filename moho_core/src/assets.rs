//! Shipped asset files, resolved one file at a time through folders the game names.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Why an asset folder could not be added or an asset could not be resolved.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AssetError {
    /// An asset folder, placeholders or named, is absent.
    #[error("asset folder {} does not exist", path.display())]
    MissingFolder {
        /// The folder that was asked for.
        path: PathBuf,
    },
    /// An asset name is absolute or steps outside its folder.
    #[error("asset name {} must be a relative path without `.` or `..`", name.display())]
    InvalidName {
        /// The refused name.
        name: PathBuf,
    },
    /// No folder holds the asset.
    #[error(
        "asset {} not found; searched: {}",
        name.display(),
        searched.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    )]
    NotFound {
        /// The asset that was asked for.
        name: PathBuf,
        /// Every folder looked in, highest layer first.
        searched: Vec<PathBuf>,
    },
}

/// Asset folders in search order. The first layer holding a file supplies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRoots {
    /// Folders from the placeholders (bottom) upward.
    layers: Vec<PathBuf>,
}

impl AssetRoots {
    /// The bottom layer.
    ///
    /// # Errors
    /// [`AssetError::MissingFolder`] when `path` is not a folder.
    pub fn placeholders(path: &Path) -> Result<Self, AssetError> {
        Self { layers: Vec::new() }.with_folder(path)
    }

    /// Adds `path` as a layer above every existing one.
    ///
    /// # Errors
    /// [`AssetError::MissingFolder`] when `path` is not a folder.
    pub fn with_folder(mut self, path: &Path) -> Result<Self, AssetError> {
        if !path.is_dir() {
            return Err(AssetError::MissingFolder {
                path: path.to_owned(),
            });
        }
        self.layers.push(path.to_owned());
        Ok(self)
    }

    /// The file `name` in the highest layer that holds it, logging which layer supplied it.
    ///
    /// # Errors
    /// [`AssetError::InvalidName`] for an absolute name or one with a `..`, `.`, root or prefix
    /// component; [`AssetError::NotFound`] listing every folder searched.
    pub fn resolve(&self, name: &Path) -> Result<PathBuf, AssetError> {
        let plain = name.components().next().is_some()
            && name
                .components()
                .all(|part| matches!(part, Component::Normal(_)));
        if !plain {
            return Err(AssetError::InvalidName {
                name: name.to_owned(),
            });
        }
        for (index, folder) in self.layers.iter().enumerate().rev() {
            let file = folder.join(name);
            if file.is_file() {
                let layer = if index == 0 {
                    "placeholders"
                } else {
                    "assets folder"
                };
                tracing::info!(
                    asset = %name.display(),
                    layer,
                    folder = %folder.display(),
                    "asset resolved"
                );
                return Ok(file);
            }
        }
        Err(AssetError::NotFound {
            name: name.to_owned(),
            searched: self.layers.iter().rev().cloned().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// A folder holding an empty file for each of `files` (parent folders created).
    fn folder_with(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, file).unwrap();
        }
        dir
    }

    fn roots(placeholders: &Path, folders: &[&Path]) -> AssetRoots {
        let mut roots = AssetRoots::placeholders(placeholders).expect("placeholders exist");
        for folder in folders {
            roots = roots.with_folder(folder).expect("folder exists");
        }
        roots
    }

    #[test]
    fn folder_file_wins_over_placeholder() {
        let placeholders = folder_with(&["arena.glb"]);
        let folder = folder_with(&["arena.glb"]);
        let roots = roots(placeholders.path(), &[folder.path()]);

        let path = roots.resolve(Path::new("arena.glb")).unwrap();

        assert_eq!(path, folder.path().join("arena.glb"));
    }

    #[test]
    fn file_the_folder_lacks_comes_from_placeholders() {
        let placeholders = folder_with(&["arena.glb", "rifle_shot.wav"]);
        let folder = folder_with(&["arena.glb"]);
        let roots = roots(placeholders.path(), &[folder.path()]);

        let path = roots.resolve(Path::new("rifle_shot.wav")).unwrap();

        assert_eq!(path, placeholders.path().join("rifle_shot.wav"));
    }

    #[test]
    fn missing_folder_is_refused_naming_it() {
        let placeholders = folder_with(&[]);
        let parent = tempfile::tempdir().unwrap();
        let missing = parent.path().join("gone");
        let base = AssetRoots::placeholders(placeholders.path()).unwrap();

        let error = base.with_folder(&missing).unwrap_err();

        assert!(
            matches!(&error, AssetError::MissingFolder { path } if path == &missing),
            "{error:?}"
        );
        assert!(
            error.to_string().contains(&missing.display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn a_file_where_a_folder_is_expected_is_refused() {
        let placeholders = folder_with(&[]);
        let holder = folder_with(&["plain.txt"]);
        let file = holder.path().join("plain.txt");
        let base = AssetRoots::placeholders(placeholders.path()).unwrap();

        let as_placeholders = AssetRoots::placeholders(&file).unwrap_err();
        let as_folder = base.with_folder(&file).unwrap_err();

        for error in [as_placeholders, as_folder] {
            assert!(
                matches!(&error, AssetError::MissingFolder { path } if path == &file),
                "{error:?}"
            );
        }
    }

    /// Collects what `tracing` writes while `run` executes.
    fn logged(run: impl FnOnce()) -> String {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("log sink").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || Sink(Arc::clone(&writer)))
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, run);
        let bytes = buffer.lock().expect("log sink").clone();
        String::from_utf8(bytes).expect("log output is utf-8")
    }

    #[test]
    fn resolve_logs_one_info_line_naming_asset_layer_and_folder() {
        let placeholders = folder_with(&["rifle_shot.wav"]);
        let folder = folder_with(&["arena.glb"]);
        let roots = roots(placeholders.path(), &[folder.path()]);

        let from_folder = logged(|| {
            roots.resolve(Path::new("arena.glb")).unwrap();
        });
        let from_placeholders = logged(|| {
            roots.resolve(Path::new("rifle_shot.wav")).unwrap();
        });

        assert_eq!(from_folder.lines().count(), 1, "{from_folder}");
        assert!(from_folder.contains("INFO"), "{from_folder}");
        assert!(from_folder.contains("arena.glb"), "{from_folder}");
        assert!(from_folder.contains("assets folder"), "{from_folder}");
        assert!(!from_folder.contains("placeholders"), "{from_folder}");
        assert!(
            from_folder.contains(&folder.path().display().to_string()),
            "{from_folder}"
        );
        assert_eq!(from_placeholders.lines().count(), 1, "{from_placeholders}");
        assert!(from_placeholders.contains("INFO"), "{from_placeholders}");
        assert!(
            from_placeholders.contains("rifle_shot.wav"),
            "{from_placeholders}"
        );
        assert!(
            from_placeholders.contains("placeholders"),
            "{from_placeholders}"
        );
        assert!(
            !from_placeholders.contains("assets folder"),
            "{from_placeholders}"
        );
        assert!(
            from_placeholders.contains(&placeholders.path().display().to_string()),
            "{from_placeholders}"
        );
    }

    #[test]
    fn missing_placeholders_are_refused_naming_them() {
        let parent = tempfile::tempdir().unwrap();
        let missing = parent.path().join("placeholders");

        let error = AssetRoots::placeholders(&missing).unwrap_err();

        assert!(
            matches!(&error, AssetError::MissingFolder { path } if path == &missing),
            "{error:?}"
        );
        assert!(
            error.to_string().contains(&missing.display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn unknown_file_names_itself_and_every_folder_in_search_order() {
        let placeholders = folder_with(&[]);
        let first = folder_with(&[]);
        let second = folder_with(&[]);
        let roots = roots(placeholders.path(), &[first.path(), second.path()]);

        let error = roots.resolve(Path::new("nope.glb")).unwrap_err();

        let AssetError::NotFound { name, searched } = &error else {
            panic!("expected NotFound, got {error:?}");
        };
        let expected = [
            second.path().to_owned(),
            first.path().to_owned(),
            placeholders.path().to_owned(),
        ];
        assert_eq!(name, Path::new("nope.glb"));
        assert_eq!(searched, &expected);
        let text = error.to_string();
        assert!(text.contains("nope.glb"), "{text}");
        let at: Vec<usize> = expected
            .iter()
            .map(|folder| {
                text.find(&folder.display().to_string())
                    .unwrap_or_else(|| panic!("{} missing from {text}", folder.display()))
            })
            .collect();
        assert!(at.is_sorted(), "{text}");
    }

    #[test]
    fn absolute_or_parent_names_are_refused() {
        let outer = folder_with(&["x.glb"]);
        let layer = outer.path().join("layer");
        fs::create_dir_all(layer.join("a")).unwrap();
        fs::write(layer.join("x.glb"), "x").unwrap();
        let absolute = outer.path().join("x.glb");
        let roots = roots(&layer, &[]);

        for name in [
            absolute.as_path(),
            Path::new("/x.glb"),
            Path::new("../x.glb"),
            Path::new("a/../x.glb"),
            Path::new("./x.glb"),
        ] {
            let error = roots.resolve(name).unwrap_err();

            assert!(
                matches!(&error, AssetError::InvalidName { name: refused } if refused == name),
                "{name:?}: {error:?}"
            );
            assert!(
                error.to_string().contains(&name.display().to_string()),
                "{error}"
            );
        }
    }

    #[test]
    fn nested_name_resolves_inside_its_layer() {
        let placeholders = folder_with(&["models/rifle.glb"]);
        let folder = folder_with(&["models/other.glb"]);
        let roots = roots(placeholders.path(), &[folder.path()]);

        let path = roots.resolve(Path::new("models/rifle.glb")).unwrap();

        assert_eq!(path, placeholders.path().join("models/rifle.glb"));
    }

    #[test]
    fn directory_with_the_name_is_not_a_match() {
        let placeholders = folder_with(&["arena.glb"]);
        let folder = tempfile::tempdir().unwrap();
        fs::create_dir(folder.path().join("arena.glb")).unwrap();
        let roots = roots(placeholders.path(), &[folder.path()]);

        let path = roots.resolve(Path::new("arena.glb")).unwrap();

        assert_eq!(path, placeholders.path().join("arena.glb"));
    }

    #[test]
    fn later_folder_outranks_earlier() {
        let placeholders = folder_with(&["arena.glb"]);
        let earlier = folder_with(&["arena.glb"]);
        let later = folder_with(&["arena.glb"]);
        let roots = roots(placeholders.path(), &[earlier.path(), later.path()]);

        let path = roots.resolve(Path::new("arena.glb")).unwrap();

        assert_eq!(path, later.path().join("arena.glb"));
    }

    proptest::proptest! {
        #[test]
        fn resolve_returns_the_highest_layer_holding_the_name(
            present in proptest::collection::vec(proptest::bool::ANY, 1..=4),
        ) {
            let dirs: Vec<tempfile::TempDir> = present
                .iter()
                .map(|has| folder_with(if *has { &["thing.bin"] } else { &[] }))
                .collect();
            let folders: Vec<&Path> = dirs[1..].iter().map(tempfile::TempDir::path).collect();
            let roots = roots(dirs[0].path(), &folders);

            let result = roots.resolve(Path::new("thing.bin"));

            if let Some(top) = present.iter().rposition(|has| *has) {
                proptest::prop_assert_eq!(
                    result.unwrap(),
                    dirs[top].path().join("thing.bin")
                );
            } else {
                let searched: Vec<PathBuf> =
                    dirs.iter().rev().map(|d| d.path().to_owned()).collect();
                let is_listing = matches!(
                    &result,
                    Err(AssetError::NotFound { searched: s, .. }) if s == &searched
                );
                proptest::prop_assert!(is_listing, "{result:?}");
            }
        }
    }
}
