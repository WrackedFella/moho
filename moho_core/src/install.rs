//! Finds the folder that holds a game's shipped folders: beside the executable for a packaged
//! build, the source checkout for a development run.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// Where an [`Install`]'s root was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The executable's own folder holds the shipped folders.
    BesideExecutable,
    /// Nothing is beside the executable; the source checkout holds them.
    SourceCheckout,
}

/// Why no folder holds the shipped folders.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InstallError {
    /// The executable's folder holds some shipped folders but not all.
    #[error(
        "install beside the executable at {} is missing: {}",
        root.display(),
        missing.join(", ")
    )]
    Incomplete {
        /// The executable's folder.
        root: PathBuf,
        /// The shipped folders it lacks.
        missing: Vec<String>,
    },
    /// Neither the executable's folder nor the checkout holds the shipped folders.
    #[error(
        "no install found; looked for {}",
        searched.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    )]
    NotFound {
        /// Every folder looked for, beside the executable and in the checkout.
        searched: Vec<PathBuf>,
    },
}

/// The folder that holds every shipped folder of a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    root: PathBuf,
    origin: Origin,
}

impl Install {
    /// Finds the install: `exe_dir` when it holds any of `folders` (then it must hold all of
    /// them), otherwise `checkout` (which must hold all of them).
    ///
    /// # Errors
    /// [`InstallError::Incomplete`] for a partial install beside the executable;
    /// [`InstallError::NotFound`] when neither place holds every folder.
    pub fn locate(exe_dir: &Path, checkout: &Path, folders: &[&str]) -> Result<Self, InstallError> {
        let _ = (exe_dir, checkout, folders);
        todo!("locate the install")
    }

    /// The folder holding the shipped folders.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the root was found.
    pub fn origin(&self) -> Origin {
        self.origin
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const FOLDERS: &[&str] = &["assets", "content"];

    fn with_folders(names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            fs::create_dir(dir.path().join(name)).unwrap();
        }
        dir
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
    fn checkout_is_used_when_nothing_is_beside_the_executable() {
        let exe = with_folders(&[]);
        let checkout = with_folders(FOLDERS);

        let install = Install::locate(exe.path(), checkout.path(), FOLDERS).unwrap();

        assert_eq!(install.root(), checkout.path());
        assert_eq!(install.origin(), Origin::SourceCheckout);
    }

    #[test]
    fn beside_the_executable_wins_when_it_holds_every_folder() {
        let exe = with_folders(FOLDERS);
        let checkout = with_folders(FOLDERS);

        let install = Install::locate(exe.path(), checkout.path(), FOLDERS).unwrap();

        assert_eq!(install.root(), exe.path());
        assert_eq!(install.origin(), Origin::BesideExecutable);
    }

    #[test]
    fn an_incomplete_install_is_refused_naming_what_is_missing() {
        let exe = with_folders(&["assets"]);
        let checkout = with_folders(FOLDERS);

        let error = Install::locate(exe.path(), checkout.path(), FOLDERS).unwrap_err();

        assert!(
            matches!(
                &error,
                InstallError::Incomplete { root, missing }
                    if root == exe.path() && missing == &["content"]
            ),
            "{error:?}"
        );
        let text = error.to_string();
        assert!(text.contains("content"), "{text}");
        assert!(text.contains(&exe.path().display().to_string()), "{text}");
    }

    #[test]
    fn a_file_with_a_folders_name_makes_the_install_incomplete() {
        let exe = with_folders(&["assets"]);
        fs::write(exe.path().join("content"), "not a folder").unwrap();
        let checkout = with_folders(FOLDERS);

        let error = Install::locate(exe.path(), checkout.path(), FOLDERS).unwrap_err();

        assert!(
            matches!(
                &error,
                InstallError::Incomplete { root, missing }
                    if root == exe.path() && missing == &["content"]
            ),
            "{error:?}"
        );
    }

    #[test]
    fn neither_place_is_an_error_naming_every_folder_searched() {
        let exe = with_folders(&[]);
        let empty_checkout = with_folders(&[]);
        let partial_checkout = with_folders(&["assets"]);

        for checkout in [&empty_checkout, &partial_checkout] {
            let error = Install::locate(exe.path(), checkout.path(), FOLDERS).unwrap_err();

            let InstallError::NotFound { searched } = &error else {
                panic!("expected NotFound, got {error:?}");
            };
            let expected: Vec<PathBuf> = [exe.path(), checkout.path()]
                .iter()
                .flat_map(|place| FOLDERS.iter().map(|folder| place.join(folder)))
                .collect();
            assert_eq!(searched.len(), expected.len(), "{searched:?}");
            let text = error.to_string();
            for path in &expected {
                assert!(searched.contains(path), "{path:?} not in {searched:?}");
                assert!(text.contains(&path.display().to_string()), "{text}");
            }
        }
    }

    #[test]
    fn locating_logs_the_origin_and_root() {
        let exe = with_folders(FOLDERS);
        let empty = with_folders(&[]);
        let checkout = with_folders(FOLDERS);

        let beside = logged(|| {
            Install::locate(exe.path(), checkout.path(), FOLDERS).unwrap();
        });
        let source = logged(|| {
            Install::locate(empty.path(), checkout.path(), FOLDERS).unwrap();
        });

        assert_eq!(beside.lines().count(), 1, "{beside}");
        assert!(beside.contains("INFO"), "{beside}");
        assert!(beside.contains("beside the executable"), "{beside}");
        assert!(!beside.contains("source checkout"), "{beside}");
        assert!(
            beside.contains(&exe.path().display().to_string()),
            "{beside}"
        );
        assert_eq!(source.lines().count(), 1, "{source}");
        assert!(source.contains("INFO"), "{source}");
        assert!(source.contains("source checkout"), "{source}");
        assert!(!source.contains("beside the executable"), "{source}");
        assert!(
            source.contains(&checkout.path().display().to_string()),
            "{source}"
        );
    }

    proptest::proptest! {
        #[test]
        fn locate_never_mixes_roots(
            exe_has in proptest::collection::vec(proptest::bool::ANY, 2),
            checkout_has in proptest::collection::vec(proptest::bool::ANY, 2),
        ) {
            let present = |has: &[bool]| -> Vec<&str> {
                FOLDERS.iter().zip(has).filter(|(_, h)| **h).map(|(f, _)| *f).collect()
            };
            let exe = with_folders(&present(&exe_has));
            let checkout = with_folders(&present(&checkout_has));

            let result = Install::locate(exe.path(), checkout.path(), FOLDERS);

            let exe_any = exe_has.iter().any(|h| *h);
            let exe_all = exe_has.iter().all(|h| *h);
            let checkout_all = checkout_has.iter().all(|h| *h);
            if exe_any && exe_all {
                let install = result.unwrap();
                proptest::prop_assert_eq!(install.root(), exe.path());
                proptest::prop_assert_eq!(install.origin(), Origin::BesideExecutable);
            } else if exe_any {
                let is_incomplete = matches!(
                    &result,
                    Err(InstallError::Incomplete { root, .. }) if root == exe.path()
                );
                proptest::prop_assert!(is_incomplete, "{result:?}");
            } else if checkout_all {
                let install = result.unwrap();
                proptest::prop_assert_eq!(install.root(), checkout.path());
                proptest::prop_assert_eq!(install.origin(), Origin::SourceCheckout);
            } else {
                let is_not_found = matches!(&result, Err(InstallError::NotFound { .. }));
                proptest::prop_assert!(is_not_found, "{result:?}");
            }
        }
    }
}
