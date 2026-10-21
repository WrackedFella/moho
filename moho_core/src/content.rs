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
    /// A mod name is not a plain folder name.
    #[error("mod name {name:?} must be non-empty ASCII letters, digits, '_' or '-'")]
    InvalidRootName {
        /// The refused name.
        name: String,
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

    /// Appends `mods_dir/<name>` per name, in order, after the base.
    ///
    /// # Errors
    ///
    /// [`ContentError::InvalidRootName`] for a name that is not a plain folder name;
    /// [`ContentError::MissingRoot`] for a folder that does not exist.
    pub fn with_mods(self, mods_dir: &Path, names: &[String]) -> Result<Self, ContentError> {
        let mut roots = self.roots;
        for name in names {
            if !is_plain_folder_name(name) {
                return Err(ContentError::InvalidRootName { name: name.clone() });
            }
            let path = mods_dir.join(name);
            if !path.is_dir() {
                return Err(ContentError::MissingRoot { path });
            }
            roots.push(ContentRoot {
                name: name.clone(),
                path,
            });
        }
        Ok(Self { roots })
    }
}

/// Non-empty ASCII letters, digits, `_` and `-`, so a name cannot leave `mods_dir`.
fn is_plain_folder_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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

    fn write_mod(mods: &Path, name: &str, weapons: &str) {
        let folder = mods.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("weapons.toml"), weapons).unwrap();
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    fn thing(count: u32, name: &str) -> String {
        format!("[thing]\ncount = {count}\nname = \"{name}\"\n")
    }

    #[test]
    fn the_last_listed_root_wins() {
        let (_base, roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();
        write_mod(mods.path(), "a", &thing(10, "a"));
        write_mod(mods.path(), "b", &thing(50, "b"));
        let base_path = roots.roots[0].path.clone();

        let ab = roots.with_mods(mods.path(), &names(&["a", "b"])).unwrap();
        let ba = ContentRoots::base(&base_path)
            .unwrap()
            .with_mods(mods.path(), &names(&["b", "a"]))
            .unwrap();

        let ab = load_kind::<Thing>(&ab, "weapons").unwrap();
        let ba = load_kind::<Thing>(&ba, "weapons").unwrap();
        assert_eq!(ab.require("thing").unwrap().count, 50);
        assert_eq!(ba.require("thing").unwrap().count, 10);
    }

    #[test]
    fn an_override_replaces_every_field() {
        let (_base, roots) = base_with("[thing]\ncount = 30\nname = \"base\"\n");
        let mods = tempfile::tempdir().unwrap();
        write_mod(mods.path(), "light", &thing(20, "light"));

        let roots = roots.with_mods(mods.path(), &names(&["light"])).unwrap();

        let definitions = load_kind::<Thing>(&roots, "weapons").unwrap();
        assert_eq!(
            definitions.require("thing").unwrap(),
            &Thing {
                count: 20,
                name: "light".into()
            }
        );
    }

    #[test]
    fn a_mod_folder_not_listed_is_not_read() {
        let (_base, roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();
        write_mod(mods.path(), "heavy", &thing(40, "heavy"));

        let roots = roots.with_mods(mods.path(), &[]).unwrap();

        let definitions = load_kind::<Thing>(&roots, "weapons").unwrap();
        assert_eq!(definitions.require("thing").unwrap().count, 30);
        assert_eq!(definitions.require("thing").unwrap().name, "base");
    }

    #[test]
    fn a_broken_mod_entry_names_the_mods_file_and_id() {
        let (_base, roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();
        write_mod(mods.path(), "heavy", "[thing]\nname = \"heavy\"\n");
        let roots = roots.with_mods(mods.path(), &names(&["heavy"])).unwrap();

        let error = load_kind::<Thing>(&roots, "weapons").unwrap_err();

        let ContentError::Invalid { file, id, reason } = &error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert_eq!(file, &mods.path().join("heavy").join("weapons.toml"));
        assert_eq!(id, "thing");
        assert!(reason.contains("count"), "{reason}");
    }

    #[test]
    fn a_listed_mod_that_does_not_exist_is_refused_naming_it() {
        let (_base, roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();

        let error = roots.with_mods(mods.path(), &names(&["gone"])).unwrap_err();

        let ContentError::MissingRoot { path } = &error else {
            panic!("expected MissingRoot, got {error:?}");
        };
        assert_eq!(path, &mods.path().join("gone"));
        assert!(error.to_string().contains("gone"), "{error}");
    }

    #[test]
    fn a_listed_mod_that_is_a_file_is_refused_naming_it() {
        let (_base, roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();
        std::fs::write(mods.path().join("heavy"), "").unwrap();

        let error = roots
            .with_mods(mods.path(), &names(&["heavy"]))
            .unwrap_err();

        let ContentError::MissingRoot { path } = &error else {
            panic!("expected MissingRoot, got {error:?}");
        };
        assert_eq!(path, &mods.path().join("heavy"));
    }

    #[test]
    fn a_mod_without_the_kind_file_keeps_the_base_entry() {
        let (_base, roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();
        std::fs::create_dir(mods.path().join("cosmetic")).unwrap();

        let roots = roots.with_mods(mods.path(), &names(&["cosmetic"])).unwrap();
        let definitions = load_kind::<Thing>(&roots, "weapons").unwrap();

        assert_eq!(definitions.require("thing").unwrap().count, 30);
    }

    #[test]
    fn a_mod_with_bad_toml_names_the_mods_file() {
        let (_base, roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();
        write_mod(mods.path(), "heavy", "[thing\ncount = 40\n");

        let roots = roots.with_mods(mods.path(), &names(&["heavy"])).unwrap();
        let error = load_kind::<Thing>(&roots, "weapons").unwrap_err();

        let ContentError::Syntax { file, .. } = &error else {
            panic!("expected Syntax, got {error:?}");
        };
        assert_eq!(file, &mods.path().join("heavy").join("weapons.toml"));
    }

    #[test]
    fn a_mod_adds_new_ids_beside_base_ones() {
        let (_base, roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();
        write_mod(
            mods.path(),
            "extra",
            "[other]\ncount = 7\nname = \"extra\"\n",
        );

        let roots = roots.with_mods(mods.path(), &names(&["extra"])).unwrap();

        let definitions = load_kind::<Thing>(&roots, "weapons").unwrap();
        assert_eq!(definitions.require("thing").unwrap().count, 30);
        assert_eq!(
            definitions.require("other").unwrap(),
            &Thing {
                count: 7,
                name: "extra".into()
            }
        );
    }

    proptest::proptest! {
        #[test]
        fn mod_names_must_be_plain_folder_names(name in "[A-Za-z0-9_./\\\\ éßЖ-]{0,8}") {
            let (_base, roots) = base_with(&thing(30, "base"));
            let mods = tempfile::tempdir().unwrap();
            let plain = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            if plain {
                std::fs::create_dir_all(mods.path().join(&name)).unwrap();
            }

            let result = roots.with_mods(mods.path(), std::slice::from_ref(&name));

            if plain {
                proptest::prop_assert!(result.is_ok(), "{name:?}: {result:?}");
            } else {
                let is_invalid = matches!(
                    &result,
                    Err(ContentError::InvalidRootName { name: refused }) if refused == &name
                );
                proptest::prop_assert!(is_invalid, "{name:?}: {result:?}");
            }
        }
    }

    #[test]
    fn the_spec_named_mod_names_are_refused_or_accepted() {
        let (base, _roots) = base_with(&thing(30, "base"));
        let mods = tempfile::tempdir().unwrap();
        for accepted in ["heavy", "my-mod_2"] {
            std::fs::create_dir_all(mods.path().join(accepted)).unwrap();
            let result = ContentRoots::base(base.path())
                .unwrap()
                .with_mods(mods.path(), &[accepted.to_owned()]);
            assert!(result.is_ok(), "{accepted:?}: {result:?}");
        }
        for refused in ["", ".", "..", "../x", "a/b", "a\\b", " ", "\u{e9}"] {
            let result = ContentRoots::base(base.path())
                .unwrap()
                .with_mods(mods.path(), &[refused.to_owned()]);
            assert!(
                matches!(&result, Err(ContentError::InvalidRootName { name }) if name == refused),
                "{refused:?}: {result:?}"
            );
        }
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
