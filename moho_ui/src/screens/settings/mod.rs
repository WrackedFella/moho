mod audio_tab;
mod binding_registry;
mod conflict_modal;
mod controls_tab;
mod key_mapping;
mod keybind_capture;
mod render_ops;
mod state;
mod types;

use conflict_modal::ConflictModalState;
use keybind_capture::KeybindCaptureHandler;
use state::SettingsState;
pub use types::SettingsTab;

use super::{FormControls, MenuAction, Screen, ScreenSpec, UiComponent};
use crate::prefs::Binding;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub(super) enum SettingsField {
    KeyW,
    KeyA,
    KeyS,
    KeyD,
    KeyUp,
    KeyDown,
    MouseSensitivity,
    InputFiltering,
    AudioSoundEffect,
    AudioMusic,
    AudioUI,
    AudioVoice,
}

pub struct SettingsMenu {
    spec: ScreenSpec,
    active_tab: SettingsTab,
    state: SettingsState,
    keybind_capture: KeybindCaptureHandler,
}

impl SettingsMenu {
    pub fn new() -> Self {
        Self {
            spec: ScreenSpec::default(),
            active_tab: SettingsTab::default(),
            state: SettingsState::new(),
            keybind_capture: KeybindCaptureHandler::new(),
        }
    }

    /// Create a SettingsMenu with specific preferences (for testing).
    ///
    /// This constructor allows tests to use isolated prefs instances without
    /// reading from or writing to the shared config/prefs.ini file.
    ///
    /// # Test Isolation
    ///
    /// Use this in tests instead of `new()` to ensure each test starts with a
    /// known, clean state. Even if tests call `apply_staged_changes()` (which
    /// saves to disk), other tests using `with_prefs()` won't be affected because
    /// they initialize with their own Prefs instance.
    ///
    /// # Example
    ///
    /// ```rust
    /// use moho_ui::screens::SettingsMenu;
    /// use moho_ui::prefs::Prefs;
    ///
    /// // Create menu with default prefs (no disk I/O)
    /// let menu = SettingsMenu::with_prefs(Prefs::default());
    ///
    /// // Or create with custom prefs for specific test scenarios
    /// let mut custom_prefs = Prefs::default();
    /// custom_prefs.mouse_sensitivity = 2.5;
    /// let menu = SettingsMenu::with_prefs(custom_prefs);
    /// ```
    ///
    /// # Arguments
    /// * `prefs` - The preferences to initialize with
    ///
    /// # Returns
    /// A new SettingsMenu with the given preferences
    pub fn with_prefs(prefs: crate::prefs::Prefs) -> Self {
        Self {
            spec: ScreenSpec::default(),
            active_tab: SettingsTab::default(),
            state: SettingsState::from_prefs(prefs),
            keybind_capture: KeybindCaptureHandler::new(),
        }
    }

    /// Return true when the menu is currently listening for a new binding.
    pub fn is_listening(&self) -> bool {
        self.keybind_capture.is_listening()
    }

    /// Return true if listening for a specific binding ID.
    /// For rendering purposes - allows UI to show "Listening..." state on specific controls.
    pub(super) fn is_listening_for(&self, binding_id: usize) -> bool {
        self.keybind_capture.is_listening()
            && self.keybind_capture.listening_id() == Some(binding_id)
    }

    /// Get a reference to the conflict modal state (for testing).
    pub fn conflict_modal(&self) -> &ConflictModalState {
        self.keybind_capture.conflict_modal()
    }

    pub fn apply_pending_binding(&mut self) {
        self.keybind_capture.apply_pending(|field, binding| {
            self.state.set_staged_binding(field, binding);
        });
    }

    pub fn cancel_pending_binding(&mut self) {
        self.keybind_capture.cancel_pending_binding();
    }

    fn is_dirty(&self) -> bool {
        self.state.is_dirty()
    }

    pub(super) fn paint_dirty_decor(ui: &mut egui::Ui, resp: &egui::Response, dirty: bool) {
        if dirty || resp.hovered() {
            let r = resp.rect;
            let hover_color = egui::Color32::from_rgba_premultiplied(150, 150, 150, 60);
            ui.painter().rect_filled(r, 4.0, hover_color);
        }
    }

    // ========== Test Helper Methods ==========
    // These methods are exposed for testing to allow direct manipulation
    // of settings menu state without requiring full UI interaction.
    // They are public to support integration tests but should not be used
    // in production code.

    /// Start listening for a key binding on the specified binding ID.
    /// For testing purposes only - allows tests to simulate user clicking "Listen" button.
    pub fn start_listening(&mut self, binding_id: usize) {
        self.keybind_capture.start_listening(binding_id);
    }

    /// Testable helper: apply a resolved key code while the menu is listening.
    ///
    /// This function contains the core logic for applying a binding or queuing a
    /// conflict modal when `listening` is active. It's public so unit tests
    /// can exercise the behavior without depending on winit event construction.
    ///
    /// For testing purposes: allows direct simulation of key input during binding listen mode.
    pub fn apply_key_code_while_listening(&mut self, code: u32, mods_bits: u8) -> bool {
        let staged_prefs = self.state.staged().clone();
        self.keybind_capture.apply_key_code_while_listening(
            code,
            mods_bits,
            &staged_prefs,
            |field, binding| {
                self.state.set_staged_binding(field, binding);
            },
        )
    }

    /// Get the currently active tab.
    /// For testing purposes only - allows tests to verify tab state.
    pub fn active_tab(&self) -> SettingsTab {
        self.active_tab
    }

    /// Set the active tab directly.
    /// For testing purposes only - allows tests to simulate tab switching.
    pub fn set_active_tab(&mut self, tab: SettingsTab) {
        self.active_tab = tab;
    }

    /// Check if there are any unsaved changes.
    /// For testing purposes only - wraps the private is_dirty() method.
    pub fn has_unsaved_changes(&self) -> bool {
        self.is_dirty()
    }

    /// Get the staged binding for a specific binding ID.
    /// For testing purposes only - allows tests to verify binding state.
    pub fn get_staged_binding(&self, binding_id: usize) -> Binding {
        let field = match binding_id {
            0 => SettingsField::KeyW,
            1 => SettingsField::KeyA,
            2 => SettingsField::KeyS,
            3 => SettingsField::KeyD,
            4 => SettingsField::KeyUp,
            5 => SettingsField::KeyDown,
            _ => return Binding::new(0, 0),
        };
        self.state.get_staged_binding(field)
    }

    /// Apply and save staged changes.
    /// For testing purposes only - allows tests to simulate clicking "Save Changes".
    pub fn apply_staged_changes(&mut self) {
        let _ = self.state.apply_changes();
        let _ = self.state.prefs().save();
    }

    /// Revert staged changes to last saved state.
    /// For testing purposes only - allows tests to simulate clicking "Cancel".
    pub fn revert_staged_changes(&mut self) {
        self.state.revert_changes();
    }

    /// Confirm a pending binding that triggered a conflict modal.
    /// For testing purposes only - allows tests to simulate clicking "Confirm" on conflict modal.
    pub fn confirm_pending_binding(&mut self) {
        self.apply_pending_binding();
    }
}

impl Default for SettingsMenu {
    fn default() -> Self {
        Self::new()
    }
}

// Implement UiComponent (base trait)
impl UiComponent for SettingsMenu {
    fn name(&self) -> &str {
        "settings"
    }

    fn render(&mut self, ctx: &egui::Context) -> Vec<super::MenuItem> {
        let mut items: Vec<super::MenuItem> = Vec::new();

        // Top panel for title and tab bar - delegates to render_ops
        self.active_tab = render_ops::render_top_panel(ctx, self.active_tab);

        // Bottom panel for buttons - delegates to render_ops
        items.extend(render_ops::render_bottom_panel(ctx, self));

        // Central panel contains only the scrollable form content - delegates to render_ops
        render_ops::render_content_area(ctx, self);

        // Handle key capture when listening for a binding - delegates to keybind_capture
        let staged_prefs = self.state.staged().clone();
        self.keybind_capture
            .handle_key_capture(ctx, &staged_prefs, |field, binding| {
                self.state.set_staged_binding(field, binding);
            });

        items
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// Implement Screen (specialized trait) with capability pattern
impl Screen for SettingsMenu {
    fn spec(&self) -> &ScreenSpec {
        &self.spec
    }

    /// Settings menu captures raw input when listening for keybinds
    fn captures_raw_input(&self) -> bool {
        self.is_listening()
    }

    /// Handle raw input for keybind capture
    fn handle_raw_input(&mut self, event: &winit::event::WindowEvent) -> bool {
        let staged_prefs = self.state.staged().clone();
        self.keybind_capture
            .handle_winit_event(event, &staged_prefs, |field, binding| {
                self.state.set_staged_binding(field, binding);
            })
    }

    /// Check if settings wants to show the keybind conflict modal
    fn take_pending_modal(&mut self) -> Option<Box<dyn crate::modal::Modal>> {
        self.keybind_capture
            .take_conflict_modal()
            .map(|conflict_modal| {
                use crate::modals::KeybindConflictModal;
                Box::new(KeybindConflictModal::new(
                    conflict_modal.conflict_key_name().to_string(),
                    conflict_modal.conflict_binding_desc().to_string(),
                )) as Box<dyn crate::modal::Modal>
            })
    }

    /// Apply pending keybind when modal is confirmed
    fn on_modal_confirm(&mut self) {
        self.apply_pending_binding();
    }

    /// Cancel pending keybind when modal is cancelled
    fn on_modal_cancel(&mut self) {
        self.keybind_capture.cancel_pending_binding();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_only_capture_applies() {
        let mut menu = SettingsMenu::new();
        menu.keybind_capture.start_listening(0);

        let staged_prefs = menu.state.staged().clone();
        let applied = menu.keybind_capture.capture_modifier_if_listening(
            1,
            &staged_prefs,
            |field, binding| {
                menu.state.set_staged_binding(field, binding);
            },
        );

        assert!(applied, "modifier capture should apply");
        assert!(
            !menu.keybind_capture.is_listening(),
            "should stop listening after capture"
        );
        assert_eq!(
            menu.state.get_staged_binding(SettingsField::KeyW),
            Binding::new(0x205, 0)
        );
    }

    #[test]
    fn modifier_only_conflict_shows_modal() {
        let mut menu = SettingsMenu::new();
        // set staged key_a to Ctrl so Ctrl will conflict with listening target 0
        menu.state
            .set_staged_binding(SettingsField::KeyA, Binding::new(0x205, 0));
        menu.keybind_capture.start_listening(0);

        let staged_prefs = menu.state.staged().clone();
        let applied = menu.keybind_capture.capture_modifier_if_listening(
            1,
            &staged_prefs,
            |field, binding| {
                menu.state.set_staged_binding(field, binding);
            },
        );

        assert!(applied, "modifier conflict should be processed");
        assert!(
            menu.keybind_capture.conflict_modal().is_visible(),
            "conflict modal should be visible when conflict is detected"
        );
        // Verify the modal has the correct conflict info
        assert_eq!(
            menu.keybind_capture.conflict_modal().conflict_key_name(),
            "Move Left"
        );
        assert_eq!(
            menu.keybind_capture
                .conflict_modal()
                .conflict_binding_desc(),
            "Ctrl"
        );
    }

    #[test]
    fn modifier_multiple_mods_ignored() {
        let mut menu = SettingsMenu::new();
        menu.keybind_capture.start_listening(0);

        let staged_prefs = menu.state.staged().clone();
        // ctrl+shift (bits 1 and 2) should not create a modifier-only binding
        let applied = menu.keybind_capture.capture_modifier_if_listening(
            3,
            &staged_prefs,
            |field, binding| {
                menu.state.set_staged_binding(field, binding);
            },
        );

        assert!(!applied, "combined modifiers should not be captured");
        assert!(
            menu.keybind_capture.is_listening(),
            "still listening after ignored multi-modifier"
        );
    }

    #[test]
    fn egui_integration_modifier_capture() {
        let ctx = egui::Context::default();
        let mut menu = SettingsMenu::new();
        menu.keybind_capture.start_listening(0);

        let mut raw = egui::RawInput::default();
        raw.modifiers.ctrl = true;

        let _full = ctx.run(raw, |ctx| {
            menu.render(ctx);
        });

        assert!(menu.state.get_staged_binding(SettingsField::KeyW) == Binding::new(0x205, 0));
        assert!(!menu.keybind_capture.is_listening());
    }

    // Note: Scroll and low-level Key event simulation in integration tests
    // depends on the egui version's RawInput/Event API. We keep focused
    // integration coverage on modifiers here; other cases are covered by
    // unit tests and adapter-level behavior.
}
