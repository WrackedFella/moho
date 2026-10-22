/// Renders the Controls tab content.
///
/// This includes keybind settings, mouse sensitivity, and input filtering options.
use super::key_mapping::binding_label;
use super::types::{BINDING_ROWS, display_name};
use super::{FormControls, SettingsField, SettingsMenu};
use crate::prefs::Prefs;
use moho_input::Action;
use moho_input::bindings::ActionMode;

const KEYBIND_BUTTON_WIDTH: f32 = 120.0;

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
            let is_listening = menu.is_listening_for(id);
            let keybind = |ui: &mut egui::Ui| {
                FormControls::keybind_control(
                    ui,
                    &format!("{}:", display_name(action)),
                    &label,
                    is_dirty,
                    is_listening,
                    label_width,
                )
            };
            let clicked = if action.toggleable() {
                ui.horizontal(|ui| {
                    // keybind_control fills the width it is given, so bound it to leave
                    // room for the checkbox on the same row.
                    let row_width =
                        label_width + KEYBIND_BUTTON_WIDTH + ui.spacing().item_spacing.x;
                    let clicked = ui.allocate_ui(egui::vec2(row_width, 28.0), keybind).inner;
                    let mut toggled = menu.state.get_staged_mode(action) == ActionMode::Toggle;
                    if ui.checkbox(&mut toggled, "Toggle").changed() {
                        let mode = if toggled {
                            ActionMode::Toggle
                        } else {
                            ActionMode::Hold
                        };
                        menu.state.set_staged_mode(action, mode);
                    }
                    clicked
                })
                .inner
            } else {
                keybind(ui)
            };
            if clicked {
                menu.start_listening(id);
            }
        }
        ui.add_space(8.0);

        let acceleration_on = menu.state.staged().mouse_acceleration();
        slider_row(
            menu,
            ui,
            label_width,
            true,
            &SliderRow {
                label: "Mouse Sensitivity:",
                field: SettingsField::MouseSensitivity,
                range: 0.01..=10.0,
                speed: 0.1,
                value: Prefs::mouse_sensitivity,
                value_mut: Prefs::mouse_sensitivity_mut,
            },
        );

        toggle_row(
            menu,
            ui,
            label_width,
            "Mouse acceleration:",
            SettingsField::MouseAcceleration,
            Prefs::mouse_acceleration,
            Prefs::mouse_acceleration_mut,
        );
        slider_row(
            menu,
            ui,
            label_width,
            acceleration_on,
            &SliderRow {
                label: "Acceleration strength:",
                field: SettingsField::MouseAccelerationStrength,
                range: 0.0..=1.0,
                speed: 0.01,
                value: Prefs::mouse_acceleration_strength,
                value_mut: Prefs::mouse_acceleration_strength_mut,
            },
        );

        ui.add_space(12.0);

        toggle_row(
            menu,
            ui,
            label_width,
            "Invert mouse Y:",
            SettingsField::InvertMouseY,
            Prefs::invert_mouse_y,
            Prefs::invert_mouse_y_mut,
        );
        toggle_row(
            menu,
            ui,
            label_width,
            "Mouse smoothing:",
            SettingsField::MouseSmoothing,
            Prefs::mouse_smoothing,
            Prefs::mouse_smoothing_mut,
        );
    });
}

/// One float pref's labelled slider and value box, decorated and marked dirty while it
/// differs from the saved value.
struct SliderRow {
    label: &'static str,
    field: SettingsField,
    range: std::ops::RangeInclusive<f32>,
    speed: f64,
    value: fn(&Prefs) -> f32,
    value_mut: fn(&mut Prefs) -> &mut f32,
}

fn slider_row(
    menu: &mut SettingsMenu,
    ui: &mut egui::Ui,
    label_width: f32,
    enabled: bool,
    row: &SliderRow,
) {
    let is_dirty =
        ((row.value)(menu.state.staged()) - (row.value)(menu.state.prefs())).abs() > f32::EPSILON;

    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(row.label);
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
                            egui::Slider::new(
                                (row.value_mut)(menu.state.staged_mut()),
                                row.range.clone(),
                            )
                            .show_value(false)
                            .min_decimals(0)
                            .max_decimals(2),
                        );
                        SettingsMenu::paint_dirty_decor(ui, &slider, is_dirty);
                    },
                );
                ui.add_space(8.0);
                let drag = ui.add(
                    egui::DragValue::new((row.value_mut)(menu.state.staged_mut()))
                        .range(row.range.clone())
                        .speed(row.speed)
                        .min_decimals(2)
                        .max_decimals(2),
                );
                SettingsMenu::paint_dirty_decor(ui, &drag, is_dirty);
            });
        });
    });

    if is_dirty {
        menu.state.mark_dirty(row.field);
    }
}

/// A labelled "Enable" checkbox for one boolean pref, decorated and marked dirty
/// while it differs from the saved value.
pub(super) fn toggle_row(
    menu: &mut SettingsMenu,
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    field: SettingsField,
    value: fn(&Prefs) -> bool,
    value_mut: fn(&mut Prefs) -> &mut bool,
) {
    let is_dirty = value(menu.state.staged()) != value(menu.state.prefs());

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(label);
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let checkbox = ui.checkbox(value_mut(menu.state.staged_mut()), "Enable");
            SettingsMenu::paint_dirty_decor(ui, &checkbox, is_dirty);
        });
    });

    if is_dirty {
        menu.state.mark_dirty(field);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiComponent;
    use crate::prefs::Prefs;

    fn render_menu(menu: &mut SettingsMenu) -> egui::FullOutput {
        let ctx = egui::Context::default();
        crate::overlays::test_support::run_ui(&ctx, egui::RawInput::default(), |ui| {
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

    fn has_text(output: &egui::FullOutput, needle: &str) -> bool {
        fn find(shape: &egui::epaint::Shape, needle: &str) -> bool {
            match shape {
                egui::epaint::Shape::Text(t) => t.galley.text().contains(needle),
                egui::epaint::Shape::Vec(v) => v.iter().any(|s| find(s, needle)),
                _ => false,
            }
        }
        output.shapes.iter().any(|c| find(&c.shape, needle))
    }

    #[test]
    fn changed_mouse_smoothing_is_dirty_and_decorated() {
        let mut menu = SettingsMenu::with_prefs(Prefs::default());
        *menu.state.staged_mut().mouse_smoothing_mut() = true;

        let output = render_menu(&mut menu);

        assert!(menu.has_unsaved_changes());
        assert!(has_dirty_decor(&output));
        assert!(has_text(&output, "Mouse smoothing"));
    }

    #[test]
    fn changed_invert_y_is_dirty_and_decorated() {
        let mut menu = SettingsMenu::with_prefs(Prefs::default());
        *menu.state.staged_mut().invert_mouse_y_mut() = true;

        let output = render_menu(&mut menu);

        assert!(menu.has_unsaved_changes());
        assert!(has_dirty_decor(&output));
        assert!(has_text(&output, "Invert mouse Y"));
    }

    #[test]
    fn changed_mouse_acceleration_is_dirty_and_decorated() {
        let mut menu = SettingsMenu::with_prefs(Prefs::default());
        *menu.state.staged_mut().mouse_acceleration_mut() = true;

        let output = render_menu(&mut menu);

        assert!(menu.has_unsaved_changes());
        assert!(menu.state.is_field_dirty(SettingsField::MouseAcceleration));
        assert!(has_dirty_decor(&output));
        assert!(has_text(&output, "Mouse acceleration"));
    }

    #[test]
    fn changed_mouse_acceleration_strength_is_dirty_and_decorated() {
        let mut menu = SettingsMenu::with_prefs(Prefs::default());
        *menu.state.staged_mut().mouse_acceleration_mut() = true;
        *menu.state.staged_mut().mouse_acceleration_strength_mut() = 0.37;

        let output = render_menu(&mut menu);

        assert!(
            menu.state
                .is_field_dirty(SettingsField::MouseAccelerationStrength)
        );
        assert!(has_dirty_decor(&output));
    }

    fn text_center(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
        fn find(shape: &egui::epaint::Shape, needle: &str) -> Option<egui::Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text().contains(needle) => {
                    Some(t.pos + t.galley.rect.center().to_vec2())
                }
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| find(s, needle)),
                _ => None,
            }
        }
        output.shapes.iter().find_map(|c| find(&c.shape, needle))
    }

    /// Drags the strength's value box rightwards with the mouse and returns the staged
    /// strength afterwards.
    fn strength_after_dragging(acceleration_on: bool) -> f32 {
        use crate::overlays::test_support::{input, run_ui};

        let mut menu = SettingsMenu::with_prefs(Prefs::default());
        *menu.state.staged_mut().mouse_acceleration_mut() = acceleration_on;
        *menu.state.staged_mut().mouse_acceleration_strength_mut() = 0.37;
        let ctx = egui::Context::default();
        let mut frame = |events: Vec<egui::Event>| {
            run_ui(&ctx, input(events), |ui| {
                menu.render(ui);
            })
        };
        frame(vec![]);
        let layout = frame(vec![]);
        let target = text_center(&layout, "0.37").expect("the strength value box is drawn");
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let moved = target + egui::vec2(120.0, 0.0);

        frame(vec![egui::Event::PointerMoved(target)]);
        frame(vec![button(target, true)]);
        frame(vec![egui::Event::PointerMoved(
            target + egui::vec2(60.0, 0.0),
        )]);
        frame(vec![egui::Event::PointerMoved(moved)]);
        frame(vec![button(moved, false)]);
        frame(vec![]);

        menu.state.staged().mouse_acceleration_strength()
    }

    #[test]
    fn strength_is_disabled_while_acceleration_is_off() {
        let strength = strength_after_dragging(false);

        assert_eq!(strength, 0.37);
    }

    #[test]
    fn strength_is_editable_while_acceleration_is_on() {
        let strength = strength_after_dragging(true);

        assert!(strength > 0.37, "strength after drag: {strength}");
    }

    #[test]
    fn untouched_controls_are_not_dirty_or_decorated() {
        let mut menu = SettingsMenu::with_prefs(Prefs::default());

        let output = render_menu(&mut menu);

        assert!(!menu.has_unsaved_changes());
        assert!(!has_dirty_decor(&output));
    }

    #[test]
    fn acceleration_strength_difference_of_exactly_one_epsilon_is_not_dirty() {
        let saved = 0.5_f32;
        let staged = saved + f32::EPSILON;
        assert_eq!(staged - saved, f32::EPSILON);
        let mut menu = SettingsMenu::with_prefs(
            Prefs::default()
                .with_mouse_acceleration(true)
                .with_mouse_acceleration_strength(saved),
        );
        *menu.state.staged_mut().mouse_acceleration_strength_mut() = staged;

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
