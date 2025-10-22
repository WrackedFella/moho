use crate::menus::menu::{Menu, MenuAction, MenuSpec};
use crate::prefs::{Binding, Prefs};

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
    // per-field dirty flags
    dirty_key_w: bool,
    dirty_key_a: bool,
    dirty_key_s: bool,
    dirty_key_d: bool,
    dirty_mouse_sens: bool,
    dirty_filtering_enabled: bool,
    listening: Option<usize>,
    pending_binding: Option<PendingBinding>,
    pub show_conflict_modal: bool,
    pub conflict_key_name: String,
    pub conflict_binding_desc: String,
}

impl SettingsMenu {
    pub fn new() -> Self {
        let prefs = Prefs::load();
        Self {
            spec: MenuSpec::default(),
            prefs: prefs.clone(),
            staged: prefs,
            dirty_key_w: false,
            dirty_key_a: false,
            dirty_key_s: false,
            dirty_key_d: false,
            dirty_mouse_sens: false,
            dirty_filtering_enabled: false,
            listening: None,
            pending_binding: None,
            show_conflict_modal: false,
            conflict_key_name: String::new(),
            conflict_binding_desc: String::new(),
        }
    }

    fn get_key_name(&self, id: usize) -> &str {
        match id {
            0 => "Move Forward",
            1 => "Move Left",
            2 => "Move Back",
            3 => "Move Right",
            _ => "Unknown",
        }
    }

    fn binding_label(b: &Binding) -> String {
        if b.code == 0 {
            return "Unbound".to_string();
        }
        let mut s = String::new();
        if b.mods & 1 != 0 {
            s.push_str("Ctrl+");
        }
        if b.mods & 2 != 0 {
            s.push_str("Shift+");
        }
        if b.mods & 4 != 0 {
            s.push_str("Alt+");
        }
        if let Some(ch) = std::char::from_u32(b.code)
            && ch.is_ascii_graphic()
        {
            s.push(ch.to_ascii_uppercase());
            return s;
        }
        match b.code {
            0x100 => s.push_str("ArrowUp"),
            0x101 => s.push_str("ArrowDown"),
            0x102 => s.push_str("ArrowLeft"),
            0x103 => s.push_str("ArrowRight"),
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
                        self.dirty_key_w = true;
                    }
                    1 => {
                        self.staged.key_a = Binding::new(0, 0);
                        self.dirty_key_a = true;
                    }
                    2 => {
                        self.staged.key_s = Binding::new(0, 0);
                        self.dirty_key_s = true;
                    }
                    3 => {
                        self.staged.key_d = Binding::new(0, 0);
                        self.dirty_key_d = true;
                    }
                    _ => {}
                }
            }

            // Apply new binding
            match pending.target_id {
                0 => {
                    self.staged.key_w = pending.binding;
                    self.dirty_key_w = true;
                }
                1 => {
                    self.staged.key_a = pending.binding;
                    self.dirty_key_a = true;
                }
                2 => {
                    self.staged.key_s = pending.binding;
                    self.dirty_key_s = true;
                }
                3 => {
                    self.staged.key_d = pending.binding;
                    self.dirty_key_d = true;
                }
                _ => {}
            }
        }
        self.show_conflict_modal = false;
    }

    pub fn cancel_pending_binding(&mut self) {
        self.pending_binding = None;
        self.show_conflict_modal = false;
    }

    fn is_dirty(&self) -> bool {
        self.dirty_key_w
            || self.dirty_key_a
            || self.dirty_key_s
            || self.dirty_key_d
            || self.dirty_mouse_sens
            || self.dirty_filtering_enabled
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

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);

                ui.heading("Game Settings");
                ui.add_space(12.0);

                let binding_label = |b: &Binding| -> String {
                    if b.code == 0 {
                        return "Unbound".to_string();
                    }
                    let mut s = String::new();
                    if b.mods & 1 != 0 {
                        s.push_str("Ctrl+");
                    }
                    if b.mods & 2 != 0 {
                        s.push_str("Shift+");
                    }
                    if b.mods & 4 != 0 {
                        s.push_str("Alt+");
                    }
                    if let Some(ch) = std::char::from_u32(b.code)
                        && ch.is_ascii_graphic()
                    {
                        s.push(ch.to_ascii_uppercase());
                        return s;
                    }
                    match b.code {
                        0x100 => {
                            s.push_str("ArrowUp");
                        }
                        0x101 => {
                            s.push_str("ArrowDown");
                        }
                        0x102 => {
                            s.push_str("ArrowLeft");
                        }
                        0x103 => {
                            s.push_str("ArrowRight");
                        }
                        _ => {
                            s.push_str("Unknown");
                        }
                    }
                    s
                };

                // Use fixed-width layout for right-aligned inputs
                let label_width = 150.0;

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(label_width, 28.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label("Move Forward:");
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let id = 0usize;
                        let mut label = binding_label(&self.staged.key_w);
                        if self.listening == Some(id) {
                            label = "Press any key...".to_string();
                        }
                        let btn =
                            ui.add(egui::Button::new(label).min_size(egui::vec2(120.0, 28.0)));
                        Self::paint_dirty_decor(ui, &btn, self.staged.key_w != self.prefs.key_w);
                        if btn.clicked() {
                            self.listening = Some(id);
                        }
                        self.dirty_key_w = self.staged.key_w != self.prefs.key_w;
                    });
                });

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(label_width, 28.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label("Move Left:");
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let id = 1usize;
                        let mut label = binding_label(&self.staged.key_a);
                        if self.listening == Some(id) {
                            label = "Press any key...".to_string();
                        }
                        let btn =
                            ui.add(egui::Button::new(label).min_size(egui::vec2(120.0, 28.0)));
                        Self::paint_dirty_decor(ui, &btn, self.staged.key_a != self.prefs.key_a);
                        if btn.clicked() {
                            self.listening = Some(id);
                        }
                        self.dirty_key_a = self.staged.key_a != self.prefs.key_a;
                    });
                });

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(label_width, 28.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label("Move Back:");
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let id = 2usize;
                        let mut label = binding_label(&self.staged.key_s);
                        if self.listening == Some(id) {
                            label = "Press any key...".to_string();
                        }
                        let btn =
                            ui.add(egui::Button::new(label).min_size(egui::vec2(120.0, 28.0)));
                        Self::paint_dirty_decor(ui, &btn, self.staged.key_s != self.prefs.key_s);
                        if btn.clicked() {
                            self.listening = Some(id);
                        }
                        self.dirty_key_s = self.staged.key_s != self.prefs.key_s;
                    });
                });

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(label_width, 28.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label("Move Right:");
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let id = 3usize;
                        let mut label = binding_label(&self.staged.key_d);
                        if self.listening == Some(id) {
                            label = "Press any key...".to_string();
                        }
                        let btn =
                            ui.add(egui::Button::new(label).min_size(egui::vec2(120.0, 28.0)));
                        Self::paint_dirty_decor(ui, &btn, self.staged.key_d != self.prefs.key_d);
                        if btn.clicked() {
                            self.listening = Some(id);
                        }
                        self.dirty_key_d = self.staged.key_d != self.prefs.key_d;
                    });
                });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(label_width, 28.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label("Mouse Sensitivity:");
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Slider matching keybind button width (120px) - added first so it appears on the right
                        ui.allocate_ui_with_layout(
                            egui::vec2(120.0, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
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
                                    (self.staged.mouse_sensitivity - self.prefs.mouse_sensitivity)
                                        .abs()
                                        > f32::EPSILON,
                                );
                            },
                        );
                        ui.add_space(8.0);
                        // Drag value input - added second so it appears on the left
                        let drag = ui.add(
                            egui::DragValue::new(&mut self.staged.mouse_sensitivity)
                                .range(0.01..=10.0)
                                .speed(0.1)
                                .min_decimals(2)
                                .max_decimals(2),
                        );
                        Self::paint_dirty_decor(
                            ui,
                            &drag,
                            (self.staged.mouse_sensitivity - self.prefs.mouse_sensitivity).abs()
                                > f32::EPSILON,
                        );
                        self.dirty_mouse_sens =
                            (self.staged.mouse_sensitivity - self.prefs.mouse_sensitivity).abs()
                                > f32::EPSILON;
                    });
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Toggle filtering on/off
                        let checkbox =
                            ui.checkbox(&mut self.staged.input_filtering_enabled, "Enable");
                        Self::paint_dirty_decor(
                            ui,
                            &checkbox,
                            self.staged.input_filtering_enabled
                                != self.prefs.input_filtering_enabled,
                        );
                        self.dirty_filtering_enabled = self.staged.input_filtering_enabled
                            != self.prefs.input_filtering_enabled;
                    });
                });

                ui.add_space(12.0);

                egui::TopBottomPanel::bottom("settings_bottom").show(ctx, |ui| {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let save = ui.add(
                                egui::Button::new("Save Changes").min_size(egui::vec2(120.0, 36.0)),
                            );
                            let save_clicked = save.clicked();
                            if save_clicked {
                                // commit staged to prefs and save
                                self.prefs = self.staged.clone();
                                let _ = self.prefs.save();
                                // clear dirty flags
                                self.dirty_key_w = false;
                                self.dirty_key_a = false;
                                self.dirty_key_s = false;
                                self.dirty_key_d = false;
                                self.dirty_mouse_sens = false;
                                self.dirty_filtering_enabled = false;
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
                                let cancel = ui.add(
                                    egui::Button::new("Cancel").min_size(egui::vec2(100.0, 36.0)),
                                );
                                let cancel_clicked = cancel.clicked();
                                if cancel_clicked {
                                    // revert staged values to last saved prefs
                                    self.staged = self.prefs.clone();
                                    self.dirty_key_w = false;
                                    self.dirty_key_a = false;
                                    self.dirty_key_s = false;
                                    self.dirty_key_d = false;
                                    self.dirty_mouse_sens = false;
                                }
                                items.push(crate::menus::menu::MenuItem {
                                    action: MenuAction::None,
                                    rect: Some(cancel.rect),
                                    enabled: true,
                                    clicked: cancel_clicked,
                                });
                            } else {
                                let back = ui.add(
                                    egui::Button::new("Back").min_size(egui::vec2(100.0, 36.0)),
                                );
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
            ctx.input(|input| {
                for ev in &input.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = ev
                    {
                        // Escape cancels listening
                        if *key == egui::Key::Escape {
                            self.listening = None;
                            return;
                        }
                        // derive code and modifiers
                        let code: u32 = key_to_code(key);
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

                        let binding = Binding::new(code, mods);

                        // Check for duplicate bindings
                        let mut conflicting_id: Option<usize> = None;
                        if binding.code != 0 {
                            // Don't check unbound keys
                            if self.staged.key_w == binding && listen_id != 0 {
                                conflicting_id = Some(0);
                            } else if self.staged.key_a == binding && listen_id != 1 {
                                conflicting_id = Some(1);
                            } else if self.staged.key_s == binding && listen_id != 2 {
                                conflicting_id = Some(2);
                            } else if self.staged.key_d == binding && listen_id != 3 {
                                conflicting_id = Some(3);
                            }
                        }

                        if let Some(conflict_id) = conflicting_id {
                            // Show conflict modal
                            self.pending_binding = Some(PendingBinding {
                                target_id: listen_id,
                                binding,
                                conflicting_id: Some(conflict_id),
                            });
                            self.conflict_key_name = self.get_key_name(conflict_id).to_string();
                            self.conflict_binding_desc = Self::binding_label(&binding);
                            self.show_conflict_modal = true;
                        } else {
                            // No conflict, apply immediately
                            match listen_id {
                                0 => {
                                    self.staged.key_w = binding;
                                    self.dirty_key_w = true;
                                }
                                1 => {
                                    self.staged.key_a = binding;
                                    self.dirty_key_a = true;
                                }
                                2 => {
                                    self.staged.key_s = binding;
                                    self.dirty_key_s = true;
                                }
                                3 => {
                                    self.staged.key_d = binding;
                                    self.dirty_key_d = true;
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
