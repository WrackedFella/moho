/// Renders the Video tab content.
///
/// Provides window mode and resolution settings.
use super::controls_tab::toggle_row;
use super::{SettingsField, SettingsMenu};
use crate::prefs::{Prefs, RESOLUTION_PRESETS, WindowMode};

pub fn render(menu: &mut SettingsMenu, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        // Video Section Header
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Video")
                    .size(18.0)
                    .color(egui::Color32::from_rgb(200, 200, 200)),
            );
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(8.0);

        let label_width = 150.0;

        // Window Mode
        {
            let saved_mode = menu.state.prefs().window_mode();
            let staged_mode = menu.state.staged().window_mode();
            let is_dirty = staged_mode != saved_mode;

            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(label_width, 28.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.label("Window Mode:");
                    },
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut current = staged_mode;
                    let combo = egui::ComboBox::from_id_salt("window_mode_combo")
                        .selected_text(mode_label(current))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for &mode in &[
                                WindowMode::Windowed,
                                WindowMode::Fullscreen,
                                WindowMode::Borderless,
                            ] {
                                ui.selectable_value(&mut current, mode, mode_label(mode));
                            }
                        });

                    if current != staged_mode {
                        menu.state.set_window_mode(current);
                    }

                    if is_dirty {
                        SettingsMenu::paint_dirty_decor(ui, &combo.response, true);
                    }
                });
            });
        }

        ui.add_space(8.0);

        // Resolution preset (disabled when Fullscreen — OS controls it)
        {
            let staged_mode = menu.state.staged().window_mode();
            let resolution_enabled = staged_mode != WindowMode::Fullscreen;
            let saved_res = menu.state.prefs().window_resolution();
            let staged_res = menu.state.staged().window_resolution();
            let is_dirty = staged_res != saved_res;

            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(label_width, 28.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_enabled(resolution_enabled, egui::Label::new("Resolution:"));
                    },
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let selected_label = RESOLUTION_PRESETS
                        .iter()
                        .find(|(_, res)| *res == staged_res)
                        .map_or("Custom", |(lbl, _)| *lbl);

                    let mut current_res = staged_res;
                    let combo = egui::ComboBox::from_id_salt("resolution_combo")
                        .selected_text(selected_label)
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for &(label, res) in RESOLUTION_PRESETS {
                                ui.add_enabled_ui(resolution_enabled, |ui| {
                                    ui.selectable_value(&mut current_res, res, label);
                                });
                            }
                        });

                    if resolution_enabled && current_res != staged_res {
                        menu.state
                            .set_window_resolution(current_res.0, current_res.1);
                    }

                    if is_dirty {
                        SettingsMenu::paint_dirty_decor(ui, &combo.response, true);
                    }
                });
            });
        }

        ui.add_space(8.0);

        toggle_row(
            menu,
            ui,
            label_width,
            "Vsync:",
            SettingsField::Vsync,
            Prefs::vsync,
            Prefs::vsync_mut,
        );

        ui.add_space(8.0);

        frame_cap_row(menu, ui, label_width);

        ui.add_space(8.0);

        brightness_row(menu, ui, label_width);

        ui.add_space(8.0);

        bloom_row(menu, ui, label_width);

        ui.add_space(8.0);

        shadow_quality_row(menu, ui, label_width);
    });
}

const SHADOW_QUALITY_LEVELS: [&str; 5] = ["Off", "Low", "Medium", "High", "Ultra"];

fn shadow_quality_row(menu: &mut SettingsMenu, ui: &mut egui::Ui, label_width: f32) {
    let staged = menu.state.staged().shadow_quality();
    let saved = menu.state.prefs().shadow_quality();
    let combo = LevelCombo {
        label: "Shadow quality:",
        id: "shadow_quality_combo",
        names: &SHADOW_QUALITY_LEVELS,
    };
    if let Some(level) = combo.show(ui, label_width, staged, saved) {
        menu.state.set_shadow_quality(level);
    }
}

const BLOOM_LEVELS: [&str; 3] = ["Off", "Low", "High"];

fn bloom_row(menu: &mut SettingsMenu, ui: &mut egui::Ui, label_width: f32) {
    let staged = menu.state.staged().bloom();
    let saved = menu.state.prefs().bloom();
    let combo = LevelCombo {
        label: "Bloom:",
        id: "bloom_combo",
        names: &BLOOM_LEVELS,
    };
    if let Some(level) = combo.show(ui, label_width, staged, saved) {
        menu.state.set_bloom(level);
    }
}

/// A labelled combo over named levels `0..names.len()`, with the dirty decor.
struct LevelCombo {
    label: &'static str,
    id: &'static str,
    names: &'static [&'static str],
}

impl LevelCombo {
    /// Draws the row; returns the level picked this frame, if it differs from `staged`.
    fn show(&self, ui: &mut egui::Ui, label_width: f32, staged: u32, saved: u32) -> Option<u32> {
        let mut picked = None;
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(self.label);
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut current = staged;
                let shown = (staged as usize).min(self.names.len() - 1);
                let combo = egui::ComboBox::from_id_salt(self.id)
                    .selected_text(self.names[shown])
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for (level, name) in (0u32..).zip(self.names) {
                            ui.selectable_value(&mut current, level, *name);
                        }
                    });
                if current != staged {
                    picked = Some(current);
                }
                SettingsMenu::paint_dirty_decor(ui, &combo.response, staged != saved);
            });
        });
        picked
    }
}

/// The Brightness slider, shown as a percentage of the default exposure.
fn brightness_row(menu: &mut SettingsMenu, ui: &mut egui::Ui, label_width: f32) {
    let staged = menu.state.staged().brightness();
    let is_dirty = (staged - menu.state.prefs().brightness()).abs() > f32::EPSILON;

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label("Brightness:");
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut percent = staged * 100.0;
            let slider = ui.add(
                egui::Slider::new(&mut percent, 50.0..=200.0)
                    .suffix("%")
                    .max_decimals(0),
            );
            if slider.changed() {
                menu.state.set_brightness(percent / 100.0);
            }
            SettingsMenu::paint_dirty_decor(ui, &slider, is_dirty);
        });
    });
}

/// The frame-rate cap, editable only while staged vsync is off.
fn frame_cap_row(menu: &mut SettingsMenu, ui: &mut egui::Ui, label_width: f32) {
    let cap_enabled = !menu.state.staged().vsync();
    let is_dirty = menu.state.staged().fps_max() != menu.state.prefs().fps_max();

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_enabled(cap_enabled, egui::Label::new("Frame-rate cap:"));
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(cap_enabled, |ui| {
                let drag = ui.add(
                    egui::DragValue::new(menu.state.staged_mut().fps_max_mut())
                        .range(0..=1000)
                        .custom_formatter(|value, _| {
                            if value == 0.0 {
                                "Unlimited".to_owned()
                            } else {
                                format!("{value:.0}")
                            }
                        }),
                );
                SettingsMenu::paint_dirty_decor(ui, &drag, is_dirty);
            });
        });
    });

    if is_dirty {
        menu.state.mark_dirty(SettingsField::FpsMax);
    }
}

fn mode_label(mode: WindowMode) -> &'static str {
    match mode {
        WindowMode::Windowed => "Windowed",
        WindowMode::Fullscreen => "Fullscreen",
        WindowMode::Borderless => "Borderless",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiComponent;
    use crate::overlays::test_support::{input, run_ui};
    use crate::prefs::Prefs;
    use crate::screens::SettingsTab;
    use crate::screens::settings::SettingsField;

    fn video_menu(prefs: Prefs) -> SettingsMenu {
        let mut menu = SettingsMenu::with_prefs(prefs);
        menu.set_active_tab(SettingsTab::Video);
        menu
    }

    fn render_menu(menu: &mut SettingsMenu) -> egui::FullOutput {
        let ctx = egui::Context::default();
        run_ui(&ctx, egui::RawInput::default(), |ui| {
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

    /// Drags the cap's value box rightwards; returns the staged cap afterwards.
    fn cap_after_dragging(vsync_on: bool) -> u32 {
        let mut prefs = Prefs::default();
        prefs.set_fps_max(144);
        let mut menu = video_menu(prefs);
        menu.state.staged_mut().set_vsync(vsync_on);
        let ctx = egui::Context::default();
        let mut frame = |events: Vec<egui::Event>| {
            run_ui(&ctx, input(events), |ui| {
                menu.render(ui);
            })
        };
        frame(vec![]);
        let layout = frame(vec![]);
        let target = text_center(&layout, "144").expect("the cap value box is drawn");
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

        menu.state.staged().fps_max()
    }

    #[test]
    fn changed_vsync_is_dirty_and_decorated() {
        let mut menu = video_menu(Prefs::default());
        menu.state.staged_mut().set_vsync(false);

        let output = render_menu(&mut menu);

        assert!(menu.has_unsaved_changes());
        assert!(menu.state.is_field_dirty(SettingsField::Vsync));
        assert!(!menu.state.is_field_dirty(SettingsField::FpsMax));
        assert!(has_dirty_decor(&output));
        assert!(text_center(&output, "Vsync").is_some());
    }

    #[test]
    fn changed_frame_rate_cap_is_dirty_and_decorated() {
        let mut menu = video_menu(Prefs::default());
        menu.state.staged_mut().set_vsync(false);
        menu.state.staged_mut().set_fps_max(144);

        let output = render_menu(&mut menu);

        assert!(menu.state.is_field_dirty(SettingsField::FpsMax));
        assert!(has_dirty_decor(&output));
        assert!(text_center(&output, "Frame-rate cap").is_some());
    }

    #[test]
    fn untouched_video_tab_is_not_dirty_or_decorated() {
        let mut menu = video_menu(Prefs::default());

        let output = render_menu(&mut menu);

        assert!(!menu.has_unsaved_changes());
        assert!(!has_dirty_decor(&output));
        assert!(text_center(&output, "Vsync").is_some());
    }

    #[test]
    fn zero_cap_is_shown_as_unlimited() {
        let mut menu = video_menu(Prefs::default());
        menu.state.staged_mut().set_vsync(false);
        menu.state.staged_mut().set_fps_max(0);

        let output = render_menu(&mut menu);

        assert!(text_center(&output, "Unlimited").is_some());
    }

    #[test]
    fn cap_is_disabled_while_vsync_is_on() {
        let cap = cap_after_dragging(true);

        assert_eq!(cap, 144);
    }

    #[test]
    fn cap_is_editable_while_vsync_is_off() {
        let cap = cap_after_dragging(false);

        assert!(cap > 144, "cap after drag: {cap}");
    }
}
