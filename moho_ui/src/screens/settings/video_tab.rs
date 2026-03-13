/// Renders the Video tab content.
///
/// Provides window mode and resolution settings.
use super::{SettingsField, SettingsMenu};
use crate::prefs::{RESOLUTION_PRESETS, WindowMode};

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
                    let combo =
                        egui::ComboBox::from_id_salt("window_mode_combo")
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
                        menu.state.staged_mut().set_window_mode(current);
                        menu.state.mark_dirty(SettingsField::WindowMode);
                    }

                    if is_dirty {
                        SettingsMenu::paint_dirty_decor(ui, &combo.response, true);
                    }
                });
            });

            if is_dirty {
                menu.state.mark_dirty(SettingsField::WindowMode);
            }
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
                        ui.add_enabled(
                            resolution_enabled,
                            egui::Label::new("Resolution:"),
                        );
                    },
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let selected_label = RESOLUTION_PRESETS
                        .iter()
                        .find(|(_, res)| *res == staged_res)
                        .map(|(lbl, _)| *lbl)
                        .unwrap_or("Custom");

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
                        menu.state.staged_mut().set_window_resolution(current_res.0, current_res.1);
                        menu.state.mark_dirty(SettingsField::WindowResolution);
                    }

                    if is_dirty {
                        SettingsMenu::paint_dirty_decor(ui, &combo.response, true);
                    }
                });
            });

            if is_dirty {
                menu.state.mark_dirty(SettingsField::WindowResolution);
            }
        }
    });
}

fn mode_label(mode: WindowMode) -> &'static str {
    match mode {
        WindowMode::Windowed => "Windowed",
        WindowMode::Fullscreen => "Fullscreen",
        WindowMode::Borderless => "Borderless",
    }
}
