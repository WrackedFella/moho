//! Registry of the settings screen's bindings, with conflict detection.

use moho_input::bindings::{ActionBindings, Binding};
use moho_input::key::Key;

use super::types::BINDING_ROWS;
use crate::actions::StrategyAction;
use crate::prefs::Prefs;

/// The bindings being edited, and the rule that one key serves one listed row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingRegistry {
    bindings: ActionBindings<StrategyAction>,
}

impl BindingRegistry {
    /// Reads the bindings from prefs, logging each line that had to fall back to a default.
    pub fn from_prefs(prefs: &Prefs) -> Self {
        let (bindings, warnings) = ActionBindings::load(prefs.bindings());
        for w in warnings {
            tracing::warn!(action = %w.name, value = %w.value, "Unusable binding, default kept");
        }
        Self { bindings }
    }

    pub fn write_to_prefs(&self, prefs: &mut Prefs) {
        prefs.set_bindings(self.bindings.to_section());
    }

    /// The listed row (other than `exclude`) already bound to `key`, if any.
    pub fn find_conflict(&self, key: Key, exclude: StrategyAction) -> Option<StrategyAction> {
        BINDING_ROWS
            .into_iter()
            .find(|&row| row != exclude && self.bindings.get(row).contains(&Binding::Key(key)))
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
        registry.set_binding(StrategyAction::MoveForward, vec![Binding::Key(Key::Q)]);
        registry.set_binding(StrategyAction::MoveLeft, vec![Binding::Key(Key::E)]);

        registry.write_to_prefs(&mut prefs);

        let reloaded = BindingRegistry::from_prefs(&prefs);
        assert_eq!(reloaded, registry);
        assert_eq!(
            reloaded.get_binding(StrategyAction::MoveForward),
            [Binding::Key(Key::Q)]
        );
    }

    #[test]
    fn find_conflict_detects_duplicate_bindings() {
        let registry = registry();

        assert_eq!(
            registry.find_conflict(Key::W, StrategyAction::MoveBack),
            Some(StrategyAction::MoveForward)
        );
        assert_eq!(
            registry.find_conflict(Key::A, StrategyAction::MoveBack),
            Some(StrategyAction::MoveLeft)
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

        registry.set_binding(StrategyAction::MoveForward, vec![Binding::Key(Key::Q)]);

        assert_eq!(
            registry.get_binding(StrategyAction::MoveForward),
            [Binding::Key(Key::Q)]
        );
    }
}
