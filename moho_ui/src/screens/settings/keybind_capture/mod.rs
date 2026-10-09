//! Keybind capture handler for configuring input bindings.
//!
//! This module contains all keybind configuration logic including:
//! - Key press capture (modifier keys are treated as regular keys)
//! - Conflict detection and modal triggering
//! - Input state tracking
//!
//! This module will be used by the future dedicated "Keybinds" screen/tab
//! (accessed via "Edit Keybinds" button).

mod binding_logic;

use moho_input::bindings::Binding;
use moho_input::key::Key;

use super::binding_registry::BindingRegistry;
use super::conflict_modal::ConflictModalState;
use super::types::display_name;
use crate::actions::StrategyAction;

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
    /// * `bindings` - The staged bindings to check for conflicts
    /// * `on_binding_changed` - Callback to update staged binding
    pub fn handle_winit_event<F>(
        &mut self,
        event: &winit::event::WindowEvent,
        bindings: &BindingRegistry,
        on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(StrategyAction, Vec<Binding>),
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

            // Keys the engine does not name cannot be bound; keep listening.
            let Some(key) = Key::from_winit(key_event.physical_key) else {
                return false;
            };

            return self.apply_key_while_listening(key, bindings, on_binding_changed);
        }
        false
    }

    /// Whether the conflict dialog is currently visible. Taking the dialog hides it
    /// while the pending binding is kept, so this is `false` once it has been taken.
    // Note: has_pending_binding is provided for API completeness but currently unused.
    // The conflict modal visibility is checked directly in most cases.
    #[allow(dead_code)]
    pub fn has_pending_binding(&self) -> bool {
        self.conflict_modal.is_visible()
    }

    /// Apply the pending binding (called after user confirms in modal)
    ///
    /// # Arguments
    /// * `bindings` - The staged bindings, to keep the holder's other keys
    /// * `on_binding_changed` - Callback to update staged bindings
    pub fn apply_pending<F>(&mut self, bindings: &BindingRegistry, mut on_binding_changed: F)
    where
        F: FnMut(StrategyAction, Vec<Binding>),
    {
        if let Some(pending) = self.conflict_modal.take_pending() {
            // The holder loses only the contested key
            if let Some(conflicting) = pending.conflicting {
                let remaining = bindings
                    .get_binding(conflicting)
                    .iter()
                    .filter(|b| **b != Binding::Key(pending.key))
                    .copied()
                    .collect();
                on_binding_changed(conflicting, remaining);
            }

            on_binding_changed(pending.target, vec![Binding::Key(pending.key)]);
        }
        self.conflict_modal.hide();
    }

    /// Cancel the pending binding (called when user cancels modal)
    pub fn cancel_pending_binding(&mut self) {
        self.conflict_modal.clear();
    }

    /// Take the dialog text (conflict key name, binding description) if the
    /// modal is visible, hiding it while keeping the pending binding for a
    /// later `apply_pending`.
    pub fn take_conflict_modal(&mut self) -> Option<(String, String)> {
        if !self.conflict_modal.is_visible() {
            return None;
        }
        self.conflict_modal.hide();
        Some((
            self.conflict_modal.conflict_key_name().to_string(),
            self.conflict_modal.conflict_binding_desc().to_string(),
        ))
    }

    pub(super) fn get_key_name(action: StrategyAction) -> &'static str {
        display_name(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Prefs;

    fn registry() -> BindingRegistry {
        BindingRegistry::from_prefs(&Prefs::default())
    }

    #[test]
    fn test_modifier_only_capture() {
        let mut handler = KeybindCaptureHandler::new();
        let bindings = registry();
        let mut bindings_changed = vec![];

        handler.start_listening(0);

        // Alt is free by default (Descend holds Ctrl, Sprint holds Shift).
        let consumed = handler.apply_key_while_listening(Key::Alt, &bindings, |action, b| {
            bindings_changed.push((action, b));
        });

        assert!(consumed);
        assert!(!handler.is_listening());
        assert_eq!(
            bindings_changed,
            vec![(StrategyAction::MoveForward, vec![Binding::Key(Key::Alt)])]
        );
    }

    #[test]
    fn test_conflict_detection() {
        let mut handler = KeybindCaptureHandler::new();
        let bindings = registry(); // Move Forward holds W
        let mut bindings_changed = vec![];

        handler.start_listening(1);

        let consumed = handler.apply_key_while_listening(Key::W, &bindings, |action, b| {
            bindings_changed.push((action, b));
        });

        assert!(consumed);
        assert!(!handler.is_listening());
        assert!(bindings_changed.is_empty()); // No binding applied yet
        assert!(handler.has_pending_binding()); // Conflict modal should be triggered
    }

    #[test]
    fn take_conflict_modal_returns_holder_name_then_key_description() {
        let mut handler = KeybindCaptureHandler::new();
        let bindings = registry();

        handler.start_listening(1); // Move Left
        handler.apply_key_while_listening(Key::W, &bindings, |_, _| {});

        assert_eq!(
            handler.take_conflict_modal(),
            Some(("Move Forward".to_string(), "W".to_string()))
        );
    }

    fn resolve_conflict(target_id: usize, key: Key) -> Vec<(StrategyAction, Vec<Binding>)> {
        let mut handler = KeybindCaptureHandler::new();
        let bindings = registry();

        handler.start_listening(target_id);
        handler.apply_key_while_listening(key, &bindings, |_, _| {});
        let mut changes = vec![];
        handler.apply_pending(&bindings, |action, b| changes.push((action, b)));
        changes
    }

    #[test]
    fn move_down_taking_shift_unbinds_sprint() {
        let changes = resolve_conflict(5, Key::Shift);

        assert_eq!(changes.len(), 2);
        assert!(changes.contains(&(StrategyAction::Sprint, vec![])));
        assert!(changes.contains(&(StrategyAction::Descend, vec![Binding::Key(Key::Shift)])));
    }

    #[test]
    fn sprint_taking_a_unbinds_move_left() {
        let changes = resolve_conflict(6, Key::A);

        assert_eq!(changes.len(), 2);
        assert!(changes.contains(&(StrategyAction::MoveLeft, vec![])));
        assert!(changes.contains(&(StrategyAction::Sprint, vec![Binding::Key(Key::A)])));
    }

    #[test]
    fn new_handler_has_no_pending_binding() {
        assert!(!KeybindCaptureHandler::new().has_pending_binding());
    }
}
