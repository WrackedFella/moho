use crate::modal::{Modal, ModalResult};

/// Modal that warns about keybind conflicts and asks for confirmation
pub struct KeybindConflictModal {
    conflicting_key_name: String,
    new_binding_description: String,
}

impl KeybindConflictModal {
    pub fn new(conflicting_key_name: String, new_binding_description: String) -> Self {
        Self {
            conflicting_key_name,
            new_binding_description,
        }
    }
}

impl Modal for KeybindConflictModal {
    fn title(&self) -> &str {
        "Keybind Conflict"
    }

    fn render(&mut self, ui: &mut egui::Ui) -> ModalResult {
        let mut result = ModalResult::None;

        ui.vertical(|ui| {
            ui.add_space(8.0);

            // Warning icon and message
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("⚠")
                        .size(32.0)
                        .color(egui::Color32::YELLOW),
                );
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("This key is already assigned!")
                            .strong()
                            .size(16.0),
                    );
                    ui.add_space(4.0);
                    ui.label(format!(
                        "The binding '{}' is currently used by:",
                        self.new_binding_description
                    ));
                    ui.label(
                        egui::RichText::new(&self.conflicting_key_name)
                            .strong()
                            .color(egui::Color32::LIGHT_BLUE),
                    );
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label("Do you want to reassign this key?");
            ui.label(
                egui::RichText::new(format!(
                    "'{}' will be unbound if you proceed.",
                    self.conflicting_key_name
                ))
                .italics()
                .color(egui::Color32::GRAY),
            );

            ui.add_space(12.0);

            // Buttons
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new("Proceed").min_size(egui::vec2(100.0, 32.0)))
                        .clicked()
                    {
                        result = ModalResult::Confirm;
                    }

                    ui.add_space(8.0);

                    if ui
                        .add(egui::Button::new("Cancel").min_size(egui::vec2(100.0, 32.0)))
                        .clicked()
                    {
                        result = ModalResult::Cancel;
                    }
                });
            });

            ui.add_space(8.0);
        });

        result
    }
}
