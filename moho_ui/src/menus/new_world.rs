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
                ui.add_space(20.0);

                // Center the card horizontally with symmetric gutters
                let form_width = 520.0_f32;
                // Field width for text fields and slider - using the same layout
                // strategy as settings.rs for consistent behavior
                let text_field_width: f32 = 272.0;
                let available_width = ui.available_width();
                let gutter = (available_width - form_width).max(0.0) / 2.0;

                let avail_h = ui.available_height();
                let panel_h = avail_h.min(420.0); // cap panel height so it doesn't stretch too far

                ui.allocate_ui_with_layout(
                    egui::vec2(available_width, panel_h),
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| {
                        ui.add_space(gutter);

                        // Wrap the form in a card-like frame for a clean panel appearance.
                        egui::Frame::group(&ctx.style())
                            .rounding(egui::Rounding::same(8))
                            .inner_margin(egui::Margin::symmetric(16, 10))
                            .show(ui, |ui| {
                                ui.set_width(form_width);

                                // Use a vertical layout with top content and bottom buttons
                                ui.vertical(|ui| {
                                    ui.add_space(8.0);

                                    // Use the same layout strategy as settings.rs for consistent field widths
                                    let label_width = 120.0;
                                    let field_width = text_field_width;

                                    // World Name
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(label_width, 0.0),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.label("World Name");
                                            },
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::TextEdit::singleline(&mut self.name_input)
                                                        .desired_width(field_width),
                                                );
                                            },
                                        );
                                    });

                                    ui.add_space(8.0);

                                    // Seed (optional)
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(label_width, 0.0),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.label("Seed (optional)");
                                            },
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::TextEdit::singleline(&mut self.seed_input)
                                                        .desired_width(field_width),
                                                );
                                            },
                                        );
                                    });

                                    ui.add_space(8.0);

                                    // World Size
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(label_width, 0.0),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.label("World Size");
                                            },
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label("size");
                                                ui.label(
                                                    egui::RichText::new(format!("{}", self.size_xz))
                                                        .monospace(),
                                                );
                                                ui.add_space(8.0);
                                                ui.add(
                                                    egui::Slider::new(&mut self.size_xz, 64..=256)
                                                        .show_value(false),
                                                )
                                                .on_hover_text(format!("World size: {}", self.size_xz));
                                            },
                                        );
                                    });

                                    ui.add_space(12.0);

                                    // Push the buttons to the bottom of the card by using a
                                    // bottom-up layout. This keeps them roughly where the
                                    // red guideline indicates regardless of panel height.
                                    ui.with_layout(
                                        egui::Layout::bottom_up(egui::Align::Min),
                                        |ui| {
                                            // Add a larger spacer so the buttons sit lower in the card
                                            // (closer to the red guideline from the screenshot).
                                            ui.add_space(96.0);

                                            // Right-align the action buttons inside the bottom area
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    let cont = ui.add(
                                                        egui::Button::new("Continue")
                                                            .min_size(egui::vec2(120.0, 28.0)),
                                                    );
                                                    ui.add_space(12.0);
                                                    let cancel = ui.add(
                                                        egui::Button::new("Back")
                                                            .min_size(egui::vec2(120.0, 28.0)),
                                                    );

                                                    if cont.clicked() {
                                                        // Sanitize inputs
                                                        let name =
                                                            self.name_input.trim().to_string();
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
                                                        items.push(MenuItem {
                                                            action: MenuAction::ShowMenu(
                                                                "start".to_string(),
                                                            ),
                                                            rect: None,
                                                            enabled: true,
                                                            clicked: true,
                                                        });
                                                    }
                                                },
                                            ); // end right_to_left
                                        },
                                    ); // end bottom_up
                                }); // end ui.vertical
                            }); // end frame.show
                    },
                );
            }); // vertical_centered
        }); // CentralPanel

        items
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
