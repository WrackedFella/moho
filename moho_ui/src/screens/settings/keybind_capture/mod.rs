//! Keybind capture handler for configuring input bindings.
//!
//! This module contains all keybind configuration logic including:
//! - Key press capture (modifier keys are treated as regular key codes)
//! - Conflict detection and modal triggering
//! - Input state tracking
//!
//! This module will be used by the future dedicated "Keybinds" screen/tab
//! (accessed via "Edit Keybinds" button).

mod modifier_encoding;
mod binding_logic;

use super::SettingsField;
use super::conflict_modal::ConflictModalState;
use super::types::BindingId;
use crate::prefs::Binding;

/// Keybind capture handler for configuring input bindings.
///
/// Handles raw keyboard input during keybind listening, including:
/// - Key press capture with modifier detection
/// - Conflict detection and modal triggering
/// - Modifier-only binding support
/// - Input state tracking
pub struct KeybindCaptureHandler {
    /// Index of the binding currently being listened for (None if not listening)
    pub(super) listening: Option<usize>,

    /// Last known modifier state (for modifier-only bindings)
    pub(super) last_mods: u8,

    /// Conflict modal state (for showing conflict dialog)
    pub(super) conflict_modal: ConflictModalState,
}

impl KeybindCaptureHandler {
    pub fn new() -> Self {
        Self {
            listening: None,
            last_mods: 0,
            conflict_modal: ConflictModalState::new(),
        }
    }

    /// Start listening for a new keybind for the given field
    pub fn start_listening(&mut self, binding_id: usize) {
        self.listening = Some(binding_id);
        self.last_mods = 0;
    }

    // Note: stop_listening is provided for API completeness but currently unused.
    // It may be needed when implementing cancel/reset functionality in the future.
    #[allow(dead_code)]
    pub fn stop_listening(&mut self) {
        self.listening = None;
        self.last_mods = 0;
    }

    /// Check if currently listening for input
    pub fn is_listening(&self) -> bool {
        self.listening.is_some()
    }

    /// Get the binding ID currently being listened for (for UI rendering)
    pub fn listening_id(&self) -> Option<usize> {
        self.listening
    }

    /// Get a reference to the conflict modal state (for testing)
    pub fn conflict_modal(&self) -> &ConflictModalState {
        &self.conflict_modal
    }

    /// Handle a winit WindowEvent when listening for a binding.
    /// Returns true if the event was consumed (binding applied or cancelled).
    ///
    /// # Arguments
    /// * `event` - The winit WindowEvent to process
    /// * `staged_prefs` - The staged preferences to check for conflicts and apply bindings
    /// * `on_binding_changed` - Callback to update staged binding
    pub fn handle_winit_event<F>(
        &mut self,
        event: &winit::event::WindowEvent,
        staged_prefs: &crate::prefs::Prefs,
        on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(SettingsField, Binding),
    {
        use winit::event::{ElementState, WindowEvent as WEvent};

        if let WEvent::KeyboardInput {
            event: key_event, ..
        } = event
        {
            // Only respond to key presses while listening
            if key_event.state != ElementState::Pressed {
                return false;
            }

            // If not listening, ignore
            if self.listening.is_none() {
                return false;
            }

            // Map physical key to binding code (delegated to shared helper)
            let code = moho_input::physical_key_to_binding_code(key_event.physical_key);

            // We don't have reliable modifier state here (winit KeyEvent doesn't expose it
            // in a consistent, portable way), so record no modifier bits for non-modifier
            // keys. We forward the resolved key code to a helper so tests can exercise
            // the post-mapping logic without constructing full winit KeyEvent structs.
            let mods_bits = 0u8;

            return self.apply_key_code_while_listening(
                code,
                mods_bits,
                staged_prefs,
                on_binding_changed,
            );
        }
        false
    }

    /// Check if there's a pending binding to apply (after modal confirmation)
    // Note: has_pending_binding is provided for API completeness but currently unused.
    // The conflict modal visibility is checked directly in most cases.
    #[allow(dead_code)]
    pub fn has_pending_binding(&self) -> bool {
        self.conflict_modal.is_visible()
    }

    /// Apply the pending binding (called after user confirms in modal)
    ///
    /// # Arguments
    /// * `on_binding_changed` - Callback to update staged bindings
    pub fn apply_pending<F>(&mut self, mut on_binding_changed: F)
    where
        F: FnMut(SettingsField, Binding),
    {
        if let Some(pending) = self.conflict_modal.take_pending() {
            // Clear conflicting binding if any
            if let Some(conflict_id) = pending.conflicting_id {
                let field = match conflict_id {
                    0 => SettingsField::KeyW,
                    1 => SettingsField::KeyA,
                    2 => SettingsField::KeyS,
                    3 => SettingsField::KeyD,
                    4 => SettingsField::KeyUp,
                    5 => SettingsField::KeyDown,
                    _ => SettingsField::KeyW, // Fallback (shouldn't happen)
                };
                on_binding_changed(field, Binding::new(0, 0));
            }

            // Apply new binding
            let target_field = match pending.target_id {
                0 => SettingsField::KeyW,
                1 => SettingsField::KeyA,
                2 => SettingsField::KeyS,
                3 => SettingsField::KeyD,
                4 => SettingsField::KeyUp,
                5 => SettingsField::KeyDown,
                _ => SettingsField::KeyW, // Fallback (shouldn't happen)
            };
            on_binding_changed(target_field, pending.binding);
        }
        self.conflict_modal.hide();
    }

    /// Cancel the pending binding (called when user cancels modal)
    pub fn cancel_pending_binding(&mut self) {
        self.conflict_modal.clear();
    }

    /// Get conflict modal state (for checking if modal should be shown)
    pub fn take_conflict_modal(&mut self) -> Option<ConflictModalState> {
        if self.conflict_modal.is_visible() {
            Some(std::mem::take(&mut self.conflict_modal))
        } else {
            None
        }
    }

    pub(super) fn get_key_name(id: usize) -> &'static str {
        BindingId::from_usize(id)
            .map(|bid| bid.display_name())
            .unwrap_or("Unknown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::key_mapping::binding_label;

    #[test]
    fn test_start_stop_listening() {
        let mut handler = KeybindCaptureHandler::new();
        assert!(!handler.is_listening());

        handler.start_listening(0);
        assert!(handler.is_listening());

        handler.stop_listening();
        assert!(!handler.is_listening());
    }

    #[test]
    fn test_escape_cancels_listening() {
        let mut handler = KeybindCaptureHandler::new();
        let prefs = crate::prefs::Prefs::default();
        let mut bindings_changed = vec![];

        handler.start_listening(0);

        // Escape key code is 0x200
        let consumed =
            handler.apply_key_code_while_listening(0x200, 0, &prefs, |field, binding| {
                bindings_changed.push((field, binding))
            });

        assert!(consumed);
        assert!(!handler.is_listening());
        assert!(bindings_changed.is_empty()); // No binding should be applied
    }

    #[test]
    fn test_modifier_only_capture() {
        let mut handler = KeybindCaptureHandler::new();
        let prefs = crate::prefs::Prefs::default();
        let mut bindings_changed = vec![];

        handler.start_listening(0);

        // Alt modifier-only binding (code 0x206, mods 0)
        // Note: Default prefs uses Shift (0x204) for key_down, so use Alt to avoid conflict
        let consumed =
            handler.apply_key_code_while_listening(0x206, 0, &prefs, |field, binding| {
                bindings_changed.push((field, binding))
            });

        assert!(consumed);
        assert!(!handler.is_listening());
        assert_eq!(bindings_changed.len(), 1);

        let (field, binding) = bindings_changed[0];
        assert!(matches!(field, SettingsField::KeyW));
        assert_eq!(binding.code, 0x206);
        assert_eq!(binding.mods, 0);
    }

    #[test]
    fn test_normal_key_capture() {
        let mut handler = KeybindCaptureHandler::new();
        let prefs = crate::prefs::Prefs::default();
        let mut bindings_changed = vec![];

        handler.start_listening(1); // KeyA

        // Press 'W' key with Ctrl modifier
        let w_code = 'W' as u32;
        let ctrl_mods = 1u8;
        let consumed =
            handler.apply_key_code_while_listening(w_code, ctrl_mods, &prefs, |field, binding| {
                bindings_changed.push((field, binding))
            });

        assert!(consumed);
        assert!(!handler.is_listening());
        assert_eq!(bindings_changed.len(), 1);

        let (field, binding) = bindings_changed[0];
        assert!(matches!(field, SettingsField::KeyA));
        assert_eq!(binding.code, w_code);
        assert_eq!(binding.mods, ctrl_mods);
    }

    #[test]
    fn test_conflict_detection() {
        let mut handler = KeybindCaptureHandler::new();

        // Create prefs where KeyW is already bound to 'W'
        let prefs = crate::prefs::Prefs::default().with_key_w(Binding::new('W' as u32, 0));

        let mut bindings_changed = vec![];

        // Try to bind KeyA to the same key 'W'
        handler.start_listening(1); // KeyA

        let consumed =
            handler.apply_key_code_while_listening('W' as u32, 0, &prefs, |field, binding| {
                bindings_changed.push((field, binding))
            });

        assert!(consumed);
        assert!(!handler.is_listening());
        assert!(bindings_changed.is_empty()); // No binding applied yet
        assert!(handler.has_pending_binding()); // Conflict modal should be triggered
    }

    #[test]
    fn test_pending_binding_apply() {
        let mut handler = KeybindCaptureHandler::new();

        // Create prefs where KeyW is already bound to 'W'
        let prefs = crate::prefs::Prefs::default().with_key_w(Binding::new('W' as u32, 0));

        // Try to bind KeyA to 'W' (conflict)
        handler.start_listening(1); // KeyA
        handler.apply_key_code_while_listening('W' as u32, 0, &prefs, |_, _| {});

        assert!(handler.has_pending_binding());

        // Apply pending binding
        let mut bindings_changed = vec![];
        handler.apply_pending(|field, binding| bindings_changed.push((field, binding)));

        // Should clear KeyW and apply KeyA
        assert_eq!(bindings_changed.len(), 2);

        // First change: clear KeyW
        let (field1, binding1) = bindings_changed[0];
        assert!(matches!(field1, SettingsField::KeyW));
        assert_eq!(binding1.code, 0);
        assert_eq!(binding1.mods, 0);

        // Second change: apply KeyA
        let (field2, binding2) = bindings_changed[1];
        assert!(matches!(field2, SettingsField::KeyA));
        assert_eq!(binding2.code, 'W' as u32);
        assert_eq!(binding2.mods, 0);
    }

    #[test]
    fn test_binding_label_formatting() {
        // Unbound
        let unbound = Binding::new(0, 0);
        assert_eq!(binding_label(&unbound), "Unbound");

        // Simple key
        let w_key = Binding::new('W' as u32, 0);
        assert_eq!(binding_label(&w_key), "W");

        // Key with Ctrl
        let ctrl_w = Binding::new('W' as u32, 1);
        assert_eq!(binding_label(&ctrl_w), "Ctrl+W");

        // Key with multiple modifiers
        let ctrl_shift_w = Binding::new('W' as u32, 3); // Ctrl(1) | Shift(2)
        assert_eq!(binding_label(&ctrl_shift_w), "Ctrl+Shift+W");

        // Modifier-only (Shift alone)
        let shift_only = Binding::new(0x204, 0);
        assert_eq!(binding_label(&shift_only), "Shift");

        // Arrow key
        let arrow_up = Binding::new(0x100, 0);
        assert_eq!(binding_label(&arrow_up), "ArrowUp");

        // Space
        let space = Binding::new(' ' as u32, 0);
        assert_eq!(binding_label(&space), "Spacebar");
    }
}
