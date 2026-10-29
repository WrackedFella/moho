//! State management for settings menu.
//!
//! This module encapsulates dirty tracking, staged changes, and conflict resolution
//! state, providing a clean API for state mutations and queries.

use super::SettingsField;
use super::binding_registry::BindingRegistry;
use crate::actions::StrategyAction;
use crate::prefs::{Prefs, WindowMode};
use moho_input::bindings::{ActionMode, Binding};
use std::collections::HashSet;
use std::path::PathBuf;

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
    /// Where `apply_changes` writes the preferences
    path: PathBuf,
}

impl SettingsState {
    /// Create a settings state from existing preferences, saved to the
    /// default config path.
    pub fn from_prefs(prefs: Prefs) -> Self {
        Self::from_prefs_at(prefs, Prefs::config_path())
    }

    /// Create a settings state from existing preferences that `apply_changes`
    /// saves to `path`.
    pub(super) fn from_prefs_at(prefs: Prefs, path: PathBuf) -> Self {
        let bindings = BindingRegistry::from_prefs(&prefs);
        Self {
            prefs: prefs.clone(),
            staged: prefs,
            saved_bindings: bindings.clone(),
            staged_bindings: bindings,
            dirty_fields: HashSet::new(),
            path,
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
        self.staged.save_to(&self.path)?;
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

    pub fn get_staged_mode(&self, action: StrategyAction) -> ActionMode {
        self.staged_bindings.get_mode(action)
    }

    /// Replace the staged mode of one action and mark its row dirty.
    pub fn set_staged_mode(&mut self, action: StrategyAction, mode: ActionMode) {
        self.staged_bindings.set_mode(action, mode);
        self.mark_dirty(SettingsField::Binding(action));
    }

    /// Stage a window mode and mark it dirty.
    pub fn set_window_mode(&mut self, mode: WindowMode) {
        self.staged.set_window_mode(mode);
        self.mark_dirty(SettingsField::WindowMode);
    }

    /// Stage a window resolution and mark it dirty.
    pub fn set_window_resolution(&mut self, width: u32, height: u32) {
        self.staged.set_window_resolution(width, height);
        self.mark_dirty(SettingsField::WindowResolution);
    }

    /// Stage a Brightness (clamped by the prefs) and mark it dirty.
    pub fn set_brightness(&mut self, brightness: f32) {
        self.staged.set_brightness(brightness);
        self.mark_dirty(SettingsField::Brightness);
    }

    /// Stage a Bloom level (0=Off, 1=Low, 2=High) and mark it dirty.
    pub fn set_bloom(&mut self, quality: u32) {
        self.staged.set_bloom(quality);
        self.mark_dirty(SettingsField::Bloom);
    }

    /// Stage a shadow quality level (0=Off .. 4=Ultra) and mark it dirty.
    pub fn set_shadow_quality(&mut self, quality: u32) {
        self.staged.set_shadow_quality(quality);
        self.mark_dirty(SettingsField::ShadowQuality);
    }

    /// Whether the staged bindings or mode of `action` differ from the saved ones.
    pub fn is_binding_modified(&self, action: StrategyAction) -> bool {
        self.staged_bindings.get_binding(action) != self.saved_bindings.get_binding(action)
            || self.staged_bindings.get_mode(action) != self.saved_bindings.get_mode(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_input::key::Key;

    fn key(k: Key) -> Vec<Binding> {
        vec![Binding::key(k)]
    }

    #[test]
    fn set_staged_binding_marks_only_that_field_dirty() {
        let mut state = SettingsState::from_prefs(Prefs::default());

        state.set_staged_binding(StrategyAction::MoveForward, key(Key::Q));

        assert!(state.is_dirty());
        assert!(state.is_field_dirty(SettingsField::Binding(StrategyAction::MoveForward)));
        assert!(!state.is_field_dirty(SettingsField::Binding(StrategyAction::MoveLeft)));
        assert_eq!(
            state.get_staged_binding(StrategyAction::MoveForward),
            key(Key::Q)
        );
    }

    #[test]
    fn apply_changes_updates_saved_prefs() {
        use crate::prefs::WindowMode;
        let dir = tempfile::tempdir().unwrap();
        let mut state =
            SettingsState::from_prefs_at(Prefs::default(), dir.path().join("prefs.ini"));
        state.set_window_mode(WindowMode::Fullscreen);

        let result = state.apply_changes();

        assert!(result.is_ok());
        assert_eq!(state.prefs().window_mode(), WindowMode::Fullscreen);
    }

    #[test]
    fn saving_settings_keeps_debug_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.ini");
        let mut prefs = Prefs::default();
        prefs.set_debug_value("debug_overlay", "on").unwrap();
        let mut state = SettingsState::from_prefs_at(prefs, path.clone());
        *state.staged_mut().music_volume_mut() = 2.5;

        state.apply_changes().unwrap();
        let (reloaded, warnings) = Prefs::load_from(&path);

        assert_eq!(warnings, vec![]);
        assert_eq!(reloaded.music_volume(), 2.5);
        assert_eq!(reloaded.debug_value("debug_overlay"), Some("on"));
    }

    #[test]
    fn video_setting_change_marks_dirty_and_reverts() {
        use crate::prefs::WindowMode;
        let mut state = SettingsState::from_prefs(Prefs::default());
        assert!(!state.is_dirty());

        state.set_window_mode(WindowMode::Fullscreen);

        assert!(state.is_field_dirty(SettingsField::WindowMode));
        assert!(!state.is_field_dirty(SettingsField::WindowResolution));
        assert_eq!(state.staged().window_mode(), WindowMode::Fullscreen);

        state.set_window_resolution(2560, 1440);

        assert!(state.is_field_dirty(SettingsField::WindowResolution));
        assert_eq!(state.staged().window_resolution(), (2560, 1440));

        state.revert_changes();

        assert!(!state.is_dirty());
        assert_eq!(state.staged().window_mode(), WindowMode::Windowed);
        assert_eq!(state.staged().window_resolution(), (1920, 1080));
    }

    #[test]
    fn staging_brightness_marks_the_tab_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let mut state =
            SettingsState::from_prefs_at(Prefs::default(), dir.path().join("prefs.ini"));
        assert!(!state.is_dirty());

        state.set_brightness(1.5);

        assert!(state.is_dirty());
        assert!(state.is_field_dirty(SettingsField::Brightness));
        assert!(!state.is_field_dirty(SettingsField::Vsync));
        assert_eq!(state.staged().brightness(), 1.5);
        assert_eq!(state.prefs().brightness(), 1.0);

        state.revert_changes();

        assert!(!state.is_dirty());
        assert_eq!(state.staged().brightness(), 1.0);

        state.set_brightness(0.75);
        state.apply_changes().unwrap();

        assert!(!state.is_dirty());
        assert_eq!(state.prefs().brightness(), 0.75);
    }

    #[test]
    fn staging_bloom_marks_the_tab_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let mut state =
            SettingsState::from_prefs_at(Prefs::default(), dir.path().join("prefs.ini"));
        assert!(!state.is_dirty());

        state.set_bloom(0);

        assert!(state.is_dirty());
        assert!(state.is_field_dirty(SettingsField::Bloom));
        assert!(!state.is_field_dirty(SettingsField::Brightness));
        assert_eq!(state.staged().bloom(), 0);
        assert_eq!(state.prefs().bloom(), 2);

        state.revert_changes();

        assert!(!state.is_dirty());
        assert_eq!(state.staged().bloom(), 2);

        state.set_bloom(1);
        state.apply_changes().unwrap();

        assert!(!state.is_dirty());
        assert_eq!(state.prefs().bloom(), 1);
    }

    #[test]
    fn staging_shadow_quality_marks_the_tab_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let mut state =
            SettingsState::from_prefs_at(Prefs::default(), dir.path().join("prefs.ini"));
        assert!(!state.is_dirty());

        state.set_shadow_quality(0);

        assert!(state.is_dirty());
        assert!(state.is_field_dirty(SettingsField::ShadowQuality));
        assert!(!state.is_field_dirty(SettingsField::Bloom));
        assert_eq!(state.staged().shadow_quality(), 0);
        assert_eq!(state.prefs().shadow_quality(), 3);

        state.revert_changes();

        assert!(!state.is_dirty());
        assert_eq!(state.staged().shadow_quality(), 3);

        state.set_shadow_quality(1);
        state.apply_changes().unwrap();

        assert!(!state.is_dirty());
        assert_eq!(state.prefs().shadow_quality(), 1);
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
    fn mode_change_marks_the_row_dirty_and_reverts() {
        let mut state = SettingsState::from_prefs(Prefs::default());
        assert_eq!(
            state.get_staged_mode(StrategyAction::Sprint),
            ActionMode::Hold
        );

        state.set_staged_mode(StrategyAction::Sprint, ActionMode::Toggle);

        assert_eq!(
            state.get_staged_mode(StrategyAction::Sprint),
            ActionMode::Toggle
        );
        assert!(state.is_dirty());
        assert!(state.is_field_dirty(SettingsField::Binding(StrategyAction::Sprint)));
        assert!(!state.is_field_dirty(SettingsField::Binding(StrategyAction::MoveForward)));
        assert!(state.is_binding_modified(StrategyAction::Sprint));
        assert!(!state.is_binding_modified(StrategyAction::MoveForward));

        state.revert_changes();

        assert!(!state.is_dirty());
        assert_eq!(
            state.get_staged_mode(StrategyAction::Sprint),
            ActionMode::Hold
        );
        assert!(!state.is_binding_modified(StrategyAction::Sprint));
    }

    #[test]
    fn setting_the_mode_back_to_the_saved_one_is_not_modified() {
        let mut state = SettingsState::from_prefs(Prefs::default());

        state.set_staged_mode(StrategyAction::Sprint, ActionMode::Toggle);
        state.set_staged_mode(StrategyAction::Sprint, ActionMode::Hold);

        assert!(!state.is_binding_modified(StrategyAction::Sprint));
    }

    #[test]
    fn saved_modes_are_read_from_prefs() {
        let (prefs, issues) = Prefs::parse("[action_modes]\nsprint = toggle\n");
        assert_eq!(issues, vec![]);

        let state = SettingsState::from_prefs(prefs);

        assert_eq!(
            state.get_staged_mode(StrategyAction::Sprint),
            ActionMode::Toggle
        );
        assert!(!state.is_binding_modified(StrategyAction::Sprint));
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
