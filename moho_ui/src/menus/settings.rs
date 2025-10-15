use crate::menus::menu::{Menu, MenuAction, MenuSpec};
use crate::prefs::Prefs;

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
        }
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

                ui.horizontal(|ui| {
                    ui.label("Move Forward:");
                    let w_resp = ui.add(egui::TextEdit::singleline(&mut self.staged.key_w).desired_width(80.0));
                    Self::paint_dirty_decor(ui, &w_resp, self.staged.key_w != self.prefs.key_w);
                    self.dirty_key_w = self.staged.key_w != self.prefs.key_w;
                });

                ui.horizontal(|ui| {
                    ui.label("Move Left:");
                    let a_resp = ui.add(egui::TextEdit::singleline(&mut self.staged.key_a).desired_width(80.0));
                    Self::paint_dirty_decor(ui, &a_resp, self.staged.key_a != self.prefs.key_a);
                    self.dirty_key_a = self.staged.key_a != self.prefs.key_a;
                });

                ui.horizontal(|ui| {
                    ui.label("Move Back:");
                    let s_resp = ui.add(egui::TextEdit::singleline(&mut self.staged.key_s).desired_width(80.0));
                    Self::paint_dirty_decor(ui, &s_resp, self.staged.key_s != self.prefs.key_s);
                    self.dirty_key_s = self.staged.key_s != self.prefs.key_s;
                });

                ui.horizontal(|ui| {
                    ui.label("Move Right:");
                    let d_resp = ui.add(egui::TextEdit::singleline(&mut self.staged.key_d).desired_width(80.0));
                    Self::paint_dirty_decor(ui, &d_resp, self.staged.key_d != self.prefs.key_d);
                    self.dirty_key_d = self.staged.key_d != self.prefs.key_d;
                });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Mouse Sensitivity:");
                    let mut sens_str = format!("{}", self.staged.mouse_sensitivity);
                    let sens_resp = ui.add(egui::TextEdit::singleline(&mut sens_str).desired_width(120.0));
                    // if changed, try parse
                    if sens_resp.changed() {
                        if let Ok(v) = sens_str.parse::<f32>() {
                            self.staged.mouse_sensitivity = v;
                        }
                    }
                    Self::paint_dirty_decor(ui, &sens_resp, (self.staged.mouse_sensitivity - self.prefs.mouse_sensitivity).abs() > f32::EPSILON);
                    self.dirty_mouse_sens = (self.staged.mouse_sensitivity - self.prefs.mouse_sensitivity).abs() > f32::EPSILON;
                });

                ui.add_space(12.0);

                egui::TopBottomPanel::bottom("settings_bottom").show(ctx, |ui| {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let save = ui.add(egui::Button::new("Save Changes").min_size(egui::vec2(120.0, 36.0)));
                            let save_clicked = save.clicked();
                            if save_clicked {
                                // commit staged to prefs and save
                                self.prefs = self.staged.clone();
                                let _ = self.prefs.save();
                            }
                            items.push(crate::menus::menu::MenuItem {
                                action: if save_clicked { MenuAction::ShowMenu("start".to_string()) } else { MenuAction::None },
                                rect: Some(save.rect),
                                enabled: true,
                                clicked: save_clicked,
                            });

                            ui.add_space(8.0);

                            let cancel = ui.add(egui::Button::new("Cancel").min_size(egui::vec2(100.0, 36.0)));
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
                                action: if cancel_clicked { MenuAction::ShowMenu("start".to_string()) } else { MenuAction::None },
                                rect: Some(cancel.rect),
                                enabled: true,
                                clicked: cancel_clicked,
                            });

                            ui.add_space(8.0);
                        });
                    });
                });
            });
        });

        items
    }
}
