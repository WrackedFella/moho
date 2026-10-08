//! Action-keyed bindings: what a game declares, and how they persist.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::hash::Hash;

use crate::key::{Key, MouseButton};

/// One physical input that triggers an action.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Binding {
    Key(Key),
    Mouse(MouseButton),
}

/// A game's action enum: stable names and default bindings, as data.
pub trait Action: Copy + Eq + Hash + Debug + 'static {
    const ALL: &'static [Self];

    /// Stable id used as the persisted key; never renamed once shipped. Lowercase
    /// `snake_case`: the INI reader lowercases keys, so other ids would not load back.
    fn name(self) -> &'static str;

    fn default_bindings(self) -> &'static [Binding];
}

/// A line of the `[bindings]` section that could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingWarning {
    /// The line's key as written: a declared action with a bad value, or an unknown action.
    pub name: String,
    pub value: String,
}

/// Every action's current bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionBindings<A: Action> {
    /// Holds every action in `A::ALL`; `defaults` fills it and `set` only replaces.
    map: HashMap<A, Vec<Binding>>,
}

impl<A: Action> ActionBindings<A> {
    fn defaults() -> Self {
        Self {
            map: A::ALL
                .iter()
                .map(|&a| (a, a.default_bindings().to_vec()))
                .collect(),
        }
    }

    /// Reads the raw `[bindings]` section. Absent actions and unusable lines keep
    /// their defaults; each unusable line yields a warning.
    pub fn load(section: &BTreeMap<String, String>) -> (Self, Vec<BindingWarning>) {
        let mut loaded = Self::defaults();
        let mut warnings = Vec::new();
        for (name, value) in section {
            let action = A::ALL.iter().copied().find(|a| a.name() == name);
            let parsed = action.and_then(|_| parse_list(value));
            match (action, parsed) {
                (Some(action), Some(bindings)) => {
                    loaded.map.insert(action, bindings);
                }
                _ => warnings.push(BindingWarning {
                    name: name.clone(),
                    value: value.clone(),
                }),
            }
        }
        (loaded, warnings)
    }

    /// Writes every action, including those left at their defaults.
    pub fn to_section(&self) -> BTreeMap<String, String> {
        self.map
            .iter()
            .map(|(action, bindings)| (action.name().to_string(), format_list(bindings)))
            .collect()
    }

    /// An action missing from `A::ALL` reads as unbound.
    pub fn get(&self, action: A) -> &[Binding] {
        self.map.get(&action).map_or(&[], Vec::as_slice)
    }

    pub fn set(&mut self, action: A, bindings: Vec<Binding>) {
        self.map.insert(action, bindings);
    }
}

const UNBOUND: &str = "Unbound";

/// `None` if any item is not a known key; empty or `Unbound` is an empty list.
fn parse_list(value: &str) -> Option<Vec<Binding>> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case(UNBOUND) {
        return Some(Vec::new());
    }
    value
        .split(',')
        .map(|item| Key::parse(item.trim()).map(Binding::Key))
        .collect()
}

fn format_list(bindings: &[Binding]) -> String {
    if bindings.is_empty() {
        return UNBOUND.to_string();
    }
    bindings
        .iter()
        .map(|binding| match binding {
            Binding::Key(key) => key.name(),
            Binding::Mouse(_) => todo!("Mouse binding names"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_core::prefs::Prefs;

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum TestAction {
        MoveForward,
        Jump,
    }

    impl Action for TestAction {
        const ALL: &'static [Self] = &[TestAction::MoveForward, TestAction::Jump];

        fn name(self) -> &'static str {
            match self {
                TestAction::MoveForward => "move_forward",
                TestAction::Jump => "jump",
            }
        }

        fn default_bindings(self) -> &'static [Binding] {
            match self {
                TestAction::MoveForward => &[Binding::Key(Key::W)],
                TestAction::Jump => &[Binding::Key(Key::Space)],
            }
        }
    }

    fn section(lines: &[(&str, &str)]) -> BTreeMap<String, String> {
        lines
            .iter()
            .map(|&(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn assert_defaults_except(bindings: &ActionBindings<TestAction>, skip: &[TestAction]) {
        for &action in TestAction::ALL.iter().filter(|a| !skip.contains(a)) {
            assert_eq!(
                bindings.get(action),
                action.default_bindings(),
                "{}",
                action.name()
            );
        }
    }

    fn keys(keys: &[Key]) -> Vec<Binding> {
        keys.iter().map(|&k| Binding::Key(k)).collect()
    }

    #[test]
    fn rebinding_survives_save_and_reload() {
        let cases: [Vec<Key>; 8] = [
            vec![Key::F],
            vec![],
            vec![Key::F, Key::Space],
            vec![Key::Comma],
            vec![Key::Semicolon],
            vec![Key::Equals],
            vec![Key::LeftBracket],
            vec![Key::Backslash],
        ];

        for rebound in cases {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("prefs.ini");
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            bindings.set(TestAction::Jump, keys(&rebound));
            let mut prefs = Prefs::default();
            prefs.set_bindings(bindings.to_section());
            std::fs::write(&path, prefs.to_ini_string()).unwrap();

            let (reloaded, prefs_warnings) = Prefs::load_from(&path);
            let (restored, binding_warnings) =
                ActionBindings::<TestAction>::load(reloaded.bindings());

            assert_eq!(prefs_warnings, vec![], "{rebound:?}");
            assert_eq!(binding_warnings, vec![], "{rebound:?}");
            assert_eq!(
                restored.get(TestAction::Jump),
                keys(&rebound),
                "{rebound:?}"
            );
            assert_defaults_except(&restored, &[TestAction::Jump]);
        }
    }

    #[test]
    fn mouse_bindings_round_trip_by_name() {
        let cases = [
            (MouseButton::Left, "Mouse Left"),
            (MouseButton::Right, "Mouse Right"),
            (MouseButton::Middle, "Mouse Middle"),
        ];

        for (button, name) in cases {
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            bindings.set(
                TestAction::Jump,
                vec![Binding::Key(Key::F), Binding::Mouse(button)],
            );

            let written = bindings.to_section();
            let (restored, warnings) = ActionBindings::<TestAction>::load(&written);

            assert_eq!(written["jump"], format!("F, {name}"));
            assert_eq!(warnings, vec![], "{name}");
            assert_eq!(
                restored.get(TestAction::Jump),
                [Binding::Key(Key::F), Binding::Mouse(button)],
                "{name}"
            );
        }
    }

    #[test]
    fn to_section_writes_every_action_by_name() {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
        bindings.set(TestAction::Jump, keys(&[Key::F]));

        let written = bindings.to_section();

        assert_eq!(written, section(&[("move_forward", "W"), ("jump", "F")]));
    }

    #[test]
    fn missing_action_gets_its_default() {
        let cases = [section(&[]), section(&[("move_forward", "Z")])];

        for raw in cases {
            let (bindings, warnings) = ActionBindings::<TestAction>::load(&raw);

            assert_eq!(warnings, vec![], "{raw:?}");
            assert_eq!(
                bindings.get(TestAction::Jump),
                [Binding::Key(Key::Space)],
                "{raw:?}"
            );
        }
        let (bindings, _) = ActionBindings::<TestAction>::load(&section(&[("jump", "F")]));
        assert_eq!(
            bindings.get(TestAction::MoveForward),
            [Binding::Key(Key::W)]
        );
    }

    #[test]
    fn multi_key_value_tolerates_whitespace() {
        let (bindings, warnings) =
            ActionBindings::<TestAction>::load(&section(&[("jump", "F ,  Space")]));

        assert_eq!(warnings, vec![]);
        assert_eq!(bindings.get(TestAction::Jump), keys(&[Key::F, Key::Space]));
    }

    type BadLineCase<'a> = (
        &'a [(&'a str, &'a str)],
        &'a [&'a str],
        &'a [Key],
        &'a [Key],
    );

    #[test]
    fn bad_line_is_reported_and_default_kept() {
        // (lines, reported names, move_forward after load, jump after load)
        let cases: [BadLineCase; 4] = [
            (&[("jump", "Banana")], &["jump"], &[Key::W], &[Key::Space]),
            (
                &[("teleport", "T")],
                &["teleport"],
                &[Key::W],
                &[Key::Space],
            ),
            (
                &[("move_forward", "Z"), ("jump", "Banana")],
                &["jump"],
                &[Key::Z],
                &[Key::Space],
            ),
            (
                &[("jump", "F, Banana")],
                &["jump"],
                &[Key::W],
                &[Key::Space],
            ),
        ];

        for (lines, reported, forward, jump) in cases {
            let (bindings, warnings) = ActionBindings::<TestAction>::load(&section(lines));

            let names: Vec<_> = warnings.iter().map(|w| w.name.as_str()).collect();
            assert_eq!(names, reported, "{lines:?}");
            assert_eq!(
                bindings.get(TestAction::MoveForward),
                keys(forward),
                "{lines:?}"
            );
            assert_eq!(bindings.get(TestAction::Jump), keys(jump), "{lines:?}");
        }
    }

    #[test]
    fn unknown_action_line_does_not_disturb_valid_lines() {
        let raw = section(&[("jump", "F"), ("teleport", "T")]);

        let (bindings, warnings) = ActionBindings::<TestAction>::load(&raw);

        let names: Vec<_> = warnings.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["teleport"]);
        assert_eq!(bindings.get(TestAction::Jump), keys(&[Key::F]));
        assert_defaults_except(&bindings, &[TestAction::Jump]);
    }

    #[test]
    fn unbound_leaves_action_without_binding() {
        for spelling in ["Unbound", "unbound", "UNBOUND", ""] {
            let (bindings, warnings) =
                ActionBindings::<TestAction>::load(&section(&[("jump", spelling)]));

            assert_eq!(warnings, vec![], "{spelling:?}");
            assert_eq!(bindings.get(TestAction::Jump), [], "{spelling:?}");
            assert_defaults_except(&bindings, &[TestAction::Jump]);
        }
    }
}
