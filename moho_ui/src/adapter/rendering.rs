//! Rendering coordination for the UI adapter
//!
//! This module handles:
//! - Menu state rendering based on game state
//! - Console overlay rendering
//! - Progress overlay rendering
//! - Pause overlay rendering
use crate::adapter::ProgressState;
use crate::screens::MenuAction;
use crate::ui_state::UiStateManager;
use moho_core::EventBus;
use moho_types::GameState;

/// Render the UI based on current game state
///
/// Returns menu actions that were clicked during rendering
pub fn render_game_state(
    ctx: &egui::Context,
    ui_state: &mut UiStateManager,
    game_state: GameState,
    event_bus: &EventBus,
) -> Vec<MenuAction> {
    match game_state {
        GameState::Menu => render_menu(ctx, ui_state),
        GameState::ConsoleOpen => {
            render_console(ctx, ui_state, event_bus);
            Vec::new()
        }
        GameState::Paused => {
            render_pause_overlay(ctx);
            Vec::new()
        }
        GameState::Playing => {
            // No UI rendering when playing
            Vec::new()
        }
    }
}

/// Render active menu screen and collect clicked actions
fn render_menu(ctx: &egui::Context, ui_state: &mut UiStateManager) -> Vec<MenuAction> {
    let mut menu_actions = Vec::new();

    if let Some(screen) = ui_state.active_screen_mut() {
        let items = screen.render(ctx);

        // Collect clicked actions for processing outside the closure
        for item in items {
            if item.clicked && item.enabled {
                menu_actions.push(item.action);
            }
        }
    }

    menu_actions
}

/// Render console overlay and process console actions
fn render_console(ctx: &egui::Context, ui_state: &mut UiStateManager, event_bus: &EventBus) {
    let console_action = ui_state.console.render(ctx);

    // Process console action through event routing
    super::event_routing::process_console_action(console_action, event_bus);
}

/// Render pause overlay (simple centered message)
fn render_pause_overlay(ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.centered_and_justified(|ui| {
            ui.heading("Paused");
            ui.label("Press ESC to resume");
        });
    });
}

/// Render progress overlay if present
///
/// This renders a centered modal progress bar with optional cancel button.
/// Updates the progress.canceled flag if user clicks cancel.
pub fn render_progress_overlay(ctx: &egui::Context, progress: &mut ProgressState) {
    use egui::{Align2, RichText};

    egui::Area::new("progress_overlay_area".into())
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(&progress.title).heading());
                ui.add_space(6.0);
                ui.add(egui::ProgressBar::new(progress.percent).show_percentage());
                ui.add_space(8.0);
                if progress.cancellable && ui.add(egui::Button::new("Cancel")).clicked() {
                    progress.canceled = true;
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_render_pause_overlay() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            render_pause_overlay(ctx);
        });
        // Verify no panic
    }

    #[test]
    fn test_render_progress_overlay() {
        let ctx = egui::Context::default();
        let mut progress = ProgressState {
            title: "Test Progress".to_string(),
            percent: 0.5,
            cancellable: true,
            canceled: false,
        };

        let _ = ctx.run(Default::default(), |ctx| {
            render_progress_overlay(ctx, &mut progress);
        });
        assert!(!progress.canceled); // No interaction in test
    }

    #[test]
    fn test_render_progress_non_cancellable() {
        let ctx = egui::Context::default();
        let mut progress = ProgressState {
            title: "Loading".to_string(),
            percent: 0.75,
            cancellable: false,
            canceled: false,
        };

        let _ = ctx.run(Default::default(), |ctx| {
            render_progress_overlay(ctx, &mut progress);
        });
        assert!(!progress.canceled);
    }

    #[test]
    fn test_render_game_state_playing() {
        let ctx = egui::Context::default();
        let mut ui_state = UiStateManager::new();
        let event_bus = Arc::new(EventBus::new());

        let mut actions = Vec::new();
        let _ = ctx.run(Default::default(), |ctx| {
            actions = render_game_state(ctx, &mut ui_state, GameState::Playing, &event_bus);
        });
        assert!(actions.is_empty());
    }

    #[test]
    fn test_render_game_state_paused() {
        let ctx = egui::Context::default();
        let mut ui_state = UiStateManager::new();
        let event_bus = Arc::new(EventBus::new());

        let mut actions = Vec::new();
        let _ = ctx.run(Default::default(), |ctx| {
            actions = render_game_state(ctx, &mut ui_state, GameState::Paused, &event_bus);
        });
        assert!(actions.is_empty());
    }
}
