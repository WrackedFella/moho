/// Renders the Controls tab content.
///
/// This includes keybind settings, mouse sensitivity, and input filtering options.
use super::key_mapping::binding_label;
use super::types::{BINDING_ROWS, display_name};
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

        for (id, action) in BINDING_ROWS.into_iter().enumerate() {
            let is_dirty = menu.state.is_binding_modified(action);
            let label = binding_label(menu.state.get_staged_binding(action));
            let clicked = FormControls::keybind_control(
                ui,
                &format!("{}:", display_name(action)),
                &label,
                is_dirty,
                menu.is_listening_for(id),
                label_width,
            );
            if clicked {
                menu.start_listening(id);
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

            let saved_sensitivity = menu.state.prefs().mouse_sensitivity();
            let is_dirty =
                (menu.state.staged().mouse_sensitivity() - saved_sensitivity).abs() > f32::EPSILON;

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Slider matching keybind button width (120px)
                ui.allocate_ui_with_layout(
                    egui::vec2(120.0, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().slider_width = 120.0;
                        let slider = ui.add(
                            egui::Slider::new(
                                menu.state.staged_mut().mouse_sensitivity_mut(),
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
                    egui::DragValue::new(menu.state.staged_mut().mouse_sensitivity_mut())
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
            let saved_filtering = menu.state.prefs().input_filtering_enabled();
            let is_dirty = menu.state.staged().input_filtering_enabled() != saved_filtering;

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
                        menu.state.staged_mut().input_filtering_enabled_mut(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiComponent;
    use crate::prefs::Prefs;

    fn render_menu(menu: &mut SettingsMenu) -> egui::FullOutput {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            menu.render(ui);
        })
    }

    fn has_dirty_decor(output: &egui::FullOutput) -> bool {
        fn find(shape: &egui::epaint::Shape) -> bool {
            match shape {
                egui::epaint::Shape::Rect(r) => {
                    r.fill == egui::Color32::from_rgba_premultiplied(150, 150, 150, 60)
                }
                egui::epaint::Shape::Vec(v) => v.iter().any(find),
                _ => false,
            }
        }
        output.shapes.iter().any(|c| find(&c.shape))
    }

    #[test]
    fn changed_mouse_sensitivity_is_dirty_and_decorated() {
        let mut menu = SettingsMenu::with_prefs(Prefs::default());
        *menu.state.staged_mut().mouse_sensitivity_mut() = 2.5;

        let output = render_menu(&mut menu);

        assert!(menu.has_unsaved_changes());
        assert!(has_dirty_decor(&output));
    }

    #[test]
    fn untouched_controls_are_not_dirty_or_decorated() {
        let mut menu = SettingsMenu::with_prefs(Prefs::default());

        let output = render_menu(&mut menu);

        assert!(!menu.has_unsaved_changes());
        assert!(!has_dirty_decor(&output));
    }

    #[test]
    fn mouse_sensitivity_difference_of_one_epsilon_is_not_dirty() {
        let mut menu = SettingsMenu::with_prefs(Prefs::default());
        *menu.state.staged_mut().mouse_sensitivity_mut() += f32::EPSILON;

        render_menu(&mut menu);

        assert!(!menu.has_unsaved_changes());
    }
}
