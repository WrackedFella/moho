use crate::menus::menu::{Menu, MenuAction, ScreenSpec};
use std::path::PathBuf;

pub struct StartMenu {
    spec: ScreenSpec,
}

impl StartMenu {
    pub fn new() -> Self {
        Self {
            spec: ScreenSpec::default(),
        }
    }
}

impl Default for StartMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu for StartMenu {
    fn name(&self) -> &str {
        "start"
    }
    fn spec(&self) -> &ScreenSpec {
        &self.spec
    }
    fn ui(&mut self, ctx: &egui::Context) -> Vec<crate::menus::menu::MenuItem> {
        let mut items: Vec<crate::menus::menu::MenuItem> = Vec::new();

        // Determine whether a saved scene exists
        let save_path = PathBuf::from("saves/scene.bin");
        let save_exists = std::path::Path::new(&save_path).exists();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.heading("Moho");
                ui.add_space(12.0);

                // Helper to paint hover/focus styling around a rect
                let paint_decor = |ui: &mut egui::Ui, resp: &egui::Response| {
                    if resp.hovered() {
                        let r = resp.rect;
                        let hover_color =
                            egui::Color32::from_rgba_premultiplied(150, 150, 150, 100); // Light grey highlight
                        ui.painter().rect_filled(r, 4.0, hover_color);
                    }
                };

                // Continue button (disabled when no save exists)
                let cont = ui.add_enabled(
                    save_exists,
                    egui::Button::new("Continue").min_size(egui::vec2(160.0, 28.0)),
                );
                paint_decor(ui, &cont);
                let cont_clicked = cont.clicked() && save_exists;
                items.push(crate::menus::menu::MenuItem {
                    action: MenuAction::LoadScene(save_path.clone()),
                    rect: Some(cont.rect),
                    enabled: save_exists,
                    clicked: cont_clicked,
                });
                ui.add_space(6.0);

                // New World button - always enabled
                let nw = ui.add(egui::Button::new("New World").min_size(egui::vec2(160.0, 28.0)));
                paint_decor(ui, &nw);
                let nw_clicked = nw.clicked();
                items.push(crate::menus::menu::MenuItem {
                    action: MenuAction::NewWorld,
                    rect: Some(nw.rect),
                    enabled: true,
                    clicked: nw_clicked,
                });
                ui.add_space(6.0);

                // Settings button
                let st = ui.add(egui::Button::new("Settings").min_size(egui::vec2(160.0, 28.0)));
                paint_decor(ui, &st);
                let st_clicked = st.clicked();
                items.push(crate::menus::menu::MenuItem {
                    action: MenuAction::ShowMenu("settings".to_string()),
                    rect: Some(st.rect),
                    enabled: true,
                    clicked: st_clicked,
                });
                ui.add_space(6.0);

                let ex = ui.add(egui::Button::new("Exit").min_size(egui::vec2(160.0, 28.0)));
                paint_decor(ui, &ex);
                let ex_clicked = ex.clicked();
                items.push(crate::menus::menu::MenuItem {
                    action: MenuAction::Exit,
                    rect: Some(ex.rect),
                    enabled: true,
                    clicked: ex_clicked,
                });
            });
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
