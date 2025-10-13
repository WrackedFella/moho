use crate::menus::menu::{Menu, MenuAction, MenuSpec};

pub struct SettingsMenu {
    spec: MenuSpec,
}

impl SettingsMenu {
    pub fn new() -> Self {
        Self {
            spec: MenuSpec::default(),
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

                // Title
                ui.heading("Game Settings");
                ui.add_space(32.0);

                // Future: Settings content will go here
                ui.label("Settings will be implemented here in future iterations.");
                ui.add_space(32.0);

                // Bottom-right buttons in their own bottom panel so they stay pinned
                egui::TopBottomPanel::bottom("settings_bottom").show(ctx, |ui| {
                    // Make the panel relatively short and give a little padding
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Use slightly taller buttons so text is vertically centered
                            let save = ui.add(
                                egui::Button::new("Save Changes").min_size(egui::vec2(120.0, 36.0)),
                            );
                            let save_clicked = save.clicked();
                            items.push(crate::menus::menu::MenuItem {
                                action: MenuAction::ShowMenu("start".to_string()), // For now, just go back to start
                                rect: Some(save.rect),
                                enabled: true,
                                clicked: save_clicked,
                            });

                            ui.add_space(8.0);

                            let cancel = ui
                                .add(egui::Button::new("Cancel").min_size(egui::vec2(100.0, 36.0)));
                            let cancel_clicked = cancel.clicked();
                            items.push(crate::menus::menu::MenuItem {
                                action: MenuAction::ShowMenu("start".to_string()), // Go back to start menu
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
