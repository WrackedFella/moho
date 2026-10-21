//! Registry of the settings screen's bindings, with conflict detection.

use moho_input::bindings::{ActionBindings, Binding};
use moho_input::key::Key;
use moho_ui_shell::CONSOLE_BINDING;

use super::types::BINDING_ROWS;
use crate::actions::{StrategyAction, load_bindings};
use crate::prefs::Prefs;

/// The bindings being edited, and the rule that one key serves one listed row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingRegistry {
    bindings: ActionBindings<StrategyAction>,
}

impl BindingRegistry {
    /// Reads the bindings from prefs, logging each line that had to fall back to a default.
    pub fn from_prefs(prefs: &Prefs) -> Self {
        Self {
            bindings: load_bindings(prefs),
        }
    }

    pub fn write_to_prefs(&self, prefs: &mut Prefs) {
        let mut section = self.bindings.to_section();
        if let Some(console) = prefs.bindings().get(CONSOLE_BINDING) {
            section.insert(CONSOLE_BINDING.to_owned(), console.clone());
        }
        prefs.set_bindings(section);
    }

    /// The listed row (other than `exclude`) already bound to `key`, if any.
    pub fn find_conflict(&self, key: Key, exclude: StrategyAction) -> Option<StrategyAction> {
        BINDING_ROWS
            .into_iter()
            .find(|&row| row != exclude && self.bindings.get(row).contains(&Binding::key(key)))
    }

    pub fn set_binding(&mut self, action: StrategyAction, bindings: Vec<Binding>) {
        self.bindings.set(action, bindings);
    }

    pub fn get_binding(&self, action: StrategyAction) -> &[Binding] {
        self.bindings.get(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_input::bindings::Input;

    fn registry() -> BindingRegistry {
        BindingRegistry::from_prefs(&Prefs::default())
    }

    #[test]
    fn from_prefs_loads_defaults_for_every_listed_row() {
        let registry = registry();

        for row in BINDING_ROWS {
            assert_eq!(
                registry.get_binding(row),
                moho_input::Action::default_bindings(row),
                "{row:?}"
            );
        }
    }

    #[test]
    fn write_to_prefs_round_trips_changes() {
        let mut prefs = Prefs::default();
        let mut registry = BindingRegistry::from_prefs(&prefs);
        registry.set_binding(StrategyAction::MoveForward, vec![Binding::key(Key::Q)]);
        registry.set_binding(StrategyAction::MoveLeft, vec![Binding::key(Key::E)]);

        registry.write_to_prefs(&mut prefs);

        let reloaded = BindingRegistry::from_prefs(&prefs);
        assert_eq!(reloaded, registry);
        assert_eq!(
            reloaded.get_binding(StrategyAction::MoveForward),
            [Binding::key(Key::Q)]
        );
    }

    #[test]
    fn find_conflict_covers_every_listed_row() {
        let registry = registry();

        for row in BINDING_ROWS {
            let asker = BINDING_ROWS.into_iter().find(|&r| r != row).unwrap();
            let keys: Vec<Key> = moho_input::Action::default_bindings(row)
                .iter()
                .filter_map(|b| match b.input {
                    Input::Key(key) => Some(key),
                    Input::Mouse(_) | Input::Pad(_) => None,
                })
                .collect();
            assert!(!keys.is_empty(), "{row:?} has a default key");
            for key in keys {
                assert_eq!(registry.find_conflict(key, asker), Some(row), "{row:?}");
            }
        }
    }

    #[test]
    fn find_conflict_matches_any_key_of_a_multi_key_row() {
        let mut registry = registry();
        registry.set_binding(
            StrategyAction::MoveForward,
            vec![Binding::key(Key::W), Binding::key(Key::ArrowUp)],
        );

        assert_eq!(
            registry.find_conflict(Key::ArrowUp, StrategyAction::MoveLeft),
            Some(StrategyAction::MoveForward)
        );
    }

    #[test]
    fn find_conflict_ignores_exclude_id() {
        let registry = registry();

        assert_eq!(
            registry.find_conflict(Key::W, StrategyAction::MoveForward),
            None
        );
    }

    #[test]
    fn find_conflict_returns_none_for_unique_binding() {
        let registry = registry();

        assert_eq!(
            registry.find_conflict(Key::Q, StrategyAction::MoveForward),
            None
        );
    }

    #[test]
    fn find_conflict_ignores_jump_which_shares_space_with_ascend() {
        let registry = registry();

        assert_eq!(
            registry.find_conflict(Key::Space, StrategyAction::Ascend),
            None
        );
    }

    #[test]
    fn find_conflict_ignores_a_hold_on_the_key() {
        let mut registry = registry();
        registry.set_binding(
            StrategyAction::MoveForward,
            vec![Binding::key(Key::W), Binding::key(Key::R).held()],
        );

        assert_eq!(
            registry.find_conflict(Key::R, StrategyAction::MoveLeft),
            None
        );
        assert_eq!(
            registry.find_conflict(Key::W, StrategyAction::MoveLeft),
            Some(StrategyAction::MoveForward)
        );
    }

    #[test]
    fn unbound_row_conflicts_with_nothing() {
        let mut registry = registry();
        registry.set_binding(StrategyAction::MoveForward, vec![]);

        assert_eq!(
            registry.find_conflict(Key::W, StrategyAction::MoveBack),
            None
        );
    }

    #[test]
    fn set_binding_replaces_the_rows_keys() {
        let mut registry = registry();

        registry.set_binding(StrategyAction::MoveForward, vec![Binding::key(Key::Q)]);

        assert_eq!(
            registry.get_binding(StrategyAction::MoveForward),
            [Binding::key(Key::Q)]
        );
    }

    #[test]
    fn writing_bindings_keeps_the_console_entry() {
        let (mut prefs, _) = Prefs::parse("[bindings]\nconsole = Backslash\nmove_forward = W\n");
        let mut registry = BindingRegistry::from_prefs(&prefs);
        registry.set_binding(StrategyAction::MoveForward, vec![Binding::key(Key::Q)]);

        registry.write_to_prefs(&mut prefs);

        assert_eq!(
            prefs.bindings().get("console").map(String::as_str),
            Some("Backslash")
        );
        assert_eq!(
            BindingRegistry::from_prefs(&prefs).get_binding(StrategyAction::MoveForward),
            [Binding::key(Key::Q)]
        );
    }

    #[test]
    fn writing_bindings_adds_no_console_entry_when_the_prefs_have_none() {
        let mut prefs = Prefs::default();
        let registry = BindingRegistry::from_prefs(&prefs);

        registry.write_to_prefs(&mut prefs);

        assert_eq!(prefs.bindings().get("console"), None);
    }
}
