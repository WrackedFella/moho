mod audio_tab;
mod binding_registry;
mod conflict_modal;
mod controls_tab;
mod state;
mod types;

pub use types::{BindingId, SettingsTab};
use binding_registry::BindingRegistry;
use conflict_modal::{ConflictModalState, PendingBinding};
use state::SettingsState;

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
    listening: Option<usize>,
    conflict_modal: ConflictModalState,
    last_mods: u8,
}

impl SettingsMenu {
    pub fn new() -> Self {
        Self {
            spec: ScreenSpec::default(),
            active_tab: SettingsTab::default(),
            state: SettingsState::new(),
            listening: None,
            conflict_modal: ConflictModalState::new(),
            last_mods: 0,
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
            listening: None,
            conflict_modal: ConflictModalState::new(),
            last_mods: 0,
        }
    }

    /// Return true when the menu is currently listening for a new binding.
    pub fn is_listening(&self) -> bool {
        self.listening.is_some()
    }

    /// Get a reference to the conflict modal state (for testing).
    pub fn conflict_modal(&self) -> &ConflictModalState {
        &self.conflict_modal
    }

    /// Handle a winit WindowEvent when the settings menu is listening for a binding.
    /// Returns true if the event was consumed (binding applied or cancelled).
    pub fn handle_winit_event(&mut self, event: &winit::event::WindowEvent) -> bool {
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

            return self.apply_key_code_while_listening(code, mods_bits);
        }
        false
    }

    /// Testable helper: apply a resolved key code while the menu is listening.
    ///
    /// This function contains the core logic for applying a binding or queuing a
    /// conflict modal when `listening` is active. It's public so unit tests
    /// can exercise the behavior without depending on winit event construction.
    /// 
    /// For testing purposes: allows direct simulation of key input during binding listen mode.
    pub fn apply_key_code_while_listening(&mut self, code: u32, mods_bits: u8) -> bool {
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
        let registry = BindingRegistry::from_prefs(self.state.staged());
        let exclude_id = BindingId::from_usize(listen_id).expect("Invalid binding ID");
        let conflicting_id = registry.find_conflict(&binding, exclude_id);

        if let Some(conflict_bid) = conflicting_id {
            let conflict_id = conflict_bid.to_usize();
            let pending = PendingBinding {
                target_id: listen_id,
                binding,
                conflicting_id: Some(conflict_id),
            };
            let conflict_key_name = self.get_key_name(conflict_id).to_string();
            let conflict_binding_desc = Self::binding_label(&binding);
            self.conflict_modal
                .show(pending, conflict_key_name, conflict_binding_desc);
            self.listening = None;
            true
        } else {
            // Apply binding using state management
            let field = match listen_id {
                0 => SettingsField::KeyW,
                1 => SettingsField::KeyA,
                2 => SettingsField::KeyS,
                3 => SettingsField::KeyD,
                4 => SettingsField::KeyUp,
                5 => SettingsField::KeyDown,
                _ => {
                    self.listening = None;
                    return true;
                }
            };
            self.state.set_staged_binding(field, binding);
            self.listening = None;
            true
        }
    }

    fn get_key_name(&self, id: usize) -> &str {
        BindingId::from_usize(id)
            .map(|bid| bid.display_name())
            .unwrap_or("Unknown")
    }

    /// Map egui::Key to numeric code for binding storage.
    /// Letters and digits map to their ASCII uppercased codes.
    fn key_to_code(k: &egui::Key) -> u32 {
        use egui::Key::*;
        match k {
            A => 'A' as u32,
            B => 'B' as u32,
            C => 'C' as u32,
            D => 'D' as u32,
            E => 'E' as u32,
            F => 'F' as u32,
            G => 'G' as u32,
            H => 'H' as u32,
            I => 'I' as u32,
            J => 'J' as u32,
            K => 'K' as u32,
            L => 'L' as u32,
            M => 'M' as u32,
            N => 'N' as u32,
            O => 'O' as u32,
            P => 'P' as u32,
            Q => 'Q' as u32,
            R => 'R' as u32,
            S => 'S' as u32,
            T => 'T' as u32,
            U => 'U' as u32,
            V => 'V' as u32,
            W => 'W' as u32,
            X => 'X' as u32,
            Y => 'Y' as u32,
            Z => 'Z' as u32,
            Num0 => '0' as u32,
            Num1 => '1' as u32,
            Num2 => '2' as u32,
            Num3 => '3' as u32,
            Num4 => '4' as u32,
            Num5 => '5' as u32,
            Num6 => '6' as u32,
            Num7 => '7' as u32,
            Num8 => '8' as u32,
            Num9 => '9' as u32,
            ArrowUp => 0x100,
            ArrowDown => 0x101,
            ArrowLeft => 0x102,
            ArrowRight => 0x103,
            Escape => 0x200,
            Tab => 0x201,
            Backspace => 0x202,
            Enter => 0x203,
            Space => ' ' as u32,
            _ => 0,
        }
    }

    /// Handle key capture when listening for a binding.
    /// Processes keyboard input from egui context to update bindings.
    ///
    /// # Arguments
    /// * `ctx` - The egui context to read input from
    fn handle_key_capture(&mut self, ctx: &egui::Context) {
        if let Some(listen_id) = self.listening {
            ctx.input(|input| {
                // Detect modifier-only presses via modifiers state
                let mut cur_mods: u8 = 0;
                if input.modifiers.ctrl {
                    cur_mods |= 1;
                }
                if input.modifiers.shift {
                    cur_mods |= 2;
                }
                if input.modifiers.alt {
                    cur_mods |= 4;
                }

                // Try to capture modifier-only bindings first
                if self.capture_modifier_if_listening(cur_mods) {
                    return;
                }

                // Process normal key events
                for ev in &input.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = ev
                    {
                        // Escape cancels listening mode
                        if *key == egui::Key::Escape {
                            self.listening = None;
                            return;
                        }

                        let mut code: u32 = Self::key_to_code(key);
                        let mut mods: u8 = 0;
                        if modifiers.ctrl {
                            mods |= 1;
                        }
                        if modifiers.shift {
                            mods |= 2;
                        }
                        if modifiers.alt {
                            mods |= 4;
                        }

                        // Handle pure modifier keys (Ctrl, Shift, Alt)
                        if code == 0 {
                            if mods == 1 {
                                code = 0x205; // Ctrl
                                mods = 0;
                            } else if mods == 2 {
                                code = 0x204; // Shift
                                mods = 0;
                            } else if mods == 4 {
                                code = 0x206; // Alt
                                mods = 0;
                            }
                        }

                        let binding = Binding::new(code, mods);

                        // Check for conflicts using registry
                        let registry = BindingRegistry::from_prefs(self.state.staged());
                        let exclude_id =
                            BindingId::from_usize(listen_id).expect("Invalid binding ID");
                        let conflicting_id = registry.find_conflict(&binding, exclude_id);

                        if let Some(conflict_bid) = conflicting_id {
                            // Conflict detected - show modal
                            let conflict_id = conflict_bid.to_usize();
                            let pending = PendingBinding {
                                target_id: listen_id,
                                binding,
                                conflicting_id: Some(conflict_id),
                            };
                            let conflict_key_name = self.get_key_name(conflict_id).to_string();
                            let conflict_binding_desc = Self::binding_label(&binding);
                            self.conflict_modal
                                .show(pending, conflict_key_name, conflict_binding_desc);
                        } else {
                            // No conflict - apply binding directly
                            let field = match listen_id {
                                0 => SettingsField::KeyW,
                                1 => SettingsField::KeyA,
                                2 => SettingsField::KeyS,
                                3 => SettingsField::KeyD,
                                4 => SettingsField::KeyUp,
                                5 => SettingsField::KeyDown,
                                _ => SettingsField::KeyW, // Fallback (shouldn't happen)
                            };
                            self.state.set_staged_binding(field, binding);
                        }
                        self.listening = None;
                    }
                }
            });
        }
    }

    pub(super) fn binding_label(b: &Binding) -> String {
        if b.code == 0 && b.mods == 0 {
            return "Unbound".to_string();
        }

        let mut s = String::new();

        // If only modifiers are set (no key code), show just the modifier
        if b.code == 0 {
            if b.mods & 1 != 0 {
                s.push_str("Ctrl");
            }
            if b.mods & 2 != 0 {
                if !s.is_empty() {
                    s.push('+');
                }
                s.push_str("Shift");
            }
            if b.mods & 4 != 0 {
                if !s.is_empty() {
                    s.push('+');
                }
                s.push_str("Alt");
            }
            return s;
        }

        // Add modifiers prefix
        if b.mods & 1 != 0 {
            s.push_str("Ctrl+");
        }
        if b.mods & 2 != 0 {
            s.push_str("Shift+");
        }
        if b.mods & 4 != 0 {
            s.push_str("Alt+");
        }

        // Handle special keys first
        match b.code {
            0x100 => {
                s.push_str("ArrowUp");
                return s;
            }
            0x101 => {
                s.push_str("ArrowDown");
                return s;
            }
            0x102 => {
                s.push_str("ArrowLeft");
                return s;
            }
            0x103 => {
                s.push_str("ArrowRight");
                return s;
            }
            0x200 => {
                s.push_str("Escape");
                return s;
            }
            0x201 => {
                s.push_str("Tab");
                return s;
            }
            0x202 => {
                s.push_str("Backspace");
                return s;
            }
            0x203 => {
                s.push_str("Enter");
                return s;
            }
            _ => {}
        }

        // Handle Space specially
        if b.code == ' ' as u32 {
            s.push_str("Spacebar");
            return s;
        }

        // Handle regular ASCII characters
        if let Some(ch) = std::char::from_u32(b.code)
            && ch.is_ascii_graphic()
        {
            s.push(ch.to_ascii_uppercase());
            return s;
        }

        // Handle special keys
        match b.code {
            0x204 => s.push_str("Shift"),
            0x205 => s.push_str("Ctrl"),
            0x206 => s.push_str("Alt"),
            _ => s.push_str("Unknown"),
        }
        s
    }

    pub fn apply_pending_binding(&mut self) {
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
                self.state.set_staged_binding(field, Binding::new(0, 0));
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
            self.state.set_staged_binding(target_field, pending.binding);
        }
        self.conflict_modal.hide();
    }
    // Returns true if a binding was applied or a conflict modal was queued.
    pub(crate) fn capture_modifier_if_listening(&mut self, cur_mods: u8) -> bool {
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
                let registry = BindingRegistry::from_prefs(self.state.staged());
                let exclude_id = BindingId::from_usize(listen_id).expect("Invalid binding ID");
                let conflicting_id = registry.find_conflict(&binding, exclude_id);

                if let Some(conflict_bid) = conflicting_id {
                    let conflict_id = conflict_bid.to_usize();
                    let pending = PendingBinding {
                        target_id: listen_id,
                        binding,
                        conflicting_id: Some(conflict_id),
                    };
                    let conflict_key_name = self.get_key_name(conflict_id).to_string();
                    let conflict_binding_desc = Self::binding_label(&binding);
                    self.conflict_modal
                        .show(pending, conflict_key_name, conflict_binding_desc);
                    self.last_mods = cur_mods;
                    return true;
                } else {
                    let field = match listen_id {
                        0 => SettingsField::KeyW,
                        1 => SettingsField::KeyA,
                        2 => SettingsField::KeyS,
                        3 => SettingsField::KeyD,
                        4 => SettingsField::KeyUp,
                        5 => SettingsField::KeyDown,
                        _ => SettingsField::KeyW, // Fallback (shouldn't happen)
                    };
                    self.state.set_staged_binding(field, binding);
                    self.listening = None;
                    self.last_mods = cur_mods;
                    return true;
                }
            }
            self.last_mods = cur_mods;
        }

        false
    }

    pub fn cancel_pending_binding(&mut self) {
        self.conflict_modal.clear();
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
        self.listening = Some(binding_id);
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

        // Top panel for title and tab bar - reserves space at top
        egui::TopBottomPanel::top("settings_top").show(ctx, |ui| {
            // Use same gutter percentage as content area (30%)
            let avail = ui.available_width();
            let gutter = FormControls::calculate_gutter(avail, 0.30);

            ui.horizontal(|ui| {
                ui.add_space(gutter);
                ui.allocate_ui_with_layout(
                    egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.add_space(16.0);
                        ui.heading("Game Settings");
                        ui.add_space(8.0);

                        // Tab bar
                        let new_tab_index = FormControls::tab_bar(
                            ui,
                            SettingsTab::all_tabs(),
                            self.active_tab.to_index(),
                        );
                        self.active_tab = SettingsTab::from_index(new_tab_index);

                        ui.add_space(12.0);
                    },
                );
                ui.add_space(gutter);
            });
        });

        // Bottom panel for buttons - reserves space at bottom
        egui::TopBottomPanel::bottom("settings_bottom").show(ctx, |ui| {
            // Use same gutter percentage as content area (30%)
            let avail = ui.available_width();
            let gutter = FormControls::calculate_gutter(avail, 0.30);

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(gutter);
                ui.allocate_ui_with_layout(
                    egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let save = ui.add(
                            egui::Button::new("Save Changes").min_size(egui::vec2(120.0, 36.0)),
                        );
                        let save_clicked = save.clicked();
                        if save_clicked {
                            let _ = self.state.apply_changes();
                            let _ = self.state.prefs().save();
                        }
                        items.push(super::MenuItem {
                            action: if save_clicked {
                                MenuAction::SettingsSaved(self.state.prefs().clone())
                            } else {
                                MenuAction::None
                            },
                            rect: Some(save.rect),
                            enabled: true,
                            clicked: save_clicked,
                        });
                        ui.add_space(8.0);

                        // Show Cancel when form is dirty, Back when clean
                        let is_dirty = self.is_dirty();
                        if is_dirty {
                            let cancel = ui
                                .add(egui::Button::new("Cancel").min_size(egui::vec2(100.0, 36.0)));
                            let cancel_clicked = cancel.clicked();
                            if cancel_clicked {
                                self.state.revert_changes();
                            }
                            items.push(super::MenuItem {
                                action: MenuAction::None,
                                rect: Some(cancel.rect),
                                enabled: true,
                                clicked: cancel_clicked,
                            });
                        } else {
                            let back =
                                ui.add(egui::Button::new("Back").min_size(egui::vec2(100.0, 36.0)));
                            let back_clicked = back.clicked();
                            items.push(super::MenuItem {
                                action: MenuAction::ShowMenu("start".to_string()),
                                rect: Some(back.rect),
                                enabled: true,
                                clicked: back_clicked,
                            });
                        }

                        ui.add_space(8.0);
                    },
                );
                ui.add_space(gutter);
            });
            ui.add_space(16.0);
        });

        // Central panel contains only the scrollable form content
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Use percentage-based gutters (30% on each side)
                    let avail = ui.available_width();
                    let gutter = FormControls::calculate_gutter(avail, 0.30);

                    ui.horizontal(|ui| {
                        ui.add_space(gutter);
                        ui.allocate_ui_with_layout(
                            egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                // Render the active tab content
                                match self.active_tab {
                                    SettingsTab::Controls => controls_tab::render(self, ui),
                                    SettingsTab::Audio => audio_tab::render(self, ui),
                                }
                            },
                        );
                        ui.add_space(gutter);
                    });
                });
        });

        // Handle key capture when listening for a binding
        self.handle_key_capture(ctx);

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
        self.handle_winit_event(event)
    }

    /// Check if settings wants to show the keybind conflict modal
    fn take_pending_modal(&mut self) -> Option<Box<dyn crate::modal::Modal>> {
        if self.conflict_modal.is_visible() {
            self.conflict_modal.hide();

            use crate::modals::KeybindConflictModal;
            let modal = KeybindConflictModal::new(
                self.conflict_modal.conflict_key_name().to_string(),
                self.conflict_modal.conflict_binding_desc().to_string(),
            );

            Some(Box::new(modal))
        } else {
            None
        }
    }

    /// Apply pending keybind when modal is confirmed
    fn on_modal_confirm(&mut self) {
        self.apply_pending_binding();
    }

    /// Cancel pending keybind when modal is cancelled
    fn on_modal_cancel(&mut self) {
        self.cancel_pending_binding();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_only_capture_applies() {
        let mut menu = SettingsMenu::new();
        menu.listening = Some(0);
        menu.last_mods = 0;

        let applied = menu.capture_modifier_if_listening(1);
        assert!(applied, "modifier capture should apply");
        assert!(
            menu.listening.is_none(),
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
        menu.listening = Some(0);
        menu.last_mods = 0;

        let applied = menu.capture_modifier_if_listening(1);
        assert!(applied, "modifier conflict should be processed");
        assert!(
            menu.conflict_modal.is_visible(),
            "conflict modal should be visible when conflict is detected"
        );
        // Verify the modal has the correct conflict info
        assert_eq!(menu.conflict_modal.conflict_key_name(), "Move Left");
        assert_eq!(menu.conflict_modal.conflict_binding_desc(), "Ctrl");
    }

    #[test]
    fn modifier_multiple_mods_ignored() {
        let mut menu = SettingsMenu::new();
        menu.listening = Some(0);
        menu.last_mods = 0;

        // ctrl+shift (bits 1 and 2) should not create a modifier-only binding
        let applied = menu.capture_modifier_if_listening(3);
        assert!(!applied, "combined modifiers should not be captured");
        assert!(
            menu.listening.is_some(),
            "still listening after ignored multi-modifier"
        );
        assert_eq!(menu.last_mods, 3);
    }

    #[test]
    fn egui_integration_modifier_capture() {
        let ctx = egui::Context::default();
        let mut menu = SettingsMenu::new();
        menu.listening = Some(0);
        menu.last_mods = 0;

        let mut raw = egui::RawInput::default();
        raw.modifiers.ctrl = true;

        let _full = ctx.run(raw, |ctx| {
            menu.render(ctx);
        });

        assert!(
            menu.state.get_staged_binding(SettingsField::KeyW) == Binding::new(0x205, 0)
        );
        assert!(menu.listening.is_none());
    }

    // Note: Scroll and low-level Key event simulation in integration tests
    // depends on the egui version's RawInput/Event API. We keep focused
    // integration coverage on modifiers here; other cases are covered by
    // unit tests and adapter-level behavior.
}
