use crate::menus::WorldSpec;
use crate::menus::menu::{Menu, MenuAction, MenuItem, MenuSpec};

pub struct NewWorldMenu {
    spec: MenuSpec,
    name_input: String,
    seed_input: String,
    size_xz: u32,
}

impl NewWorldMenu {
    pub fn new() -> Self {
        Self {
            spec: MenuSpec::default(),
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

    fn spec(&self) -> &MenuSpec {
        &self.spec
    }

    fn ui(&mut self, ctx: &egui::Context) -> Vec<MenuItem> {
        let mut items: Vec<MenuItem> = Vec::new();

        // Top panel: title area
        egui::TopBottomPanel::top("new_world_top").show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                ui.heading("Create New World");
                ui.add_space(8.0);
            });
        });

        // Bottom panel: buttons reserved at the bottom so they're snug
        egui::TopBottomPanel::bottom("new_world_bottom").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cont = ui.add(egui::Button::new("Continue").min_size(egui::vec2(120.0, 36.0)));
                    ui.add_space(12.0);
                    let cancel = ui.add(egui::Button::new("Back").min_size(egui::vec2(120.0, 36.0)));

                    if cont.clicked() {
                        let name = self.name_input.trim().to_string();
                        let name = if name.is_empty() { String::from("New World") } else { name.chars().take(64).collect() };
                        let seed = match self.seed_input.trim() { "" => None, s => s.parse::<u64>().ok() };
                        let spec = WorldSpec { name, seed, size_xz: self.size_xz };
                        items.push(MenuItem { action: MenuAction::GenerateWorld(spec), rect: Some(cont.rect), enabled: true, clicked: true });
                    }

                    if cancel.clicked() {
                        items.push(MenuItem { action: MenuAction::ShowMenu("start".to_string()), rect: Some(cancel.rect), enabled: true, clicked: true });
                    }
                });
            });
        });

        // Central panel: form fields inside a scroll area
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let gutter: f32 = 20.0;
                let avail = ui.available_width();
                ui.horizontal(|ui| {
                    ui.add_space(gutter);
                    ui.allocate_ui_with_layout(egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.vertical_centered(|ui| {
                            // field sizing same as settings
                            let label_width: f32 = 150.0;
                            let field_width: f32 = 272.0;

                            // World Name
                            ui.horizontal(|ui| {
                                ui.allocate_ui_with_layout(egui::vec2(label_width, 0.0), egui::Layout::left_to_right(egui::Align::Center), |ui| { ui.label("World Name"); });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add(egui::TextEdit::singleline(&mut self.name_input).desired_width(field_width));
                                });
                            });

                            ui.add_space(8.0);

                            // Seed
                            ui.horizontal(|ui| {
                                ui.allocate_ui_with_layout(egui::vec2(label_width, 0.0), egui::Layout::left_to_right(egui::Align::Center), |ui| { ui.label("Seed (optional)"); });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add(egui::TextEdit::singleline(&mut self.seed_input).desired_width(field_width));
                                });
                            });

                            ui.add_space(8.0);

                            // World Size - put slider in the same width as fields
                            ui.horizontal(|ui| {
                                ui.allocate_ui_with_layout(egui::vec2(label_width, 0.0), egui::Layout::left_to_right(egui::Align::Center), |ui| { ui.label("World Size"); });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label("size");
                                    ui.label(egui::RichText::new(format!("{}", self.size_xz)).monospace());
                                    ui.add_space(8.0);
                                    ui.add_sized(egui::vec2(field_width, 20.0), egui::Slider::new(&mut self.size_xz, 64..=256));
                                });
                            });

                            ui.add_space(12.0);
                        });
                    });
                    ui.add_space(gutter);
                });
            });
        });

        items
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

}
