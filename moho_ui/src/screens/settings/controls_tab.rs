/// Renders the Controls tab content.
///
/// This includes keybind settings, mouse sensitivity, and input filtering options.
use super::key_mapping::binding_label;
use super::{FormControls, SettingsField, SettingsMenu};

pub fn render(menu: &mut SettingsMenu, ui: &mut egui::Ui) {
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
            let is_dirty = menu.state.is_binding_modified(SettingsField::KeyW);
            let binding = menu.state.get_staged_binding(SettingsField::KeyW);
            let clicked = FormControls::keybind_control(
                ui,
                "Move Forward:",
                &binding_label(&binding),
                is_dirty,
                menu.is_listening_for(0),
                label_width,
            );
            if clicked {
                menu.start_listening(0);
            }
        }

        // Move Left
        {
            let is_dirty = menu.state.is_binding_modified(SettingsField::KeyA);
            let binding = menu.state.get_staged_binding(SettingsField::KeyA);
            let clicked = FormControls::keybind_control(
                ui,
                "Move Left:",
                &binding_label(&binding),
                is_dirty,
                menu.is_listening_for(1),
                label_width,
            );
            if clicked {
                menu.start_listening(1);
            }
        }

        // Move Back
        {
            let is_dirty = menu.state.is_binding_modified(SettingsField::KeyS);
            let binding = menu.state.get_staged_binding(SettingsField::KeyS);
            let clicked = FormControls::keybind_control(
                ui,
                "Move Back:",
                &binding_label(&binding),
                is_dirty,
                menu.is_listening_for(2),
                label_width,
            );
            if clicked {
                menu.start_listening(2);
            }
        }

        // Move Right
        {
            let is_dirty = menu.state.is_binding_modified(SettingsField::KeyD);
            let binding = menu.state.get_staged_binding(SettingsField::KeyD);
            let clicked = FormControls::keybind_control(
                ui,
                "Move Right:",
                &binding_label(&binding),
                is_dirty,
                menu.is_listening_for(3),
                label_width,
            );
            if clicked {
                menu.start_listening(3);
            }
        }

        // Move Up
        {
            let is_dirty = menu.state.is_binding_modified(SettingsField::KeyUp);
            let binding = menu.state.get_staged_binding(SettingsField::KeyUp);
            let clicked = FormControls::keybind_control(
                ui,
                "Move Up:",
                &binding_label(&binding),
                is_dirty,
                menu.is_listening_for(4),
                label_width,
            );
            if clicked {
                menu.start_listening(4);
            }
        }

        // Move Down
        {
            let is_dirty = menu.state.is_binding_modified(SettingsField::KeyDown);
            let binding = menu.state.get_staged_binding(SettingsField::KeyDown);
            let clicked = FormControls::keybind_control(
                ui,
                "Move Down:",
                &binding_label(&binding),
                is_dirty,
                menu.is_listening_for(5),
                label_width,
            );
            if clicked {
                menu.start_listening(5);
            }
        }

        ui.add_space(8.0);

        // Mouse Sensitivity
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label("Mouse Sensitivity:");
                },
            );

            let saved_sensitivity = menu.state.prefs().mouse_sensitivity;
            let is_dirty =
                (menu.state.staged().mouse_sensitivity - saved_sensitivity).abs() > f32::EPSILON;

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Slider matching keybind button width (120px)
                ui.allocate_ui_with_layout(
                    egui::vec2(120.0, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().slider_width = 120.0;
                        let slider = ui.add(
                            egui::Slider::new(
                                &mut menu.state.staged_mut().mouse_sensitivity,
                                0.01..=10.0,
                            )
                            .show_value(false)
                            .min_decimals(0)
                            .max_decimals(2),
                        );
                        SettingsMenu::paint_dirty_decor(ui, &slider, is_dirty);
                    },
                );
                ui.add_space(8.0);
                // Drag value input
                let drag = ui.add(
                    egui::DragValue::new(&mut menu.state.staged_mut().mouse_sensitivity)
                        .range(0.01..=10.0)
                        .speed(0.1)
                        .min_decimals(2)
                        .max_decimals(2),
                );
                SettingsMenu::paint_dirty_decor(ui, &drag, is_dirty);
            });

            if is_dirty {
                menu.state.mark_dirty(SettingsField::MouseSensitivity);
            }
        });

        ui.add_space(12.0);

        // Input Filtering
        {
            let saved_filtering = menu.state.prefs().input_filtering_enabled;
            let is_dirty = menu.state.staged().input_filtering_enabled != saved_filtering;

            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(label_width, 28.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.label("Input Filtering:");
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let checkbox = ui.checkbox(
                        &mut menu.state.staged_mut().input_filtering_enabled,
                        "Enable",
                    );
                    SettingsMenu::paint_dirty_decor(ui, &checkbox, is_dirty);
                });
            });

            if is_dirty {
                menu.state.mark_dirty(SettingsField::InputFiltering);
            }
        }
    });
}
