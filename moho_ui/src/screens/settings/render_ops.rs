//! Rendering operations for the settings screen
//!
//! This module handles the visual layout and rendering of settings panels:
//! - Top panel with title and tab bar
//! - Bottom panel with action buttons (Save/Cancel/Back)
//! - Central content area with scrollable form

use super::{FormControls, MenuAction, SettingsMenu, SettingsTab};

// MenuItem is in the parent screens module, not settings
type MenuItem = super::super::MenuItem;

/// Render the top panel with title and tab selector
///
/// Returns the newly selected tab (may be different from current if user clicked a tab)
pub fn render_top_panel(ui: &mut egui::Ui, current_tab: SettingsTab) -> SettingsTab {
    let mut new_tab = current_tab;

    egui::Panel::top("settings_top").show_inside(ui, |ui| {
        // Use same gutter percentage as content area (30%)
        let avail = ui.available_width();
        let gutter = FormControls::calculate_gutter(avail, 0.30);

        ui.horizontal(|ui| {
            ui.add_space(gutter);
            ui.allocate_ui_with_layout(
                egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.add_space(16.0);
                    ui.heading("Game Settings");
                    ui.add_space(8.0);

                    // Tab bar
                    let new_tab_index =
                        FormControls::tab_bar(ui, SettingsTab::all_tabs(), current_tab.to_index());
                    new_tab = SettingsTab::from_index(new_tab_index);

                    ui.add_space(12.0);
                },
            );
            ui.add_space(gutter);
        });
    });

    new_tab
}

/// Render the bottom panel with action buttons
///
/// Returns menu items representing the button interactions
pub fn render_bottom_panel(ui: &mut egui::Ui, menu: &mut SettingsMenu) -> Vec<MenuItem> {
    let mut items = Vec::new();

    egui::Panel::bottom("settings_bottom").show_inside(ui, |ui| {
        // Use same gutter percentage as content area (30%)
        let avail = ui.available_width();
        let gutter = FormControls::calculate_gutter(avail, 0.30);

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(gutter);
            ui.allocate_ui_with_layout(
                egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    // Save button (always shown)
                    let save =
                        ui.add(egui::Button::new("Save Changes").min_size(egui::vec2(120.0, 36.0)));
                    let save_clicked = save.clicked();
                    if save_clicked {
                        let _ = menu.state.apply_changes();
                        let _ = menu.state.prefs().save();
                    }
                    items.push(MenuItem {
                        action: if save_clicked {
                            MenuAction::SettingsSaved(menu.state.prefs().clone())
                        } else {
                            MenuAction::None
                        },
                        rect: Some(save.rect),
                        enabled: true,
                        clicked: save_clicked,
                        hovered: false,
                    });
                    ui.add_space(8.0);

                    // Show Cancel when form is dirty, Back when clean
                    let is_dirty = menu.is_dirty();
                    if is_dirty {
                        let cancel =
                            ui.add(egui::Button::new("Cancel").min_size(egui::vec2(100.0, 36.0)));
                        let cancel_clicked = cancel.clicked();
                        if cancel_clicked {
                            menu.state.revert_changes();
                        }
                        items.push(MenuItem {
                            action: MenuAction::None,
                            rect: Some(cancel.rect),
                            enabled: true,
                            clicked: cancel_clicked,
                            hovered: false,
                        });
                    } else {
                        let back =
                            ui.add(egui::Button::new("Back").min_size(egui::vec2(100.0, 36.0)));
                        let back_clicked = back.clicked();
                        items.push(MenuItem {
                            action: MenuAction::ShowMenu("start".to_string()),
                            rect: Some(back.rect),
                            enabled: true,
                            clicked: back_clicked,
                            hovered: false,
                        });
                    }

                    ui.add_space(8.0);
                },
            );
            ui.add_space(gutter);
        });
        ui.add_space(16.0);
    });

    items
}

/// Render the central content area with scrollable form
pub fn render_content_area(ui: &mut egui::Ui, menu: &mut SettingsMenu) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Use percentage-based gutters (30% on each side)
                let avail = ui.available_width();
                let gutter = FormControls::calculate_gutter(avail, 0.30);

                ui.horizontal(|ui| {
                    ui.add_space(gutter);
                    ui.allocate_ui_with_layout(
                        egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            // Render the active tab content
                            match menu.active_tab {
                                SettingsTab::Controls => super::controls_tab::render(menu, ui),
                                SettingsTab::Audio => super::audio_tab::render(menu, ui),
                                SettingsTab::Video => super::video_tab::render(menu, ui),
                            }
                        },
                    );
                    ui.add_space(gutter);
                });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlays::test_support::{click_at, input, rect_of};

    fn run(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        tab: SettingsTab,
    ) -> (SettingsTab, egui::FullOutput) {
        let mut result = tab;
        let output = ctx.run_ui(input(events), |ui| {
            result = render_top_panel(ui, tab);
        });
        (result, output)
    }

    #[test]
    fn render_top_panel_returns_clicked_tab() {
        let ctx = egui::Context::default();
        let _ = run(&ctx, Vec::new(), SettingsTab::Controls);
        let (idle, output) = run(&ctx, Vec::new(), SettingsTab::Video);
        let audio = rect_of(&output, "Audio").center();
        let video = rect_of(&output, "Video").center();

        let (clicked_audio, _) = run(&ctx, click_at(audio), SettingsTab::Controls);
        let _ = run(&ctx, Vec::new(), SettingsTab::Controls);
        let (clicked_video, _) = run(&ctx, click_at(video), SettingsTab::Controls);

        assert_eq!(idle, SettingsTab::Video);
        assert_eq!(clicked_audio, SettingsTab::Audio);
        assert_eq!(clicked_video, SettingsTab::Video);
    }
}
