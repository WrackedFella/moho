//! Reads preferences from INI text and files, reporting every fallback as data.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use super::{Prefs, WindowMode};

/// One problem found while loading preferences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefsIssue {
    /// A known key whose value is unusable. `value` is `None` when the line has no `=`.
    Malformed {
        section: String,
        key: String,
        value: Option<String>,
    },
    UnknownKey {
        section: String,
        key: String,
    },
    UnknownSection {
        section: String,
    },
    Unparseable {
        reason: String,
    },
    Unreadable {
        reason: String,
    },
    NotCreated {
        reason: String,
    },
}

/// A [`PrefsIssue`] together with the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefsWarning {
    pub file: PathBuf,
    pub issue: PrefsIssue,
}

impl fmt::Display for PrefsWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = self.file.display();
        match &self.issue {
            PrefsIssue::Malformed {
                section,
                key,
                value: Some(value),
            } => write!(f, "{file}: [{section}] {key}: unusable value {value:?}"),
            PrefsIssue::Malformed {
                section,
                key,
                value: None,
            } => write!(f, "{file}: [{section}] {key}: no '=' and no value"),
            PrefsIssue::UnknownKey { section, key } => {
                write!(f, "{file}: [{section}] has unknown key {key}")
            }
            PrefsIssue::UnknownSection { section } => {
                write!(f, "{file}: unknown section [{section}]")
            }
            PrefsIssue::Unparseable { reason } => write!(f, "{file}: cannot parse: {reason}"),
            PrefsIssue::Unreadable { reason } => write!(f, "{file}: cannot read: {reason}"),
            PrefsIssue::NotCreated { reason } => {
                write!(f, "{file}: cannot create default file: {reason}")
            }
        }
    }
}

/// Parses a raw value into the field; `None` leaves the field untouched.
type Setter = fn(&mut Prefs, &str) -> Option<()>;

fn set<T>(slot: &mut T, value: Option<T>) -> Option<()> {
    *slot = value?;
    Some(())
}

fn parse_f32(s: &str) -> Option<f32> {
    s.parse::<f32>().ok().filter(|v| v.is_finite())
}

fn parse_bool(s: &str) -> Option<bool> {
    if s.eq_ignore_ascii_case("true") || s == "1" {
        Some(true)
    } else if s.eq_ignore_ascii_case("false") || s == "0" {
        Some(false)
    } else {
        None
    }
}

fn parse_u32(s: &str) -> Option<u32> {
    s.parse().ok()
}

/// Every key `parse` accepts: `(section, key, setter)`.
const KNOWN_KEYS: &[(&str, &str, Setter)] = &[
    ("prefs", "mouse_sensitivity", |p, s| {
        set(&mut p.mouse_sensitivity, parse_f32(s))
    }),
    ("prefs", "input_filtering_enabled", |p, s| {
        set(&mut p.input_filtering_enabled, parse_bool(s))
    }),
    ("audio", "sound_effect_volume", |p, s| {
        set(&mut p.audio_sound_effect_volume, parse_f32(s))
    }),
    ("audio", "music_volume", |p, s| {
        set(&mut p.audio_music_volume, parse_f32(s))
    }),
    ("audio", "ui_volume", |p, s| {
        set(&mut p.audio_ui_volume, parse_f32(s))
    }),
    ("audio", "voice_volume", |p, s| {
        set(&mut p.audio_voice_volume, parse_f32(s))
    }),
    ("graphics", "shadow_quality", |p, s| {
        set(&mut p.graphics_shadow_quality, parse_u32(s))
    }),
    ("graphics", "ssao_quality", |p, s| {
        set(&mut p.graphics_ssao_quality, parse_u32(s))
    }),
    ("video", "window_mode", |p, s| {
        set(&mut p.window_mode, WindowMode::parse(s))
    }),
    ("video", "window_width", |p, s| {
        set(&mut p.window_resolution.0, parse_u32(s))
    }),
    ("video", "window_height", |p, s| {
        set(&mut p.window_resolution.1, parse_u32(s))
    }),
    ("world", "load_radius", |p, s| {
        set(&mut p.world_load_radius, parse_u32(s))
    }),
    ("world", "unload_radius", |p, s| {
        set(&mut p.world_unload_radius, parse_u32(s))
    }),
    ("world", "chunks_per_frame", |p, s| {
        set(
            &mut p.world_chunks_per_frame,
            parse_u32(s).filter(|&v| v > 0),
        )
    }),
];

type SectionKeys = HashMap<String, Option<String>>;

fn sorted_entries(keys: &SectionKeys) -> Vec<(&String, &Option<String>)> {
    let mut entries: Vec<_> = keys.iter().collect();
    entries.sort_by_key(|(key, _)| key.as_str());
    entries
}

/// Applies `keys` to `prefs` using the `table` section of `KNOWN_KEYS`; `section` is the
/// name reported in issues. Issues come back in sorted key order.
fn read_section(
    prefs: &mut Prefs,
    section: &str,
    table: &str,
    keys: &SectionKeys,
) -> Vec<PrefsIssue> {
    let mut issues = Vec::new();
    for (key, value) in sorted_entries(keys) {
        let Some((_, _, setter)) = KNOWN_KEYS.iter().find(|(s, k, _)| *s == table && *k == key)
        else {
            issues.push(PrefsIssue::UnknownKey {
                section: section.to_string(),
                key: key.clone(),
            });
            continue;
        };
        if value.as_deref().and_then(|v| setter(prefs, v)).is_none() {
            issues.push(PrefsIssue::Malformed {
                section: section.to_string(),
                key: key.clone(),
                value: value.clone(),
            });
        }
    }
    issues
}

/// Takes the `[bindings]` section verbatim; interpreting it is the game's job.
fn read_bindings(prefs: &mut Prefs, keys: &SectionKeys) -> Vec<PrefsIssue> {
    let mut issues = Vec::new();
    for (key, value) in sorted_entries(keys) {
        match value {
            Some(value) => {
                prefs.bindings.insert(key.clone(), value.clone());
            }
            None => issues.push(PrefsIssue::Malformed {
                section: "bindings".to_string(),
                key: key.clone(),
                value: None,
            }),
        }
    }
    issues
}

impl Prefs {
    /// Parses INI text. Every problem is returned alongside the preferences, and the
    /// affected key keeps its default. Keys before any header count as `[prefs]`
    /// unless the file has a `[prefs]` section, in which case they are ignored and
    /// reported as unknown keys of section `default`.
    pub fn parse(content: &str) -> (Self, Vec<PrefsIssue>) {
        let mut prefs = Self::default();
        let mut issues = Vec::new();
        let map = match ini::macro_safe_read(content) {
            Ok(map) => map,
            Err(reason) => return (prefs, vec![PrefsIssue::Unparseable { reason }]),
        };

        let mut sections: Vec<_> = map.iter().collect();
        sections.sort_by_key(|(name, _)| name.as_str());
        for (name, keys) in sections {
            if name == "bindings" {
                issues.extend(read_bindings(&mut prefs, keys));
                continue;
            }
            let table = match name.as_str() {
                "default" if map.contains_key("prefs") => {
                    issues.extend(sorted_entries(keys).into_iter().map(|(key, _)| {
                        PrefsIssue::UnknownKey {
                            section: name.clone(),
                            key: key.clone(),
                        }
                    }));
                    continue;
                }
                "default" => "prefs",
                other => other,
            };
            if !KNOWN_KEYS.iter().any(|(s, _, _)| *s == table) {
                issues.push(PrefsIssue::UnknownSection {
                    section: name.clone(),
                });
                continue;
            }
            issues.extend(read_section(&mut prefs, name, table, keys));
        }
        (prefs, issues)
    }

    /// Loads preferences from `path`, creating the file with defaults when it is missing.
    /// Any failure yields defaults (for the affected keys, or all of them) plus a warning.
    pub fn load_from(path: &Path) -> (Self, Vec<PrefsWarning>) {
        let warn = |issue| PrefsWarning {
            file: path.to_path_buf(),
            issue,
        };

        if !path.exists() {
            let defaults = Self::default();
            let warnings = match defaults.save_to(path) {
                Ok(()) => vec![],
                Err(e) => vec![warn(PrefsIssue::NotCreated {
                    reason: e.to_string(),
                })],
            };
            return (defaults, warnings);
        }

        match std::fs::read_to_string(path) {
            Ok(content) => {
                let (prefs, issues) = Self::parse(&content);
                (prefs, issues.into_iter().map(warn).collect())
            }
            Err(e) => (
                Self::default(),
                vec![warn(PrefsIssue::Unreadable {
                    reason: e.to_string(),
                })],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::WindowMode;
    use proptest::prelude::*;
    use std::fmt::Write as _;

    #[test]
    fn warning_display_names_file_and_detail_for_each_issue() {
        let issues = [
            (
                PrefsIssue::Malformed {
                    section: "audio".into(),
                    key: "music_volume".into(),
                    value: Some("loud".into()),
                },
                vec!["audio", "music_volume", "loud"],
            ),
            (
                PrefsIssue::Malformed {
                    section: "video".into(),
                    key: "window_mode".into(),
                    value: None,
                },
                vec!["video", "window_mode"],
            ),
            (
                PrefsIssue::UnknownKey {
                    section: "world".into(),
                    key: "mystery_key".into(),
                },
                vec!["world", "mystery_key"],
            ),
            (
                PrefsIssue::UnknownSection {
                    section: "mystery_section".into(),
                },
                vec!["mystery_section"],
            ),
            (
                PrefsIssue::Unparseable {
                    reason: "unparseable-reason".into(),
                },
                vec!["unparseable-reason"],
            ),
            (
                PrefsIssue::Unreadable {
                    reason: "unreadable-reason".into(),
                },
                vec!["unreadable-reason"],
            ),
            (
                PrefsIssue::NotCreated {
                    reason: "notcreated-reason".into(),
                },
                vec!["notcreated-reason"],
            ),
        ];

        for (issue, details) in issues {
            let warning = PrefsWarning {
                file: PathBuf::from("settings/prefs.ini"),
                issue,
            };

            let text = warning.to_string();

            assert!(
                text.contains("settings/prefs.ini"),
                "file missing in {text}"
            );
            for detail in details {
                assert!(text.contains(detail), "{detail} missing in {text}");
            }
        }
    }

    /// Every key with a valid value that differs from the default, as `(section, [(key, value)])`.
    const BASE: &[(&str, &[(&str, &str)])] = &[
        (
            "prefs",
            &[
                ("mouse_sensitivity", "2.5"),
                ("input_filtering_enabled", "false"),
            ],
        ),
        (
            "audio",
            &[
                ("sound_effect_volume", "1.5"),
                ("music_volume", "2.5"),
                ("ui_volume", "3.5"),
                ("voice_volume", "4.5"),
            ],
        ),
        (
            "graphics",
            &[("shadow_quality", "1"), ("ssao_quality", "2")],
        ),
        (
            "video",
            &[
                ("window_mode", "Borderless"),
                ("window_width", "1280"),
                ("window_height", "720"),
            ],
        ),
        (
            "world",
            &[
                ("load_radius", "5"),
                ("unload_radius", "9"),
                ("chunks_per_frame", "2"),
            ],
        ),
    ];

    /// Renders `BASE`, replacing one value; panics if the target is not in `BASE`.
    fn base_ini_with(replace: Option<(&str, &str, &str)>) -> String {
        let mut found = replace.is_none();
        let mut out = String::new();
        for (section, keys) in BASE {
            writeln!(out, "[{section}]").unwrap();
            for (key, value) in *keys {
                match replace {
                    Some((s, k, v)) if s == *section && k == *key => {
                        found = true;
                        writeln!(out, "{key}={v}").unwrap();
                    }
                    _ => writeln!(out, "{key}={value}").unwrap(),
                }
            }
        }
        assert!(found, "{replace:?} is not a key in BASE");
        out
    }

    /// `BASE` as a `Prefs`, built without going through the reader.
    fn base_prefs() -> Prefs {
        let mut p = Prefs::default();
        *p.mouse_sensitivity_mut() = 2.5;
        *p.input_filtering_enabled_mut() = false;
        *p.sound_effect_volume_mut() = 1.5;
        *p.music_volume_mut() = 2.5;
        *p.ui_volume_mut() = 3.5;
        *p.voice_volume_mut() = 4.5;
        p.set_shadow_quality(1);
        p.set_ssao_quality(2);
        p.set_window_mode(WindowMode::Borderless);
        p.set_window_resolution(1280, 720);
        p.set_world_load_radius(5);
        p.set_world_unload_radius(9);
        p.set_world_chunks_per_frame(2);
        p
    }

    /// Every persisted value, keyed `section.key`, so tests can compare key by key.
    fn snapshot(p: &Prefs) -> Vec<(&'static str, String)> {
        vec![
            (
                "prefs.mouse_sensitivity",
                format!("{:?}", p.mouse_sensitivity()),
            ),
            (
                "prefs.input_filtering_enabled",
                format!("{:?}", p.input_filtering_enabled()),
            ),
            (
                "audio.sound_effect_volume",
                format!("{:?}", p.sound_effect_volume()),
            ),
            ("audio.music_volume", format!("{:?}", p.music_volume())),
            ("audio.ui_volume", format!("{:?}", p.ui_volume())),
            ("audio.voice_volume", format!("{:?}", p.voice_volume())),
            (
                "graphics.shadow_quality",
                format!("{:?}", p.shadow_quality()),
            ),
            ("graphics.ssao_quality", format!("{:?}", p.ssao_quality())),
            ("video.window_mode", format!("{:?}", p.window_mode())),
            (
                "video.window_width",
                format!("{:?}", p.window_resolution().0),
            ),
            (
                "video.window_height",
                format!("{:?}", p.window_resolution().1),
            ),
            ("world.load_radius", format!("{:?}", p.world_load_radius())),
            (
                "world.unload_radius",
                format!("{:?}", p.world_unload_radius()),
            ),
            (
                "world.chunks_per_frame",
                format!("{:?}", p.world_chunks_per_frame()),
            ),
        ]
    }

    fn malformed(section: &str, key: &str, value: Option<&str>) -> PrefsIssue {
        PrefsIssue::Malformed {
            section: section.to_string(),
            key: key.to_string(),
            value: value.map(str::to_string),
        }
    }

    fn assert_base_differs_from_default() {
        let base = snapshot(&base_prefs());
        let default = snapshot(&Prefs::default());

        for ((name, base_value), (_, default_value)) in base.iter().zip(&default) {
            assert_ne!(
                base_value, default_value,
                "{name} must differ from default in BASE"
            );
        }
    }

    #[test]
    fn base_file_loads_to_base_prefs_without_issues() {
        assert_base_differs_from_default();

        let (prefs, issues) = Prefs::parse(&base_ini_with(None));

        assert_eq!(issues, vec![]);
        assert_eq!(prefs, base_prefs());
    }

    #[test]
    fn malformed_value_warns_and_only_that_key_falls_back() {
        assert_base_differs_from_default();

        let rows = [
            ("prefs", "mouse_sensitivity", "fast"),
            ("prefs", "mouse_sensitivity", "NaN"),
            ("prefs", "mouse_sensitivity", ""),
            ("prefs", "input_filtering_enabled", "yes"),
            ("audio", "music_volume", "5,0"),
            ("graphics", "shadow_quality", "high"),
            ("video", "window_mode", "Windowd"),
            ("video", "window_width", "-1920"),
            ("world", "chunks_per_frame", "0"),
        ];
        let default = snapshot(&Prefs::default());
        let base = snapshot(&base_prefs());

        for (section, key, raw) in rows {
            let label = format!("row [{section}] {key}={raw:?}");
            let target = format!("{section}.{key}");

            let (prefs, issues) = Prefs::parse(&base_ini_with(Some((section, key, raw))));

            // An empty value after '=' reads back as an empty string, not as "no '='".
            assert_eq!(issues, vec![malformed(section, key, Some(raw))], "{label}");
            for (i, (name, value)) in snapshot(&prefs).iter().enumerate() {
                let expected = if *name == target {
                    &default[i].1
                } else {
                    &base[i].1
                };
                assert_eq!(value, expected, "{label}: {name}");
            }
        }
    }

    #[test]
    fn key_without_value_is_malformed() {
        let content = "[video]\nwindow_mode\nwindow_width=1280\n";

        let (prefs, issues) = Prefs::parse(content);

        assert_eq!(issues, vec![malformed("video", "window_mode", None)]);
        assert_eq!(prefs.window_mode(), WindowMode::Windowed);
        assert_eq!(prefs.window_resolution().0, 1280);
    }

    #[test]
    fn each_malformed_key_gets_its_own_warning() {
        let content = "[prefs]\nmouse_sensitivity=fast\n[video]\nwindow_width=wide\n";

        let (prefs, mut issues) = Prefs::parse(content);

        issues.sort_by_key(|i| format!("{i:?}"));
        assert_eq!(
            issues,
            vec![
                malformed("prefs", "mouse_sensitivity", Some("fast")),
                malformed("video", "window_width", Some("wide")),
            ]
        );
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn missing_section_is_not_a_warning() {
        let content: String = base_ini_with(None)
            .split("[world]")
            .next()
            .unwrap()
            .to_string();
        assert!(
            !content.contains("chunks_per_frame"),
            "test setup: [world] must be gone"
        );

        let (prefs, issues) = Prefs::parse(&content);

        assert_eq!(issues, vec![]);
        let default = Prefs::default();
        assert_eq!(prefs.world_load_radius(), default.world_load_radius());
        assert_eq!(prefs.world_unload_radius(), default.world_unload_radius());
        assert_eq!(
            prefs.world_chunks_per_frame(),
            default.world_chunks_per_frame()
        );
        assert_eq!(prefs.mouse_sensitivity(), 2.5);
        assert_eq!(prefs.window_mode(), WindowMode::Borderless);
    }

    #[test]
    fn unknown_key_is_reported() {
        let content = "[prefs]\nmouse_sensitivty=2.0\n";

        let (prefs, issues) = Prefs::parse(content);

        assert_eq!(
            issues,
            vec![PrefsIssue::UnknownKey {
                section: "prefs".to_string(),
                key: "mouse_sensitivty".to_string(),
            }]
        );
        assert_eq!(
            prefs.mouse_sensitivity(),
            Prefs::default().mouse_sensitivity()
        );
    }

    #[test]
    fn old_key_lines_are_unknown_keys() {
        let (prefs, issues) = Prefs::parse("[prefs]\nkey_w = Z\n");

        assert_eq!(
            issues,
            vec![PrefsIssue::UnknownKey {
                section: "prefs".to_string(),
                key: "key_w".to_string(),
            }]
        );
        assert!(prefs.bindings().is_empty());
    }

    #[test]
    fn bindings_line_without_value_is_malformed() {
        let (prefs, issues) = Prefs::parse("[bindings]\njump\n");

        assert_eq!(issues, vec![malformed("bindings", "jump", None)]);
        assert!(prefs.bindings().is_empty());
    }

    #[test]
    fn unknown_section_is_reported_and_its_keys_default() {
        let content = base_ini_with(None).replace("[video]", "[vidoe]");

        let (prefs, issues) = Prefs::parse(&content);

        assert_eq!(
            issues,
            vec![PrefsIssue::UnknownSection {
                section: "vidoe".to_string()
            }]
        );
        let default = Prefs::default();
        assert_eq!(prefs.window_mode(), default.window_mode());
        assert_eq!(prefs.window_resolution(), default.window_resolution());
        assert!(!prefs.input_filtering_enabled());
        assert_eq!(prefs.music_volume(), 2.5);
    }

    #[test]
    fn unparseable_file_falls_back_with_parser_reason() {
        let content = "[prefs]\nmouse_sensitivity=2.5\n[video\nwindow_mode=Fullscreen\n";

        let (prefs, issues) = Prefs::parse(content);

        assert_eq!(issues.len(), 1, "{issues:?}");
        let PrefsIssue::Unparseable { reason } = &issues[0] else {
            panic!("expected Unparseable, got {:?}", issues[0]);
        };
        assert!(!reason.is_empty());
        // The parser counts lines from 0; "[video" is line index 2.
        assert!(
            reason.starts_with("line 2:"),
            "reason should carry the line: {reason}"
        );
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn unreadable_file_falls_back_with_io_reason() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.ini");
        std::fs::write(&path, [0xff, 0xfe, 0xfd, b'\n']).unwrap();

        let (prefs, warnings) = Prefs::load_from(&path);

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].file, path);
        let PrefsIssue::Unreadable { reason } = &warnings[0].issue else {
            panic!("expected Unreadable, got {:?}", warnings[0].issue);
        };
        assert!(!reason.is_empty());
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn failed_default_file_creation_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "not a directory").unwrap();
        let path = blocker.join("prefs.ini");

        let (prefs, warnings) = Prefs::load_from(&path);

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].file, path);
        let PrefsIssue::NotCreated { reason } = &warnings[0].issue else {
            panic!("expected NotCreated, got {:?}", warnings[0].issue);
        };
        assert!(!reason.is_empty());
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn missing_file_is_created_with_defaults_and_no_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("prefs.ini");

        let (prefs, warnings) = Prefs::load_from(&path);

        assert_eq!(warnings, vec![]);
        assert_eq!(prefs, Prefs::default());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            Prefs::default().to_ini_string()
        );
    }

    #[test]
    fn warning_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.ini");
        std::fs::write(&path, "[prefs]\nmouse_sensitivity=fast\n").unwrap();

        let (_, warnings) = Prefs::load_from(&path);

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].file, path);
        let text = warnings[0].to_string();
        for needle in [path.to_str().unwrap(), "prefs", "mouse_sensitivity", "fast"] {
            assert!(text.contains(needle), "{needle:?} missing from {text:?}");
        }
    }

    #[test]
    fn keys_before_any_header_load_as_prefs() {
        let content = "input_filtering_enabled=false\nmouse_sensitivity=2.5\n";

        let (prefs, issues) = Prefs::parse(content);

        assert_eq!(issues, vec![]);
        assert!(!prefs.input_filtering_enabled());
        assert_eq!(prefs.mouse_sensitivity(), 2.5);
    }

    #[test]
    fn keys_before_header_are_reported_when_prefs_section_exists() {
        let content = "mouse_sensitivity=2.0\n[prefs]\ninput_filtering_enabled=false\n";

        let (prefs, issues) = Prefs::parse(content);

        assert_eq!(
            issues,
            vec![PrefsIssue::UnknownKey {
                section: "default".into(),
                key: "mouse_sensitivity".into(),
            }]
        );
        assert_eq!(
            prefs.mouse_sensitivity(),
            Prefs::default().mouse_sensitivity()
        );
        assert!(!prefs.input_filtering_enabled());
    }

    /// Pins the exact text written for the defaults, so a format change is deliberate.
    #[test]
    fn to_ini_string_of_defaults_is_the_save_format() {
        let expected = "[prefs]\n\
             mouse_sensitivity=1\n\
             input_filtering_enabled=true\n\
             \n[audio]\n\
             sound_effect_volume=7.0\n\
             music_volume=5.0\n\
             ui_volume=8.0\n\
             voice_volume=7.0\n\
             \n[graphics]\n\
             shadow_quality=3\n\
             ssao_quality=3\n\
             \n[video]\n\
             window_mode=Windowed\n\
             window_width=1920\n\
             window_height=1080\n\
             \n[world]\n\
             load_radius=8\n\
             unload_radius=12\n\
             chunks_per_frame=4\n";

        assert_eq!(Prefs::default().to_ini_string(), expected);
        assert_eq!(Prefs::parse(expected), (Prefs::default(), vec![]));
    }

    #[test]
    fn save_writes_to_ini_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("prefs.ini");
        let prefs = base_prefs();

        prefs.save_to(&path).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            prefs.to_ini_string()
        );
    }

    fn volume_strategy() -> impl Strategy<Value = f32> {
        (0u32..=100).prop_map(|n| n as f32 / 10.0)
    }

    fn window_mode_strategy() -> impl Strategy<Value = WindowMode> {
        prop_oneof![
            Just(WindowMode::Windowed),
            Just(WindowMode::Fullscreen),
            Just(WindowMode::Borderless),
        ]
    }

    fn prefs_strategy() -> impl Strategy<Value = Prefs> {
        (
            any::<f32>().prop_filter("finite", |v| v.is_finite()),
            any::<bool>(),
            [
                volume_strategy(),
                volume_strategy(),
                volume_strategy(),
                volume_strategy(),
            ],
            (0u32..=4, 0u32..=4),
            (window_mode_strategy(), any::<u32>(), any::<u32>()),
            (any::<u32>(), any::<u32>(), 1u32..=u32::MAX),
        )
            .prop_map(|(sens, filtering, vols, quality, video, world)| {
                let mut p = Prefs::default();
                *p.mouse_sensitivity_mut() = sens;
                *p.input_filtering_enabled_mut() = filtering;
                *p.sound_effect_volume_mut() = vols[0];
                *p.music_volume_mut() = vols[1];
                *p.ui_volume_mut() = vols[2];
                *p.voice_volume_mut() = vols[3];
                p.set_shadow_quality(quality.0);
                p.set_ssao_quality(quality.1);
                p.set_window_mode(video.0);
                p.set_window_resolution(video.1, video.2);
                p.set_world_load_radius(world.0);
                p.set_world_unload_radius(world.1);
                p.set_world_chunks_per_frame(world.2);
                p
            })
    }

    proptest! {
        #[test]
        fn saved_prefs_parse_back_unchanged_without_warnings(prefs in prefs_strategy()) {
            let text = prefs.to_ini_string();

            let (parsed, issues) = Prefs::parse(&text);

            prop_assert_eq!(issues, vec![], "text:\n{}", text);
            prop_assert_eq!(parsed, prefs);
        }
    }
}
