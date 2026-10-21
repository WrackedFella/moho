//! Definition files: one TOML file per kind in each content root, a table per id.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use thiserror::Error;

/// Why content could not be loaded or looked up.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ContentError {
    /// The base content folder is absent.
    #[error("content folder {} does not exist", path.display())]
    MissingRoot {
        /// The folder that was asked for.
        path: PathBuf,
    },
    /// A definitions file exists but could not be read.
    #[error("cannot read {}: {reason}", file.display())]
    Unreadable {
        /// The file that failed to read.
        file: PathBuf,
        /// What the OS reported.
        reason: String,
    },
    /// A definitions file is not valid TOML.
    #[error("{}: {reason}", file.display())]
    Syntax {
        /// The file that failed to parse.
        file: PathBuf,
        /// The parser's message, carrying line and column.
        reason: String,
    },
    /// An entry does not fit its type.
    #[error("{}: \"{id}\": {reason}", file.display())]
    Invalid {
        /// The file holding the entry.
        file: PathBuf,
        /// The entry's table name.
        id: String,
        /// What is wrong with it, trimmed.
        reason: String,
    },
    /// No root defines the requested id.
    #[error("no content root defines \"{id}\" in {kind}.toml")]
    MissingId {
        /// The definition kind searched.
        kind: String,
        /// The id that was not found.
        id: String,
    },
}

#[derive(Debug)]
struct ContentRoot {
    name: String,
    path: PathBuf,
}

/// Content folders in override order, base first.
#[derive(Debug)]
pub struct ContentRoots {
    roots: Vec<ContentRoot>,
}

impl ContentRoots {
    /// Roots holding only the base folder.
    ///
    /// # Errors
    ///
    /// [`ContentError::MissingRoot`] when `path` is not a folder.
    pub fn base(path: &Path) -> Result<Self, ContentError> {
        if !path.is_dir() {
            return Err(ContentError::MissingRoot {
                path: path.to_path_buf(),
            });
        }
        Ok(Self {
            roots: vec![ContentRoot {
                name: "base".to_owned(),
                path: path.to_path_buf(),
            }],
        })
    }
}

/// Typed definitions of one kind, keyed by id.
#[derive(Debug)]
pub struct Definitions<T> {
    entries: BTreeMap<String, T>,
    kind: String,
}

impl<T> Definitions<T> {
    /// The definition under `id`, if any root defines it.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&T> {
        self.entries.get(id)
    }

    /// The definition under `id`.
    ///
    /// # Errors
    ///
    /// [`ContentError::MissingId`] when no root defines it.
    pub fn require(&self, id: &str) -> Result<&T, ContentError> {
        self.get(id).ok_or_else(|| ContentError::MissingId {
            kind: self.kind.clone(),
            id: id.to_owned(),
        })
    }
}

/// Collapses whitespace runs, newlines included, so a message fits one line.
fn one_line(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Reads and parses `file`; `None` when it does not exist.
fn read_table(file: &Path) -> Result<Option<toml::Table>, ContentError> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ContentError::Unreadable {
                file: file.to_path_buf(),
                reason: error.to_string(),
            });
        }
    };
    text.parse::<toml::Table>()
        .map(Some)
        .map_err(|error| ContentError::Syntax {
            file: file.to_path_buf(),
            reason: one_line(&error.to_string()),
        })
}

/// Loads `<root>/<kind>.toml` from every root and types each table as `T`.
///
/// # Errors
///
/// A [`ContentError`] naming the file, and the id where one applies.
pub fn load_kind<T: DeserializeOwned>(
    roots: &ContentRoots,
    kind: &str,
) -> Result<Definitions<T>, ContentError> {
    let mut entries = BTreeMap::new();
    for root in &roots.roots {
        let file = root.path.join(format!("{kind}.toml"));
        let Some(table) = read_table(&file)? else {
            continue;
        };
        tracing::debug!(root = %root.name, kind, entries = table.len(), "loaded definitions");
        for (id, value) in table {
            let typed = value
                .try_into::<T>()
                .map_err(|error| ContentError::Invalid {
                    file: file.clone(),
                    id: id.clone(),
                    reason: one_line(&error.to_string()),
                })?;
            entries.insert(id, typed);
        }
    }
    Ok(Definitions {
        entries,
        kind: kind.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Thing {
        count: u32,
        name: String,
    }

    fn base_with(weapons: &str) -> (tempfile::TempDir, ContentRoots) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("weapons.toml"), weapons).unwrap();
        let roots = ContentRoots::base(dir.path()).unwrap();
        (dir, roots)
    }

    #[test]
    fn an_entry_missing_a_field_is_refused_naming_file_and_id() {
        let (dir, roots) = base_with("[thing]\nname = \"x\"\n");

        let error = load_kind::<Thing>(&roots, "weapons").unwrap_err();

        let ContentError::Invalid { file, id, reason } = &error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert_eq!(file, &dir.path().join("weapons.toml"));
        assert_eq!(id, "thing");
        assert!(reason.contains("count"), "{reason}");
        assert!(!reason.ends_with('\n'), "{reason:?}");
        assert!(!reason.contains('\n'), "{reason:?}");
    }

    #[test]
    fn an_entry_with_a_wrong_type_is_refused_naming_file_and_id() {
        let (dir, roots) = base_with("[thing]\ncount = \"lots\"\nname = \"x\"\n");

        let error = load_kind::<Thing>(&roots, "weapons").unwrap_err();

        let ContentError::Invalid { file, id, reason } = &error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert_eq!(file, &dir.path().join("weapons.toml"));
        assert_eq!(id, "thing");
        assert!(reason.contains("lots"), "{reason}");
        assert!(reason.contains("count"), "{reason}");
        assert!(!reason.contains('\n'), "{reason:?}");
    }

    #[test]
    fn a_missing_base_folder_is_refused_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");

        let error = ContentRoots::base(&missing).unwrap_err();

        let ContentError::MissingRoot { path } = &error else {
            panic!("expected MissingRoot, got {error:?}");
        };
        assert_eq!(path, &missing);
        assert!(error.to_string().contains(&missing.display().to_string()));
    }

    #[test]
    fn a_kind_no_root_defines_is_missing_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let roots = ContentRoots::base(dir.path()).unwrap();

        let definitions = load_kind::<Thing>(&roots, "weapons")
            .expect("a root without the kind file contributes nothing");

        assert!(definitions.get("thing").is_none());
        let error = definitions.require("thing").unwrap_err();
        let ContentError::MissingId { kind, id } = &error else {
            panic!("expected MissingId, got {error:?}");
        };
        assert_eq!(kind, "weapons");
        assert_eq!(id, "thing");
        let text = error.to_string();
        assert!(
            text.contains("thing") && text.contains("weapons.toml"),
            "{text}"
        );
    }

    #[test]
    fn a_file_that_is_not_toml_is_refused_naming_it() {
        let (dir, roots) = base_with("[thing\ncount = = 3\n");

        let error = load_kind::<Thing>(&roots, "weapons").unwrap_err();

        let ContentError::Syntax { file, reason } = &error else {
            panic!("expected Syntax, got {error:?}");
        };
        assert_eq!(file, &dir.path().join("weapons.toml"));
        assert!(reason.contains("line"), "{reason}");
    }

    #[test]
    fn a_kind_file_that_cannot_be_read_is_refused_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("weapons.toml")).unwrap();
        let roots = ContentRoots::base(dir.path()).unwrap();

        let error = load_kind::<Thing>(&roots, "weapons").unwrap_err();

        let ContentError::Unreadable { file, .. } = &error else {
            panic!("expected Unreadable, got {error:?}");
        };
        assert_eq!(file, &dir.path().join("weapons.toml"));
    }

    #[test]
    fn every_entry_loads_under_its_table_name() {
        let (_dir, roots) =
            base_with("[alpha]\ncount = 1\nname = \"a\"\n\n[beta]\ncount = 2\nname = \"b\"\n");

        let definitions = load_kind::<Thing>(&roots, "weapons").unwrap();

        assert_eq!(
            definitions.get("alpha"),
            Some(&Thing {
                count: 1,
                name: "a".into()
            })
        );
        assert_eq!(
            definitions.require("beta").unwrap(),
            &Thing {
                count: 2,
                name: "b".into()
            }
        );
        assert!(definitions.get("gamma").is_none());
    }

    #[test]
    fn errors_display_file_and_id() {
        let file = PathBuf::from("content/weapons.toml");
        let table = [
            (
                ContentError::MissingRoot {
                    path: PathBuf::from("mods/xyz"),
                },
                vec!["mods/xyz", "does not exist"],
            ),
            (
                ContentError::Unreadable {
                    file: file.clone(),
                    reason: "denied".into(),
                },
                vec!["content/weapons.toml", "denied"],
            ),
            (
                ContentError::Syntax {
                    file: file.clone(),
                    reason: "line 2".into(),
                },
                vec!["content/weapons.toml", "line 2"],
            ),
            (
                ContentError::Invalid {
                    file: file.clone(),
                    id: "rifle".into(),
                    reason: "missing field `magazine_capacity`".into(),
                },
                vec![
                    "content/weapons.toml",
                    "\"rifle\"",
                    "missing field `magazine_capacity`",
                ],
            ),
            (
                ContentError::MissingId {
                    kind: "weapons".into(),
                    id: "rifle".into(),
                },
                vec!["\"rifle\"", "weapons.toml"],
            ),
        ];

        for (error, needles) in table {
            let text = error.to_string();
            for needle in needles {
                assert!(text.contains(needle), "{text:?} lacks {needle:?}");
            }
        }
    }
}
