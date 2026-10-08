//! State management for settings menu.
//!
//! This module encapsulates dirty tracking, staged changes, and conflict resolution
//! state, providing a clean API for state mutations and queries.

use super::SettingsField;
use super::binding_registry::BindingRegistry;
use crate::actions::StrategyAction;
use crate::prefs::Prefs;
use moho_input::bindings::Binding;
use std::collections::HashSet;

/// Manages the state of the settings menu including saved prefs, staged changes,
/// and dirty field tracking.
///
/// Separates concerns:
/// - `prefs`: Last saved state (loaded from disk)
/// - `staged`: Current working state (not yet saved)
/// - `dirty_fields`: Which fields have been modified
#[derive(Clone, Debug)]
pub struct SettingsState {
    /// The last saved preferences (source of truth on disk)
    prefs: Prefs,
    /// Staged changes (working copy, not yet saved)
    staged: Prefs,
    /// Bindings as last saved
    saved_bindings: BindingRegistry,
    /// Bindings being edited; written into `staged` on save
    staged_bindings: BindingRegistry,
    /// Fields that have been modified (differ from prefs)
    dirty_fields: HashSet<SettingsField>,
}

impl SettingsState {
    /// Create a new settings state by loading preferences from disk.
    ///
    /// # Returns
    /// A new SettingsState with prefs loaded and no dirty fields.
    pub fn new() -> Self {
        Self::from_prefs(Prefs::load())
    }

    /// Create a settings state from existing preferences.
    ///
    /// # Arguments
    /// * `prefs` - The preferences to initialize with
    ///
    /// # Returns
    /// A new SettingsState with the given prefs and no dirty fields.
    pub fn from_prefs(prefs: Prefs) -> Self {
        let bindings = BindingRegistry::from_prefs(&prefs);
        Self {
            prefs: prefs.clone(),
            staged: prefs,
            saved_bindings: bindings.clone(),
            staged_bindings: bindings,
            dirty_fields: HashSet::new(),
        }
    }

    /// Check if there are any unsaved changes.
    ///
    /// # Returns
    /// true if any fields have been modified, false otherwise.
    pub fn is_dirty(&self) -> bool {
        !self.dirty_fields.is_empty()
    }

    /// Mark a specific field as dirty.
    ///
    /// # Arguments
    /// * `field` - The field that has been modified
    pub fn mark_dirty(&mut self, field: SettingsField) {
        self.dirty_fields.insert(field);
    }

    /// Check if a specific field is dirty.
    ///
    /// # Arguments
    /// * `field` - The field to check
    ///
    /// # Returns
    /// true if the field has been modified, false otherwise.
    #[allow(dead_code)]
    pub fn is_field_dirty(&self, field: SettingsField) -> bool {
        self.dirty_fields.contains(&field)
    }

    /// Get a reference to the saved preferences.
    ///
    /// # Returns
    /// Reference to the last saved Prefs
    pub fn prefs(&self) -> &Prefs {
        &self.prefs
    }

    /// Get a reference to the staged (working) preferences.
    ///
    /// # Returns
    /// Reference to the staged Prefs
    pub fn staged(&self) -> &Prefs {
        &self.staged
    }

    /// Get a mutable reference to the staged preferences.
    ///
    /// # Returns
    /// Mutable reference to the staged Prefs
    pub fn staged_mut(&mut self) -> &mut Prefs {
        &mut self.staged
    }

    /// Apply staged changes by saving them to disk and updating saved prefs.
    ///
    /// # Returns
    /// Result indicating success or failure of save operation
    pub fn apply_changes(&mut self) -> Result<(), std::io::Error> {
        self.staged_bindings.write_to_prefs(&mut self.staged);
        self.staged.save()?;
        self.prefs = self.staged.clone();
        self.saved_bindings = self.staged_bindings.clone();
        self.dirty_fields.clear();
        Ok(())
    }

    /// Revert staged changes back to the last saved state.
    ///
    /// Discards all uncommitted changes and clears dirty flags.
    pub fn revert_changes(&mut self) {
        self.staged = self.prefs.clone();
        self.staged_bindings = self.saved_bindings.clone();
        self.dirty_fields.clear();
    }

    /// The staged bindings, for conflict checks.
    pub fn staged_bindings(&self) -> &BindingRegistry {
        &self.staged_bindings
    }

    /// The staged bindings of one action.
    pub fn get_staged_binding(&self, action: StrategyAction) -> &[Binding] {
        self.staged_bindings.get_binding(action)
    }

    /// Replace the staged bindings of one action and mark it dirty.
    pub fn set_staged_binding(&mut self, action: StrategyAction, bindings: Vec<Binding>) {
        self.staged_bindings.set_binding(action, bindings);
        self.mark_dirty(SettingsField::Binding(action));
    }

    /// Whether the staged bindings of `action` differ from the saved ones.
    pub fn is_binding_modified(&self, action: StrategyAction) -> bool {
        self.staged_bindings.get_binding(action) != self.saved_bindings.get_binding(action)
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_input::key::Key;

    fn key(k: Key) -> Vec<Binding> {
        vec![Binding::Key(k)]
    }

    #[test]
    fn new_state_has_no_dirty_fields() {
        let state = SettingsState::new();
        assert!(!state.is_dirty());
    }

    #[test]
    fn mark_dirty_sets_dirty_flag() {
        let mut state = SettingsState::new();
        let forward = SettingsField::Binding(StrategyAction::MoveForward);
        let left = SettingsField::Binding(StrategyAction::MoveLeft);
        assert!(!state.is_dirty());

        state.mark_dirty(forward);
        assert!(state.is_dirty());
        assert!(state.is_field_dirty(forward));
        assert!(!state.is_field_dirty(left));
    }

    #[test]
    fn revert_changes_clears_dirty_fields() {
        let mut state = SettingsState::from_prefs(Prefs::default());
        state.set_staged_binding(StrategyAction::MoveForward, key(Key::Q));
        assert!(state.is_dirty());

        state.revert_changes();

        assert!(!state.is_dirty());
        assert_eq!(
            state.get_staged_binding(StrategyAction::MoveForward),
            key(Key::W)
        );
    }

    #[test]
    fn apply_changes_clears_dirty_fields() {
        let mut state = SettingsState::from_prefs(Prefs::default());
        state.set_staged_binding(StrategyAction::MoveForward, key(Key::Q));
        assert!(state.is_dirty());

        let result = state.apply_changes();
        assert!(result.is_ok());

        assert!(!state.is_dirty());
        assert!(!state.is_binding_modified(StrategyAction::MoveForward));
        assert_eq!(
            state
                .prefs()
                .bindings()
                .get("move_forward")
                .map(String::as_str),
            Some("Q")
        );
    }

    #[test]
    fn set_staged_binding_marks_dirty() {
        let mut state = SettingsState::from_prefs(Prefs::default());

        state.set_staged_binding(StrategyAction::MoveForward, key(Key::Q));

        assert!(state.is_dirty());
        assert!(state.is_field_dirty(SettingsField::Binding(StrategyAction::MoveForward)));
        assert_eq!(
            state.get_staged_binding(StrategyAction::MoveForward),
            key(Key::Q)
        );
    }

    #[test]
    fn get_staged_binding_returns_correct_value() {
        let mut state = SettingsState::from_prefs(Prefs::default());

        state.set_staged_binding(StrategyAction::MoveLeft, key(Key::Q));

        assert_eq!(
            state.get_staged_binding(StrategyAction::MoveLeft),
            key(Key::Q)
        );
    }

    #[test]
    fn is_binding_modified_detects_changes() {
        let mut state = SettingsState::from_prefs(Prefs::default());
        assert!(!state.is_binding_modified(StrategyAction::MoveForward));

        state.set_staged_binding(StrategyAction::MoveForward, key(Key::Q));

        assert!(state.is_binding_modified(StrategyAction::MoveForward));
        assert!(!state.is_binding_modified(StrategyAction::MoveLeft));
    }

    #[test]
    fn from_prefs_reads_saved_bindings() {
        let mut prefs = Prefs::default();
        prefs.set_bindings(
            [("move_forward".to_string(), "Q".to_string())]
                .into_iter()
                .collect(),
        );

        let state = SettingsState::from_prefs(prefs);

        assert!(!state.is_dirty());
        assert_eq!(
            state.get_staged_binding(StrategyAction::MoveForward),
            key(Key::Q)
        );
        assert!(!state.is_binding_modified(StrategyAction::MoveForward));
    }
}
