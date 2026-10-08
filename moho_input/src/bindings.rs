//! Action-keyed bindings: what a game declares, and how they persist.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

use crate::key::Key;

/// One physical input that triggers an action.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Binding {
    Key(Key),
}

/// A game's action enum: stable names and default bindings, as data.
pub trait Action: Copy + Eq + Hash + Debug + 'static {
    const ALL: &'static [Self];

    /// Stable id used as the persisted key; never renamed once shipped.
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
    map: BTreeMap<&'static str, Vec<Binding>>,
    _action: PhantomData<A>,
}

impl<A: Action> ActionBindings<A> {
    /// Reads the raw `[bindings]` section. Absent actions and unusable lines keep
    /// their defaults; each unusable line yields a warning.
    pub fn load(_section: &BTreeMap<String, String>) -> (Self, Vec<BindingWarning>) {
        let empty = Self {
            map: BTreeMap::new(),
            _action: PhantomData,
        };
        (empty, Vec::new())
    }

    /// Writes every action, including those left at their defaults.
    pub fn to_section(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    pub fn get(&self, _action: A) -> &[Binding] {
        &[]
    }

    pub fn set(&mut self, _action: A, _bindings: Vec<Binding>) {}
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

    #[test]
    fn rebinding_survives_save_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.ini");
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
        bindings.set(TestAction::Jump, vec![Binding::Key(Key::F)]);
        let mut prefs = Prefs::default();
        prefs.set_bindings(bindings.to_section());
        std::fs::write(&path, prefs.to_ini_string()).unwrap();

        let (reloaded, prefs_warnings) = Prefs::load_from(&path);
        let (restored, binding_warnings) = ActionBindings::<TestAction>::load(reloaded.bindings());

        assert_eq!(prefs_warnings, vec![]);
        assert_eq!(binding_warnings, vec![]);
        assert_eq!(restored.get(TestAction::Jump), [Binding::Key(Key::F)]);
        assert!(
            !restored
                .get(TestAction::Jump)
                .contains(&Binding::Key(Key::Space))
        );
        assert_defaults_except(&restored, &[TestAction::Jump]);
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
    fn bad_line_is_reported_and_default_kept() {
        let cases: [(&[(&str, &str)], &str); 3] = [
            (&[("jump", "Banana")], "jump"),
            (&[("teleport", "T")], "teleport"),
            (&[("move_forward", "Z"), ("jump", "Banana")], "jump"),
        ];

        for (lines, reported) in cases {
            let (bindings, warnings) = ActionBindings::<TestAction>::load(&section(lines));

            let names: Vec<_> = warnings.iter().map(|w| w.name.as_str()).collect();
            assert_eq!(names, vec![reported], "{lines:?}");
            let valid: Vec<_> = lines
                .iter()
                .filter(|&&(k, _)| k == "move_forward")
                .collect();
            if valid.is_empty() {
                assert_defaults_except(&bindings, &[]);
            } else {
                assert_eq!(
                    bindings.get(TestAction::MoveForward),
                    [Binding::Key(Key::Z)],
                    "{lines:?}"
                );
                assert_defaults_except(&bindings, &[TestAction::MoveForward]);
            }
        }
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
