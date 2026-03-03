//! State management for settings menu.
//!
//! This module encapsulates dirty tracking, staged changes, and conflict resolution
//! state, providing a clean API for state mutations and queries.

use super::SettingsField;
use crate::prefs::{Binding, Prefs};
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
    /// Fields that have been modified (differ from prefs)
    dirty_fields: HashSet<SettingsField>,
}

impl SettingsState {
    /// Create a new settings state by loading preferences from disk.
    ///
    /// # Returns
    /// A new SettingsState with prefs loaded and no dirty fields.
    pub fn new() -> Self {
        let prefs = Prefs::load();
        Self {
            prefs: prefs.clone(),
            staged: prefs,
            dirty_fields: HashSet::new(),
        }
    }

    /// Create a settings state from existing preferences.
    ///
    /// # Arguments
    /// * `prefs` - The preferences to initialize with
    ///
    /// # Returns
    /// A new SettingsState with the given prefs and no dirty fields.
    #[allow(dead_code)]
    pub fn from_prefs(prefs: Prefs) -> Self {
        Self {
            prefs: prefs.clone(),
            staged: prefs,
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
        self.staged.save()?;
        self.prefs = self.staged.clone();
        self.dirty_fields.clear();
        Ok(())
    }

    /// Revert staged changes back to the last saved state.
    ///
    /// Discards all uncommitted changes and clears dirty flags.
    pub fn revert_changes(&mut self) {
        self.staged = self.prefs.clone();
        self.dirty_fields.clear();
    }

    /// Get a specific binding from staged preferences by field.
    ///
    /// # Arguments
    /// * `field` - The binding field to retrieve
    ///
    /// # Returns
    /// The current staged binding for the specified field
    pub fn get_staged_binding(&self, field: SettingsField) -> Binding {
        match field {
            SettingsField::KeyW => self.staged.key_w(),
            SettingsField::KeyA => self.staged.key_a(),
            SettingsField::KeyS => self.staged.key_s(),
            SettingsField::KeyD => self.staged.key_d(),
            SettingsField::KeyUp => self.staged.key_up(),
            SettingsField::KeyDown => self.staged.key_down(),
            _ => Binding::new(0, 0), // Non-binding fields return unbound
        }
    }

    /// Set a specific binding in staged preferences by field.
    ///
    /// # Arguments
    /// * `field` - The binding field to update
    /// * `binding` - The new binding value
    pub fn set_staged_binding(&mut self, field: SettingsField, binding: Binding) {
        match field {
            SettingsField::KeyW => self.staged.set_key_w(binding),
            SettingsField::KeyA => self.staged.set_key_a(binding),
            SettingsField::KeyS => self.staged.set_key_s(binding),
            SettingsField::KeyD => self.staged.set_key_d(binding),
            SettingsField::KeyUp => self.staged.set_key_up(binding),
            SettingsField::KeyDown => self.staged.set_key_down(binding),
            _ => {} // Non-binding fields are no-op
        }
        self.mark_dirty(field);
    }

    /// Check if a specific binding field differs from saved state.
    ///
    /// # Arguments
    /// * `field` - The field to compare
    ///
    /// # Returns
    /// true if the staged value differs from saved value
    pub fn is_binding_modified(&self, field: SettingsField) -> bool {
        let staged = self.get_staged_binding(field);
        let saved = match field {
            SettingsField::KeyW => self.prefs.key_w(),
            SettingsField::KeyA => self.prefs.key_a(),
            SettingsField::KeyS => self.prefs.key_s(),
            SettingsField::KeyD => self.prefs.key_d(),
            SettingsField::KeyUp => self.prefs.key_up(),
            SettingsField::KeyDown => self.prefs.key_down(),
            _ => Binding::new(0, 0),
        };
        staged != saved
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

    #[test]
    fn new_state_has_no_dirty_fields() {
        let state = SettingsState::new();
        assert!(!state.is_dirty());
    }

    #[test]
    fn mark_dirty_sets_dirty_flag() {
        let mut state = SettingsState::new();
        assert!(!state.is_dirty());

        state.mark_dirty(SettingsField::KeyW);
        assert!(state.is_dirty());
        assert!(state.is_field_dirty(SettingsField::KeyW));
        assert!(!state.is_field_dirty(SettingsField::KeyA));
    }

    #[test]
    fn revert_changes_clears_dirty_fields() {
        let mut state = SettingsState::new();

        // Modify staged prefs
        state.staged_mut().set_key_w(Binding::new('Q' as u32, 0));
        state.mark_dirty(SettingsField::KeyW);

        assert!(state.is_dirty());

        // Revert changes
        state.revert_changes();

        assert!(!state.is_dirty());
        assert_eq!(state.staged().key_w(), state.prefs().key_w());
    }

    #[test]
    fn apply_changes_clears_dirty_fields() {
        let mut state = SettingsState::new();

        // Modify staged prefs
        state.staged_mut().set_key_w(Binding::new('Q' as u32, 0));
        state.mark_dirty(SettingsField::KeyW);

        assert!(state.is_dirty());

        // Apply changes (saves to disk)
        let result = state.apply_changes();
        assert!(result.is_ok());

        assert!(!state.is_dirty());
        assert_eq!(state.prefs().key_w(), state.staged().key_w());
    }

    #[test]
    fn set_staged_binding_marks_dirty() {
        let mut state = SettingsState::new();

        let new_binding = Binding::new('Q' as u32, 1);
        state.set_staged_binding(SettingsField::KeyW, new_binding);

        assert!(state.is_dirty());
        assert!(state.is_field_dirty(SettingsField::KeyW));
        assert_eq!(state.staged().key_w(), new_binding);
    }

    #[test]
    fn get_staged_binding_returns_correct_value() {
        let mut state = SettingsState::new();

        let new_binding = Binding::new('Q' as u32, 1);
        state.set_staged_binding(SettingsField::KeyA, new_binding);

        assert_eq!(state.get_staged_binding(SettingsField::KeyA), new_binding);
    }

    #[test]
    fn is_binding_modified_detects_changes() {
        let prefs = Prefs::default();
        let mut state = SettingsState::from_prefs(prefs);

        // Initially not modified
        assert!(!state.is_binding_modified(SettingsField::KeyW));

        // Modify the binding to something different from default (W = 87, Q = 81)
        state.set_staged_binding(SettingsField::KeyW, Binding::new('Q' as u32, 0));

        // Now it's modified
        assert!(state.is_binding_modified(SettingsField::KeyW));
        assert!(!state.is_binding_modified(SettingsField::KeyA)); // Other fields unchanged
    }

    #[test]
    fn from_prefs_creates_clean_state() {
        let prefs = Prefs::default().with_key_w(Binding::new('Q' as u32, 0));

        let state = SettingsState::from_prefs(prefs.clone());

        assert!(!state.is_dirty());
        assert_eq!(state.prefs().key_w(), prefs.key_w());
        assert_eq!(state.staged().key_w(), prefs.key_w());
    }

    #[test]
    fn non_binding_fields_handled_gracefully() {
        let mut state = SettingsState::new();

        // Setting a non-binding field should not panic
        state.set_staged_binding(SettingsField::MouseSensitivity, Binding::new(0, 0));

        // Getting a non-binding field returns unbound
        let binding = state.get_staged_binding(SettingsField::AudioMusic);
        assert_eq!(binding, Binding::new(0, 0));
    }
}
