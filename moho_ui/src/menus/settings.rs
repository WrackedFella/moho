use crate::forms::FormBuilder;
use crate::menus::menu::{Menu, MenuAction, MenuSpec};
use crate::prefs::{Binding, Prefs};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
enum SettingsField {
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

#[derive(Clone)]
struct PendingBinding {
    target_id: usize,
    binding: Binding,
    conflicting_id: Option<usize>,
}

pub struct SettingsMenu {
    spec: MenuSpec,
    prefs: Prefs,
    staged: Prefs,
    dirty_fields: HashSet<SettingsField>,
    listening: Option<usize>,
    pending_binding: Option<PendingBinding>,
    pub show_conflict_modal: bool,
    pub conflict_key_name: String,
    pub conflict_binding_desc: String,
    last_mods: u8,
}

impl SettingsMenu {
    pub fn new() -> Self {
        let prefs = Prefs::load();
        Self {
            spec: MenuSpec::default(),
            prefs: prefs.clone(),
            staged: prefs,
            dirty_fields: HashSet::new(),
            listening: None,
            pending_binding: None,
            show_conflict_modal: false,
            conflict_key_name: String::new(),
            conflict_binding_desc: String::new(),
            last_mods: 0,
        }
    }

    fn get_key_name(&self, id: usize) -> &str {
        match id {
            0 => "Move Forward",
            1 => "Move Left",
            2 => "Move Back",
            3 => "Move Right",
            4 => "Move Up",
            5 => "Move Down",
            _ => "Unknown",
        }
    }

    fn binding_label(b: &Binding) -> String {
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
        if let Some(pending) = self.pending_binding.take() {
            // Clear conflicting binding if any
            if let Some(conflict_id) = pending.conflicting_id {
                match conflict_id {
                    0 => {
                        self.staged.key_w = Binding::new(0, 0);
                        self.dirty_fields.insert(SettingsField::KeyW);
                    }
                    1 => {
                        self.staged.key_a = Binding::new(0, 0);
                        self.dirty_fields.insert(SettingsField::KeyA);
                    }
                    2 => {
                        self.staged.key_s = Binding::new(0, 0);
                        self.dirty_fields.insert(SettingsField::KeyS);
                    }
                    3 => {
                        self.staged.key_d = Binding::new(0, 0);
                        self.dirty_fields.insert(SettingsField::KeyD);
                    }
                    4 => {
                        self.staged.key_up = Binding::new(0, 0);
                        self.dirty_fields.insert(SettingsField::KeyUp);
                    }
                    5 => {
                        self.staged.key_down = Binding::new(0, 0);
                        self.dirty_fields.insert(SettingsField::KeyDown);
                    }
                    _ => {}
                }
            }

            // Apply new binding
            match pending.target_id {
                0 => {
                    self.staged.key_w = pending.binding;
                    self.dirty_fields.insert(SettingsField::KeyW);
                }
                1 => {
                    self.staged.key_a = pending.binding;
                    self.dirty_fields.insert(SettingsField::KeyA);
                }
                2 => {
                    self.staged.key_s = pending.binding;
                    self.dirty_fields.insert(SettingsField::KeyS);
                }
                3 => {
                    self.staged.key_d = pending.binding;
                    self.dirty_fields.insert(SettingsField::KeyD);
                }
                4 => {
                    self.staged.key_up = pending.binding;
                    self.dirty_fields.insert(SettingsField::KeyUp);
                }
                5 => {
                    self.staged.key_down = pending.binding;
                    self.dirty_fields.insert(SettingsField::KeyDown);
                }
                _ => {}
            }
        }
        self.show_conflict_modal = false;
    }

    // Attempt to capture a modifier-only binding when listening.
    // Returns true if a binding was applied or a conflict modal was queued.
    pub(crate) fn capture_modifier_if_listening(&mut self, cur_mods: u8) -> bool {
        if let Some(listen_id) = self.listening {
            if cur_mods != self.last_mods {
                if self.last_mods == 0 && (cur_mods == 1 || cur_mods == 2 || cur_mods == 4) {
                    let code = match cur_mods {
                        1 => 0x205,
                        2 => 0x204,
                        4 => 0x206,
                        _ => 0,
                    };
                    let binding = Binding::new(code, 0);

                    // Check for duplicate bindings
                    let mut conflicting_id: Option<usize> = None;
                    if binding.code != 0 {
                        if self.staged.key_w == binding && listen_id != 0 {
                            conflicting_id = Some(0);
                        } else if self.staged.key_a == binding && listen_id != 1 {
                            conflicting_id = Some(1);
                        } else if self.staged.key_s == binding && listen_id != 2 {
                            conflicting_id = Some(2);
                        } else if self.staged.key_d == binding && listen_id != 3 {
                            conflicting_id = Some(3);
                        } else if self.staged.key_up == binding && listen_id != 4 {
                            conflicting_id = Some(4);
                        } else if self.staged.key_down == binding && listen_id != 5 {
                            conflicting_id = Some(5);
                        }
                    }

                    if let Some(conflict_id) = conflicting_id {
                        self.pending_binding = Some(PendingBinding {
                            target_id: listen_id,
                            binding,
                            conflicting_id: Some(conflict_id),
                        });
                        self.conflict_key_name = self.get_key_name(conflict_id).to_string();
                        self.conflict_binding_desc = Self::binding_label(&binding);
                        self.show_conflict_modal = true;
                        self.last_mods = cur_mods;
                        return true;
                    } else {
                        match listen_id {
                            0 => {
                                self.staged.key_w = binding;
                                self.dirty_fields.insert(SettingsField::KeyW);
                            }
                            1 => {
                                self.staged.key_a = binding;
                                self.dirty_fields.insert(SettingsField::KeyA);
                            }
                            2 => {
                                self.staged.key_s = binding;
                                self.dirty_fields.insert(SettingsField::KeyS);
                            }
                            3 => {
                                self.staged.key_d = binding;
                                self.dirty_fields.insert(SettingsField::KeyD);
                            }
                            4 => {
                                self.staged.key_up = binding;
                                self.dirty_fields.insert(SettingsField::KeyUp);
                            }
                            5 => {
                                self.staged.key_down = binding;
                                self.dirty_fields.insert(SettingsField::KeyDown);
                            }
                            _ => {}
                        }
                        self.listening = None;
                        self.last_mods = cur_mods;
                        return true;
                    }
                }
                self.last_mods = cur_mods;
            }
        }
        false
    }

    pub fn cancel_pending_binding(&mut self) {
        self.pending_binding = None;
        self.show_conflict_modal = false;
    }

    fn is_dirty(&self) -> bool {
        !self.dirty_fields.is_empty()
    }

    fn paint_dirty_decor(ui: &mut egui::Ui, resp: &egui::Response, dirty: bool) {
        if dirty || resp.hovered() {
            let r = resp.rect;
            let hover_color = egui::Color32::from_rgba_premultiplied(150, 150, 150, 60);
            ui.painter().rect_filled(r, 4.0, hover_color);
        }
    }
}

impl Default for SettingsMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu for SettingsMenu {
    fn name(&self) -> &str {
        "settings"
    }

    fn spec(&self) -> &MenuSpec {
        &self.spec
    }

    fn ui(&mut self, ctx: &egui::Context) -> Vec<crate::menus::menu::MenuItem> {
        let mut items: Vec<crate::menus::menu::MenuItem> = Vec::new();

        // Top panel for title - reserves space at top
        egui::TopBottomPanel::top("settings_top").show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.heading("Game Settings");
                ui.add_space(12.0);
            });
        });

        // Bottom panel for buttons - reserves space at bottom
        egui::TopBottomPanel::bottom("settings_bottom").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save =
                        ui.add(egui::Button::new("Save Changes").min_size(egui::vec2(120.0, 36.0)));
                    let save_clicked = save.clicked();
                    if save_clicked {
                        // commit staged to prefs and save
                        self.prefs = self.staged.clone();
                        let _ = self.prefs.save();
                        // clear dirty flags
                        self.dirty_fields.clear();
                    }
                    items.push(crate::menus::menu::MenuItem {
                        action: if save_clicked {
                            MenuAction::SettingsSaved(self.prefs.clone())
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
                        let cancel =
                            ui.add(egui::Button::new("Cancel").min_size(egui::vec2(100.0, 36.0)));
                        let cancel_clicked = cancel.clicked();
                        if cancel_clicked {
                            // revert staged values to last saved prefs
                            self.staged = self.prefs.clone();
                            self.dirty_fields.clear();
                        }
                        items.push(crate::menus::menu::MenuItem {
                            action: MenuAction::None,
                            rect: Some(cancel.rect),
                            enabled: true,
                            clicked: cancel_clicked,
                        });
                    } else {
                        let back =
                            ui.add(egui::Button::new("Back").min_size(egui::vec2(100.0, 36.0)));
                        let back_clicked = back.clicked();
                        items.push(crate::menus::menu::MenuItem {
                            action: MenuAction::ShowMenu("start".to_string()),
                            rect: Some(back.rect),
                            enabled: true,
                            clicked: back_clicked,
                        });
                    }

                    ui.add_space(8.0);
                });
            });
        });

        // Central panel contains only the scrollable form content
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let gutter: f32 = 20.0;
                    let avail = ui.available_width();
                    ui.horizontal(|ui| {
                        ui.add_space(gutter);
                        ui.allocate_ui_with_layout(
                            egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.vertical_centered(|ui| {
                                    // Controls Section Header
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Controls")
                                                .size(18.0)
                                                .color(egui::Color32::from_rgb(200, 200, 200)),
                                        );
                                    });
                                    ui.add_space(4.0);
                                    ui.separator();
                                    ui.add_space(8.0);

                                    // Use fixed-width layout for right-aligned inputs
                                    let label_width = 150.0;

                                    // Move Forward
                                    {
                                        let is_dirty = self.staged.key_w != self.prefs.key_w;
                                        let clicked = FormBuilder::keybind_control(
                                            ui,
                                            "Move Forward:",
                                            &Self::binding_label(&self.staged.key_w),
                                            is_dirty,
                                            self.listening == Some(0),
                                            label_width,
                                        );
                                        if is_dirty {
                                            self.dirty_fields.insert(SettingsField::KeyW);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::KeyW);
                                        }
                                        if clicked {
                                            self.listening = Some(0);
                                        }
                                    }

                                    // Move Left
                                    {
                                        let is_dirty = self.staged.key_a != self.prefs.key_a;
                                        let clicked = FormBuilder::keybind_control(
                                            ui,
                                            "Move Left:",
                                            &Self::binding_label(&self.staged.key_a),
                                            is_dirty,
                                            self.listening == Some(1),
                                            label_width,
                                        );
                                        if is_dirty {
                                            self.dirty_fields.insert(SettingsField::KeyA);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::KeyA);
                                        }
                                        if clicked {
                                            self.listening = Some(1);
                                        }
                                    }

                                    // Move Back
                                    {
                                        let is_dirty = self.staged.key_s != self.prefs.key_s;
                                        let clicked = FormBuilder::keybind_control(
                                            ui,
                                            "Move Back:",
                                            &Self::binding_label(&self.staged.key_s),
                                            is_dirty,
                                            self.listening == Some(2),
                                            label_width,
                                        );
                                        if is_dirty {
                                            self.dirty_fields.insert(SettingsField::KeyS);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::KeyS);
                                        }
                                        if clicked {
                                            self.listening = Some(2);
                                        }
                                    }

                                    // Move Right
                                    {
                                        let is_dirty = self.staged.key_d != self.prefs.key_d;
                                        let clicked = FormBuilder::keybind_control(
                                            ui,
                                            "Move Right:",
                                            &Self::binding_label(&self.staged.key_d),
                                            is_dirty,
                                            self.listening == Some(3),
                                            label_width,
                                        );
                                        if is_dirty {
                                            self.dirty_fields.insert(SettingsField::KeyD);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::KeyD);
                                        }
                                        if clicked {
                                            self.listening = Some(3);
                                        }
                                    }

                                    // Move Up
                                    {
                                        let is_dirty = self.staged.key_up != self.prefs.key_up;
                                        let clicked = FormBuilder::keybind_control(
                                            ui,
                                            "Move Up:",
                                            &Self::binding_label(&self.staged.key_up),
                                            is_dirty,
                                            self.listening == Some(4),
                                            label_width,
                                        );
                                        if is_dirty {
                                            self.dirty_fields.insert(SettingsField::KeyUp);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::KeyUp);
                                        }
                                        if clicked {
                                            self.listening = Some(4);
                                        }
                                    }

                                    // Move Down
                                    {
                                        let is_dirty = self.staged.key_down != self.prefs.key_down;
                                        let clicked = FormBuilder::keybind_control(
                                            ui,
                                            "Move Down:",
                                            &Self::binding_label(&self.staged.key_down),
                                            is_dirty,
                                            self.listening == Some(5),
                                            label_width,
                                        );
                                        if is_dirty {
                                            self.dirty_fields.insert(SettingsField::KeyDown);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::KeyDown);
                                        }
                                        if clicked {
                                            self.listening = Some(5);
                                        }
                                    }

                                    ui.add_space(8.0);

                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(label_width, 28.0),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.label("Mouse Sensitivity:");
                                            },
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                // Slider matching keybind button width (120px) - added first so it appears on the right
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(120.0, 20.0),
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.spacing_mut().slider_width = 120.0;
                                                        let slider = ui.add(
                                                            egui::Slider::new(
                                                                &mut self.staged.mouse_sensitivity,
                                                                0.01..=10.0,
                                                            )
                                                            .show_value(false)
                                                            .min_decimals(0)
                                                            .max_decimals(2),
                                                        );
                                                        Self::paint_dirty_decor(
                                                            ui,
                                                            &slider,
                                                            (self.staged.mouse_sensitivity
                                                                - self.prefs.mouse_sensitivity)
                                                                .abs()
                                                                > f32::EPSILON,
                                                        );
                                                    },
                                                );
                                                ui.add_space(8.0);
                                                // Drag value input - added second so it appears on the left
                                                let drag = ui.add(
                                                    egui::DragValue::new(
                                                        &mut self.staged.mouse_sensitivity,
                                                    )
                                                    .range(0.01..=10.0)
                                                    .speed(0.1)
                                                    .min_decimals(2)
                                                    .max_decimals(2),
                                                );
                                                Self::paint_dirty_decor(
                                                    ui,
                                                    &drag,
                                                    (self.staged.mouse_sensitivity
                                                        - self.prefs.mouse_sensitivity)
                                                        .abs()
                                                        > f32::EPSILON,
                                                );
                                                if (self.staged.mouse_sensitivity
                                                    - self.prefs.mouse_sensitivity)
                                                    .abs()
                                                    > f32::EPSILON
                                                {
                                                    self.dirty_fields
                                                        .insert(SettingsField::MouseSensitivity);
                                                } else {
                                                    self.dirty_fields
                                                        .remove(&SettingsField::MouseSensitivity);
                                                }
                                            },
                                        );
                                    });

                                    ui.add_space(12.0);

                                    // Input Filtering Section
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(label_width, 28.0),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.label("Input Filtering:");
                                            },
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                // Toggle filtering on/off
                                                let checkbox = ui.checkbox(
                                                    &mut self.staged.input_filtering_enabled,
                                                    "Enable",
                                                );
                                                Self::paint_dirty_decor(
                                                    ui,
                                                    &checkbox,
                                                    self.staged.input_filtering_enabled
                                                        != self.prefs.input_filtering_enabled,
                                                );
                                                if self.staged.input_filtering_enabled
                                                    != self.prefs.input_filtering_enabled
                                                {
                                                    self.dirty_fields
                                                        .insert(SettingsField::InputFiltering);
                                                } else {
                                                    self.dirty_fields
                                                        .remove(&SettingsField::InputFiltering);
                                                }
                                            },
                                        );
                                    });

                                    ui.add_space(24.0);

                                    // Audio Section Header
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Audio")
                                                .size(18.0)
                                                .color(egui::Color32::from_rgb(200, 200, 200)),
                                        );
                                    });
                                    ui.add_space(4.0);
                                    ui.separator();
                                    ui.add_space(8.0);

                                    // Sound Effect Volume
                                    {
                                        let dirty = FormBuilder::volume_slider(
                                            ui,
                                            "Sound Effects:",
                                            &mut self.staged.audio_sound_effect_volume,
                                            self.prefs.audio_sound_effect_volume,
                                            label_width,
                                        );
                                        if dirty {
                                            self.dirty_fields
                                                .insert(SettingsField::AudioSoundEffect);
                                        } else {
                                            self.dirty_fields
                                                .remove(&SettingsField::AudioSoundEffect);
                                        }
                                    }

                                    // Music Volume
                                    {
                                        let dirty = FormBuilder::volume_slider(
                                            ui,
                                            "Music:",
                                            &mut self.staged.audio_music_volume,
                                            self.prefs.audio_music_volume,
                                            label_width,
                                        );
                                        if dirty {
                                            self.dirty_fields.insert(SettingsField::AudioMusic);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::AudioMusic);
                                        }
                                    }

                                    // UI Volume
                                    {
                                        let dirty = FormBuilder::volume_slider(
                                            ui,
                                            "User Interface:",
                                            &mut self.staged.audio_ui_volume,
                                            self.prefs.audio_ui_volume,
                                            label_width,
                                        );
                                        if dirty {
                                            self.dirty_fields.insert(SettingsField::AudioUI);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::AudioUI);
                                        }
                                    }

                                    // Voice Volume
                                    {
                                        let dirty = FormBuilder::volume_slider(
                                            ui,
                                            "Voice:",
                                            &mut self.staged.audio_voice_volume,
                                            self.prefs.audio_voice_volume,
                                            label_width,
                                        );
                                        if dirty {
                                            self.dirty_fields.insert(SettingsField::AudioVoice);
                                        } else {
                                            self.dirty_fields.remove(&SettingsField::AudioVoice);
                                        }
                                    }
                                });
                            },
                        );
                        ui.add_space(gutter);
                    });
                });
        });

        // helper: map egui::Key to numeric code. Letters and digits map to their ASCII uppercased codes.
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

        // Handle key capture when listening for binding
        if let Some(listen_id) = self.listening {
            // detect modifier-only presses via modifiers state (for keys like Ctrl/Shift/Alt)
            ctx.input(|input| {
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

                // Try to capture modifier-only bindings; ignore scroll and other events.
                if self.capture_modifier_if_listening(cur_mods) {
                    return;
                }

                // process normal key events as before
                for ev in &input.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = ev
                    {
                        if *key == egui::Key::Escape {
                            self.listening = None;
                            return;
                        }

                        let mut code: u32 = key_to_code(key);
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

                        if code == 0 {
                            if mods == 1 {
                                code = 0x205;
                                mods = 0;
                            } else if mods == 2 {
                                code = 0x204;
                                mods = 0;
                            } else if mods == 4 {
                                code = 0x206;
                                mods = 0;
                            }
                        }

                        let binding = Binding::new(code, mods);

                        let mut conflicting_id: Option<usize> = None;
                        if binding.code != 0 {
                            if self.staged.key_w == binding && listen_id != 0 {
                                conflicting_id = Some(0);
                            } else if self.staged.key_a == binding && listen_id != 1 {
                                conflicting_id = Some(1);
                            } else if self.staged.key_s == binding && listen_id != 2 {
                                conflicting_id = Some(2);
                            } else if self.staged.key_d == binding && listen_id != 3 {
                                conflicting_id = Some(3);
                            } else if self.staged.key_up == binding && listen_id != 4 {
                                conflicting_id = Some(4);
                            } else if self.staged.key_down == binding && listen_id != 5 {
                                conflicting_id = Some(5);
                            }
                        }

                        if let Some(conflict_id) = conflicting_id {
                            self.pending_binding = Some(PendingBinding {
                                target_id: listen_id,
                                binding,
                                conflicting_id: Some(conflict_id),
                            });
                            self.conflict_key_name = self.get_key_name(conflict_id).to_string();
                            self.conflict_binding_desc = Self::binding_label(&binding);
                            self.show_conflict_modal = true;
                        } else {
                            match listen_id {
                                0 => {
                                    self.staged.key_w = binding;
                                    self.dirty_fields.insert(SettingsField::KeyW);
                                }
                                1 => {
                                    self.staged.key_a = binding;
                                    self.dirty_fields.insert(SettingsField::KeyA);
                                }
                                2 => {
                                    self.staged.key_s = binding;
                                    self.dirty_fields.insert(SettingsField::KeyS);
                                }
                                3 => {
                                    self.staged.key_d = binding;
                                    self.dirty_fields.insert(SettingsField::KeyD);
                                }
                                4 => {
                                    self.staged.key_up = binding;
                                    self.dirty_fields.insert(SettingsField::KeyUp);
                                }
                                5 => {
                                    self.staged.key_down = binding;
                                    self.dirty_fields.insert(SettingsField::KeyDown);
                                }
                                _ => {}
                            }
                        }
                        self.listening = None;
                    }
                }
            });
        }

        items
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
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
        assert!(menu.listening.is_none(), "should stop listening after capture");
        assert_eq!(menu.staged.key_w, Binding::new(0x205, 0));
    }

    #[test]
    fn modifier_only_conflict_shows_modal() {
        let mut menu = SettingsMenu::new();
        // set staged key_a to Ctrl so Ctrl will conflict with listening target 0
        menu.staged.key_a = Binding::new(0x205, 0);
        menu.listening = Some(0);
        menu.last_mods = 0;

        let applied = menu.capture_modifier_if_listening(1);
        assert!(applied, "modifier conflict should be processed");
        assert!(menu.pending_binding.is_some(), "pending binding should be set on conflict");
        assert!(menu.show_conflict_modal, "conflict modal flag should be set");
    }

    #[test]
    fn modifier_multiple_mods_ignored() {
        let mut menu = SettingsMenu::new();
        menu.listening = Some(0);
        menu.last_mods = 0;

        // ctrl+shift (bits 1 and 2) should not create a modifier-only binding
        let applied = menu.capture_modifier_if_listening(3);
        assert!(!applied, "combined modifiers should not be captured");
        assert!(menu.listening.is_some(), "still listening after ignored multi-modifier");
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
            menu.ui(ctx);
        });

        assert!(menu.staged.key_w == Binding::new(0x205, 0));
        assert!(menu.listening.is_none());
    }

    // Note: Scroll and low-level Key event simulation in integration tests
    // depends on the egui version's RawInput/Event API. We keep focused
    // integration coverage on modifiers here; other cases are covered by
    // unit tests and adapter-level behavior.
}
