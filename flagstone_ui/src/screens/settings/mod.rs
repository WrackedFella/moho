mod audio_tab;
mod binding_registry;
mod conflict_modal;
mod controls_tab;
mod key_mapping;
mod keybind_capture;
mod render_ops;
mod state;
mod types;
mod video_tab;

use binding_registry::BindingRegistry;
use conflict_modal::ConflictModalState;
use keybind_capture::KeybindCaptureHandler;
use moho_input::bindings::Binding;
use moho_input::key::Key;
use state::SettingsState;
pub use types::SettingsTab;

use super::{FormControls, MenuAction, Screen, ScreenSpec, UiComponent};
use crate::actions::StrategyAction;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub(super) enum SettingsField {
    Binding(StrategyAction),
    MouseSensitivity,
    InvertMouseY,
    MouseSmoothing,
    MouseAcceleration,
    MouseAccelerationStrength,
    AudioSoundEffect,
    AudioMusic,
    AudioUI,
    AudioVoice,
    WindowMode,
    WindowResolution,
    Vsync,
    FpsMax,
    Brightness,
    Bloom,
    ShadowQuality,
}

pub struct SettingsMenu {
    spec: ScreenSpec,
    active_tab: SettingsTab,
    state: SettingsState,
    keybind_capture: KeybindCaptureHandler,
}

impl SettingsMenu {
    /// Create a SettingsMenu from the app's loaded preferences.
    ///
    /// Saving (`apply_staged_changes` or the Save button) writes to
    /// `Prefs::config_path()`. Use `with_prefs_at` to save elsewhere.
    ///
    /// # Example
    ///
    /// ```rust
    /// use flagstone_ui::screens::SettingsMenu;
    /// use flagstone_ui::prefs::Prefs;
    ///
    /// let menu = SettingsMenu::with_prefs(Prefs::default());
    /// ```
    pub fn with_prefs(prefs: crate::prefs::Prefs) -> Self {
        Self {
            spec: ScreenSpec::default(),
            active_tab: SettingsTab::default(),
            state: SettingsState::from_prefs(prefs),
            keybind_capture: KeybindCaptureHandler::new(),
        }
    }

    /// Create a SettingsMenu whose saves are written to `path`.
    pub fn with_prefs_at(prefs: crate::prefs::Prefs, path: std::path::PathBuf) -> Self {
        Self {
            spec: ScreenSpec::default(),
            active_tab: SettingsTab::default(),
            state: SettingsState::from_prefs_at(prefs, path),
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
        self.capture(|handler, staged, on_change| handler.apply_pending(staged, on_change));
    }

    /// Runs `run` on the capture handler with a snapshot of the staged bindings, and a
    /// callback that writes changes back to the staged state. The snapshot is a clone
    /// because the callback mutates the registry it is read from.
    fn capture<R>(
        &mut self,
        run: impl FnOnce(
            &mut KeybindCaptureHandler,
            &BindingRegistry,
            &mut dyn FnMut(StrategyAction, Vec<Binding>),
        ) -> R,
    ) -> R {
        let staged = self.state.staged_bindings().clone();
        run(
            &mut self.keybind_capture,
            &staged,
            &mut |action, bindings| {
                self.state.set_staged_binding(action, bindings);
            },
        )
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

    /// Testable helper: apply a resolved key while the menu is listening.
    ///
    /// This function contains the core logic for applying a binding or queuing a
    /// conflict modal when `listening` is active. It's public so unit tests
    /// can exercise the behavior without depending on winit event construction.
    ///
    /// For testing purposes: allows direct simulation of key input during binding listen mode.
    pub fn apply_key_while_listening(&mut self, key: Key) -> bool {
        self.capture(|handler, staged, on_change| {
            handler.apply_key_while_listening(key, staged, on_change)
        })
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

    /// Get the staged bindings for a specific binding ID.
    /// For testing purposes only - allows tests to verify binding state.
    /// Empty for an unbound action or an unknown ID.
    pub fn get_staged_binding(&self, binding_id: usize) -> &[Binding] {
        types::row_action(binding_id).map_or(&[], |action| self.state.get_staged_binding(action))
    }

    /// Apply and save staged changes to the menu's save path.
    /// For testing purposes only - allows tests to simulate clicking "Save Changes".
    ///
    /// # Errors
    /// Returns the I/O error if the preferences cannot be written.
    pub fn apply_staged_changes(&mut self) -> std::io::Result<()> {
        self.state.apply_changes()
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

// Implement UiComponent (base trait)
impl UiComponent for SettingsMenu {
    fn name(&self) -> &'static str {
        "settings"
    }

    fn render(&mut self, ui: &mut egui::Ui) -> Vec<super::MenuItem> {
        let mut items: Vec<super::MenuItem> = Vec::new();

        // Top panel for title and tab bar - delegates to render_ops
        self.active_tab = render_ops::render_top_panel(ui, self.active_tab);

        // Bottom panel for buttons - delegates to render_ops
        items.extend(render_ops::render_bottom_panel(ui, self));

        // Central panel contains only the scrollable form content - delegates to render_ops
        render_ops::render_content_area(ui, self);

        // Handle key capture when listening for a binding - delegates to keybind_capture
        if self.is_listening() {
            self.capture(|handler, staged, on_change| {
                handler.handle_key_capture(ui.ctx(), staged, on_change);
            });
        }

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
        self.is_listening()
            && self.capture(|handler, staged, on_change| {
                handler.handle_winit_event(event, staged, on_change)
            })
    }

    /// Check if settings wants to show the keybind conflict modal
    fn take_pending_modal(&mut self) -> Option<Box<dyn crate::modal::Modal>> {
        self.keybind_capture
            .take_conflict_modal()
            .map(|(key_name, binding_desc)| {
                use crate::modals::KeybindConflictModal;
                Box::new(KeybindConflictModal::new(key_name, binding_desc))
                    as Box<dyn crate::modal::Modal>
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
