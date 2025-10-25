use crate::menus::WorldSpec;
use crate::menus::menu::{Menu, MenuAction, MenuItem};

pub struct NewWorldMenu {
    spec: crate::menus::menu::MenuSpec,
    name_input: String,
    seed_input: String,
    size_xz: u32,
}

impl NewWorldMenu {
    pub fn new() -> Self {
        Self {
            spec: crate::menus::menu::MenuSpec::default(),
            name_input: String::from("New World"),
            seed_input: String::new(),
            size_xz: 128,
        }
    }
}

impl Default for NewWorldMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu for NewWorldMenu {
    fn name(&self) -> &str {
        "new_world"
    }

    fn spec(&self) -> &crate::menus::menu::MenuSpec {
        &self.spec
    }

    fn ui(&mut self, ctx: &egui::Context) -> Vec<MenuItem> {
        let mut items = Vec::new();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Create New World");
                ui.add_space(8.0);

                ui.label("World Name");
                ui.text_edit_singleline(&mut self.name_input);
                ui.add_space(6.0);

                ui.label("Seed (optional)");
                ui.text_edit_singleline(&mut self.seed_input);
                ui.add_space(6.0);

                ui.label("World Size");
                ui.add(egui::Slider::new(&mut self.size_xz, 64..=256).text("size"));
                ui.add_space(12.0);

                // Buttons
                ui.horizontal(|ui| {
                    let cont =
                        ui.add(egui::Button::new("Continue").min_size(egui::vec2(120.0, 28.0)));
                    let cancel =
                        ui.add(egui::Button::new("Back").min_size(egui::vec2(120.0, 28.0)));

                    if cont.clicked() {
                        // Sanitize inputs
                        let name = self.name_input.trim().to_string();
                        let name = if name.is_empty() {
                            String::from("New World")
                        } else {
                            name.chars().take(64).collect()
                        };

                        let seed = match self.seed_input.trim() {
                            "" => None,
                            s => s.parse::<u64>().ok(),
                        };

                        let spec = WorldSpec {
                            name,
                            seed,
                            size_xz: self.size_xz,
                        };
                        items.push(MenuItem {
                            action: MenuAction::GenerateWorld(spec),
                            rect: None,
                            enabled: true,
                            clicked: true,
                        });
                    }

                    if cancel.clicked() {
                        // Return to the start menu instead of closing the whole UI.
                        items.push(MenuItem {
                            action: MenuAction::ShowMenu("start".to_string()),
                            rect: None,
                            enabled: true,
                            clicked: true,
                        });
                    }
                });
            });
        });

        items
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
