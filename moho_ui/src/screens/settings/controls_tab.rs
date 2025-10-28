/// Renders the Controls tab content.
///
/// This includes keybind settings, mouse sensitivity, and input filtering options.
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
            let is_dirty = menu.staged.key_w != menu.prefs.key_w;
            let clicked = FormControls::keybind_control(
                ui,
                "Move Forward:",
                &SettingsMenu::binding_label(&menu.staged.key_w),
                is_dirty,
                menu.listening == Some(0),
                label_width,
            );
            if is_dirty {
                menu.dirty_fields.insert(SettingsField::KeyW);
            } else {
                menu.dirty_fields.remove(&SettingsField::KeyW);
            }
            if clicked {
                menu.listening = Some(0);
            }
        }

        // Move Left
        {
            let is_dirty = menu.staged.key_a != menu.prefs.key_a;
            let clicked = FormControls::keybind_control(
                ui,
                "Move Left:",
                &SettingsMenu::binding_label(&menu.staged.key_a),
                is_dirty,
                menu.listening == Some(1),
                label_width,
            );
            if is_dirty {
                menu.dirty_fields.insert(SettingsField::KeyA);
            } else {
                menu.dirty_fields.remove(&SettingsField::KeyA);
            }
            if clicked {
                menu.listening = Some(1);
            }
        }

        // Move Back
        {
            let is_dirty = menu.staged.key_s != menu.prefs.key_s;
            let clicked = FormControls::keybind_control(
                ui,
                "Move Back:",
                &SettingsMenu::binding_label(&menu.staged.key_s),
                is_dirty,
                menu.listening == Some(2),
                label_width,
            );
            if is_dirty {
                menu.dirty_fields.insert(SettingsField::KeyS);
            } else {
                menu.dirty_fields.remove(&SettingsField::KeyS);
            }
            if clicked {
                menu.listening = Some(2);
            }
        }

        // Move Right
        {
            let is_dirty = menu.staged.key_d != menu.prefs.key_d;
            let clicked = FormControls::keybind_control(
                ui,
                "Move Right:",
                &SettingsMenu::binding_label(&menu.staged.key_d),
                is_dirty,
                menu.listening == Some(3),
                label_width,
            );
            if is_dirty {
                menu.dirty_fields.insert(SettingsField::KeyD);
            } else {
                menu.dirty_fields.remove(&SettingsField::KeyD);
            }
            if clicked {
                menu.listening = Some(3);
            }
        }

        // Move Up
        {
            let is_dirty = menu.staged.key_up != menu.prefs.key_up;
            let clicked = FormControls::keybind_control(
                ui,
                "Move Up:",
                &SettingsMenu::binding_label(&menu.staged.key_up),
                is_dirty,
                menu.listening == Some(4),
                label_width,
            );
            if is_dirty {
                menu.dirty_fields.insert(SettingsField::KeyUp);
            } else {
                menu.dirty_fields.remove(&SettingsField::KeyUp);
            }
            if clicked {
                menu.listening = Some(4);
            }
        }

        // Move Down
        {
            let is_dirty = menu.staged.key_down != menu.prefs.key_down;
            let clicked = FormControls::keybind_control(
                ui,
                "Move Down:",
                &SettingsMenu::binding_label(&menu.staged.key_down),
                is_dirty,
                menu.listening == Some(5),
                label_width,
            );
            if is_dirty {
                menu.dirty_fields.insert(SettingsField::KeyDown);
            } else {
                menu.dirty_fields.remove(&SettingsField::KeyDown);
            }
            if clicked {
                menu.listening = Some(5);
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Slider matching keybind button width (120px)
                ui.allocate_ui_with_layout(
                    egui::vec2(120.0, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().slider_width = 120.0;
                        let slider = ui.add(
                            egui::Slider::new(&mut menu.staged.mouse_sensitivity, 0.01..=10.0)
                                .show_value(false)
                                .min_decimals(0)
                                .max_decimals(2),
                        );
                        SettingsMenu::paint_dirty_decor(
                            ui,
                            &slider,
                            (menu.staged.mouse_sensitivity - menu.prefs.mouse_sensitivity).abs()
                                > f32::EPSILON,
                        );
                    },
                );
                ui.add_space(8.0);
                // Drag value input
                let drag = ui.add(
                    egui::DragValue::new(&mut menu.staged.mouse_sensitivity)
                        .range(0.01..=10.0)
                        .speed(0.1)
                        .min_decimals(2)
                        .max_decimals(2),
                );
                SettingsMenu::paint_dirty_decor(
                    ui,
                    &drag,
                    (menu.staged.mouse_sensitivity - menu.prefs.mouse_sensitivity).abs()
                        > f32::EPSILON,
                );
                if (menu.staged.mouse_sensitivity - menu.prefs.mouse_sensitivity).abs()
                    > f32::EPSILON
                {
                    menu.dirty_fields.insert(SettingsField::MouseSensitivity);
                } else {
                    menu.dirty_fields.remove(&SettingsField::MouseSensitivity);
                }
            });
        });

        ui.add_space(12.0);

        // Input Filtering
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label("Input Filtering:");
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let checkbox = ui.checkbox(&mut menu.staged.input_filtering_enabled, "Enable");
                SettingsMenu::paint_dirty_decor(
                    ui,
                    &checkbox,
                    menu.staged.input_filtering_enabled != menu.prefs.input_filtering_enabled,
                );
                if menu.staged.input_filtering_enabled != menu.prefs.input_filtering_enabled {
                    menu.dirty_fields.insert(SettingsField::InputFiltering);
                } else {
                    menu.dirty_fields.remove(&SettingsField::InputFiltering);
                }
            });
        });
    });
}
