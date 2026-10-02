use super::super::SettingsField;
use super::super::binding_registry::BindingRegistry;
use super::super::conflict_modal::PendingBinding;
use super::super::key_mapping::binding_label;
use super::super::types::BindingId;
use super::KeybindCaptureHandler;
use super::modifier_encoding::{create_binding_from_key, detect_active_modifiers};
use crate::prefs::Binding;

impl KeybindCaptureHandler {
    /// Testable helper: apply a resolved key code while the menu is listening.
    ///
    /// This function contains the core logic for applying a binding or queuing a
    /// conflict modal when `listening` is active. It's public so unit tests
    /// can exercise the behavior without depending on winit event construction.
    ///
    /// For testing purposes: allows direct simulation of key input during binding listen mode.
    ///
    /// # Arguments
    /// * `code` - The resolved key code
    /// * `mods_bits` - Modifier bitfield (Ctrl=1, Shift=2, Alt=4)
    /// * `staged_prefs` - The staged preferences to check for conflicts
    /// * `on_binding_changed` - Callback to update staged binding
    pub fn apply_key_code_while_listening<F>(
        &mut self,
        code: u32,
        mods_bits: u8,
        staged_prefs: &crate::prefs::Prefs,
        mut on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(SettingsField, Binding),
    {
        // If not listening, ignore
        let listen_id = match self.listening {
            Some(id) => id,
            None => return false,
        };

        // Escape is reserved: cancel listening
        if code == 0x200 {
            self.cancel_pending_binding();
            self.listening = None;
            return true;
        }

        // If the pressed key is a pure modifier key (mapped to special codes), and
        // there are no other modifiers active, treat it as a modifier-only binding.
        let is_pure_modifier = code == 0x204 || code == 0x205 || code == 0x206;
        let binding = if is_pure_modifier && mods_bits == 0 {
            Binding::new(code, 0)
        } else {
            Binding::new(code, mods_bits)
        };

        // Check for duplicate bindings using registry
        let registry = BindingRegistry::from_prefs(staged_prefs);
        let exclude_id = BindingId::from_usize(listen_id).expect("Invalid binding ID");
        let conflicting_id = registry.find_conflict(&binding, exclude_id);

        if let Some(conflict_bid) = conflicting_id {
            let conflict_id = conflict_bid.to_usize();
            let pending = PendingBinding {
                target_id: listen_id,
                binding,
                conflicting_id: Some(conflict_id),
            };
            let conflict_key_name = Self::get_key_name(conflict_id).to_string();
            let conflict_binding_desc = binding_label(&binding);
            self.conflict_modal
                .show(pending, conflict_key_name, conflict_binding_desc);
            self.listening = None;
            true
        } else {
            // Apply binding using callback
            let binding_id = match BindingId::from_listen_id(listen_id) {
                Some(id) => id,
                None => {
                    self.listening = None;
                    return true;
                }
            };
            let field = binding_id.to_settings_field();
            on_binding_changed(field, binding);
            self.listening = None;
            true
        }
    }

    /// Handle key capture when listening for a binding.
    /// Processes keyboard input from egui context to update bindings.
    ///
    /// # Arguments
    /// * `ctx` - The egui context to read input from
    /// * `staged_prefs` - The staged preferences to check for conflicts
    /// * `on_binding_changed` - Callback to update staged binding
    pub fn handle_key_capture<F>(
        &mut self,
        ctx: &egui::Context,
        staged_prefs: &crate::prefs::Prefs,
        mut on_binding_changed: F,
    ) where
        F: FnMut(SettingsField, Binding),
    {
        if self.listening.is_none() {
            return;
        }

        ctx.input(|input| {
            // Try to capture modifier-only bindings first
            let cur_mods = detect_active_modifiers(input);
            if self.capture_modifier_if_listening(cur_mods, staged_prefs, &mut on_binding_changed) {
                return;
            }

            // Process normal key events
            self.process_key_events(input, staged_prefs, on_binding_changed);
        });
    }

    /// Returns true if a binding was applied or a conflict modal was queued.
    pub(crate) fn capture_modifier_if_listening<F>(
        &mut self,
        cur_mods: u8,
        staged_prefs: &crate::prefs::Prefs,
        mut on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(SettingsField, Binding),
    {
        if let Some(listen_id) = self.listening
            && cur_mods != self.last_mods
        {
            if self.last_mods == 0 && (cur_mods == 1 || cur_mods == 2 || cur_mods == 4) {
                let code = match cur_mods {
                    1 => 0x205,
                    2 => 0x204,
                    4 => 0x206,
                    _ => 0,
                };
                let binding = Binding::new(code, 0);

                // Check for duplicate bindings using registry
                let registry = BindingRegistry::from_prefs(staged_prefs);
                let exclude_id = BindingId::from_usize(listen_id).expect("Invalid binding ID");
                let conflicting_id = registry.find_conflict(&binding, exclude_id);

                if let Some(conflict_bid) = conflicting_id {
                    let conflict_id = conflict_bid.to_usize();
                    let pending = PendingBinding {
                        target_id: listen_id,
                        binding,
                        conflicting_id: Some(conflict_id),
                    };
                    let conflict_key_name = Self::get_key_name(conflict_id).to_string();
                    let conflict_binding_desc = binding_label(&binding);
                    self.conflict_modal
                        .show(pending, conflict_key_name, conflict_binding_desc);
                    self.last_mods = cur_mods;
                    return true;
                } else {
                    let binding_id = match BindingId::from_listen_id(listen_id) {
                        Some(id) => id,
                        None => {
                            self.last_mods = cur_mods;
                            return false;
                        }
                    };
                    let field = binding_id.to_settings_field();
                    on_binding_changed(field, binding);
                    self.listening = None;
                    self.last_mods = cur_mods;
                    return true;
                }
            }
            self.last_mods = cur_mods;
        }

        false
    }

    /// Apply a binding or show conflict modal.
    /// Returns true if binding was handled (applied or conflict shown).
    fn apply_binding_or_show_conflict<F>(
        &mut self,
        binding: Binding,
        listen_id: usize,
        staged_prefs: &crate::prefs::Prefs,
        mut on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(SettingsField, Binding),
    {
        // Check for conflicts using registry
        let registry = BindingRegistry::from_prefs(staged_prefs);
        let exclude_id = match BindingId::from_usize(listen_id) {
            Some(id) => id,
            None => return false, // Invalid listen_id
        };
        let conflicting_id = registry.find_conflict(&binding, exclude_id);

        if let Some(conflict_bid) = conflicting_id {
            // Conflict detected - show modal
            let conflict_id = conflict_bid.to_usize();
            let pending = PendingBinding {
                target_id: listen_id,
                binding,
                conflicting_id: Some(conflict_id),
            };
            let conflict_key_name = Self::get_key_name(conflict_id).to_string();
            let conflict_binding_desc = binding_label(&binding);
            self.conflict_modal
                .show(pending, conflict_key_name, conflict_binding_desc);
            true
        } else {
            // No conflict - apply binding directly
            let binding_id = match BindingId::from_listen_id(listen_id) {
                Some(id) => id,
                None => return false, // Invalid listen_id
            };
            let field = binding_id.to_settings_field();
            on_binding_changed(field, binding);
            true
        }
    }

    /// Process a single key event during binding capture.
    /// Returns true if the key was handled (stops listening).
    fn process_single_key<F>(
        &mut self,
        key: &egui::Key,
        modifiers: &egui::Modifiers,
        staged_prefs: &crate::prefs::Prefs,
        on_binding_changed: F,
    ) -> bool
    where
        F: FnMut(SettingsField, Binding),
    {
        // Escape cancels listening mode
        if *key == egui::Key::Escape {
            return true; // Stop listening
        }

        let listen_id = match self.listening {
            Some(id) => id,
            None => return false,
        };

        let binding = create_binding_from_key(key, modifiers);
        self.apply_binding_or_show_conflict(binding, listen_id, staged_prefs, on_binding_changed)
    }

    /// Process all key events from egui input.
    fn process_key_events<F>(
        &mut self,
        input: &egui::InputState,
        staged_prefs: &crate::prefs::Prefs,
        mut on_binding_changed: F,
    ) where
        F: FnMut(SettingsField, Binding),
    {
        for ev in &input.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = ev
                && self.process_single_key(key, modifiers, staged_prefs, &mut on_binding_changed)
            {
                self.listening = None;
                return;
            }
        }
    }
}
