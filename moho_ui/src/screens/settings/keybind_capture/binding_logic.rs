use moho_input::bindings::Binding;
use moho_input::key::Key;

use super::super::binding_registry::BindingRegistry;
use super::super::conflict_modal::PendingBinding;
use super::super::key_mapping::{binding_label, egui_key_to_key, lone_modifier_key, modifier_bits};
use super::super::types::row_action;
use super::KeybindCaptureHandler;
use crate::actions::StrategyAction;

impl KeybindCaptureHandler {
    /// Testable helper: apply a resolved key while the menu is listening.
    ///
    /// This function contains the core logic for applying a binding or queuing a
    /// conflict modal when `listening` is active. It's public so unit tests
    /// can exercise the behavior without depending on winit event construction.
    ///
    /// # Arguments
    /// * `key` - The resolved key
    /// * `bindings` - The staged bindings to check for conflicts
    /// * `on_binding_changed` - Callback to update staged binding
    pub fn apply_key_while_listening<F>(
        &mut self,
        key: Key,
        bindings: &BindingRegistry,
        on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(StrategyAction, Vec<Binding>),
    {
        // If not listening, ignore
        let Some(listen_id) = self.listening else {
            return false;
        };

        // Escape is reserved: cancel listening
        if key == Key::Escape {
            self.cancel_pending_binding();
            self.listening = None;
            return true;
        }

        let Some(target) = row_action(listen_id) else {
            self.listening = None;
            return true;
        };
        self.place_key(target, key, bindings, on_binding_changed);
        self.listening = None;
        true
    }

    /// Handle key capture when listening for a binding.
    /// Processes keyboard input from egui context to update bindings.
    ///
    /// # Arguments
    /// * `ctx` - The egui context to read input from
    /// * `bindings` - The staged bindings to check for conflicts
    /// * `on_binding_changed` - Callback to update staged binding
    pub fn handle_key_capture<F>(
        &mut self,
        ctx: &egui::Context,
        bindings: &BindingRegistry,
        mut on_binding_changed: F,
    ) where
        F: FnMut(StrategyAction, Vec<Binding>),
    {
        if self.listening.is_none() {
            return;
        }

        ctx.input(|input| {
            // Try to capture modifier-only bindings first
            let cur_mods = modifier_bits(&input.modifiers);
            if self.capture_modifier_if_listening(cur_mods, bindings, &mut on_binding_changed) {
                return;
            }

            // Process normal key events
            self.process_key_events(input, bindings, on_binding_changed);
        });
    }

    /// Returns true if a binding was applied or a conflict modal was queued.
    pub(crate) fn capture_modifier_if_listening<F>(
        &mut self,
        cur_mods: u8,
        bindings: &BindingRegistry,
        on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(StrategyAction, Vec<Binding>),
    {
        if let Some(listen_id) = self.listening
            && cur_mods != self.last_mods
        {
            let pressed_alone = if self.last_mods == 0 {
                lone_modifier_key(cur_mods)
            } else {
                None
            };
            self.last_mods = cur_mods;
            if let Some(key) = pressed_alone {
                let Some(target) = row_action(listen_id) else {
                    return false;
                };
                if self.place_key(target, key, bindings, on_binding_changed) {
                    self.listening = None;
                }
                return true;
            }
        }

        false
    }

    /// Bind `key` to `target`, or queue the conflict modal when another row holds it.
    /// Returns true if the binding was applied.
    fn place_key<F>(
        &mut self,
        target: StrategyAction,
        key: Key,
        bindings: &BindingRegistry,
        mut on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(StrategyAction, Vec<Binding>),
    {
        if let Some(conflicting) = bindings.find_conflict(key, target) {
            let pending = PendingBinding {
                target,
                key,
                conflicting: Some(conflicting),
            };
            let conflict_key_name = Self::get_key_name(conflicting).to_string();
            let conflict_binding_desc = binding_label(&[Binding::Key(key)]);
            self.conflict_modal
                .show(pending, conflict_key_name, conflict_binding_desc);
            false
        } else {
            on_binding_changed(target, vec![Binding::Key(key)]);
            true
        }
    }

    /// Process all key events from egui input.
    fn process_key_events<F>(
        &mut self,
        input: &egui::InputState,
        bindings: &BindingRegistry,
        mut on_binding_changed: F,
    ) where
        F: FnMut(StrategyAction, Vec<Binding>),
    {
        for ev in &input.events {
            if let egui::Event::Key {
                key, pressed: true, ..
            } = ev
            {
                // Escape cancels listening mode
                if *key == egui::Key::Escape {
                    self.listening = None;
                    return;
                }
                let Some(listen_id) = self.listening else {
                    return;
                };
                let (Some(target), Some(key)) = (row_action(listen_id), egui_key_to_key(*key))
                else {
                    continue;
                };
                self.place_key(target, key, bindings, &mut on_binding_changed);
                self.listening = None;
                return;
            }
        }
    }
}
