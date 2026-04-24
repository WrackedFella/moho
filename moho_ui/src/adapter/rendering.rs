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

/// Result of rendering the menu for a single frame.
pub struct MenuRenderResult {
    /// Actions triggered by clicking menu items
    pub actions: Vec<MenuAction>,
    /// Debug string of the currently-hovered item action (None if nothing hovered)
    pub hovered_key: Option<String>,
}

/// Render the UI based on current game state
///
/// Returns menu actions that were clicked during rendering, plus hover state.
pub fn render_game_state(
    ctx: &egui::Context,
    ui_state: &mut UiStateManager,
    game_state: GameState,
    event_bus: &EventBus,
) -> MenuRenderResult {
    match game_state {
        GameState::Menu => render_menu(ctx, ui_state),
        GameState::ConsoleOpen => {
            render_overlays(ctx, ui_state);
            render_console(ctx, ui_state, event_bus);
            MenuRenderResult {
                actions: Vec::new(),
                hovered_key: None,
            }
        }
        GameState::Paused => {
            render_overlays(ctx, ui_state);
            render_pause_overlay(ctx);
            MenuRenderResult {
                actions: Vec::new(),
                hovered_key: None,
            }
        }
        GameState::Playing => {
            render_overlays(ctx, ui_state);
            MenuRenderResult {
                actions: Vec::new(),
                hovered_key: None,
            }
        }
    }
}

/// Render all active overlay layers (HUDs, debug info)
fn render_overlays(ctx: &egui::Context, ui_state: &mut UiStateManager) {
    ui_state.overlay_manager.render_all(ctx);
}

/// Render active menu screen and collect clicked actions and hover state
fn render_menu(ctx: &egui::Context, ui_state: &mut UiStateManager) -> MenuRenderResult {
    let mut actions = Vec::new();
    let mut hovered_key: Option<String> = None;

    if let Some(screen) = ui_state.active_screen_mut() {
        let items = screen.render(ctx);

        for item in items {
            if item.hovered && item.enabled {
                // Use the action's debug string as a stable per-item key
                hovered_key = Some(format!("{:?}", item.action));
            }
            if item.clicked && item.enabled {
                actions.push(item.action);
            }
        }
    }

    MenuRenderResult {
        actions,
        hovered_key,
    }
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

        let mut result = MenuRenderResult {
            actions: Vec::new(),
            hovered_key: None,
        };
        let _ = ctx.run(Default::default(), |ctx| {
            result = render_game_state(ctx, &mut ui_state, GameState::Playing, &event_bus);
        });
        assert!(result.actions.is_empty());
    }

    #[test]
    fn test_render_game_state_paused() {
        let ctx = egui::Context::default();
        let mut ui_state = UiStateManager::new();
        let event_bus = Arc::new(EventBus::new());

        let mut result = MenuRenderResult {
            actions: Vec::new(),
            hovered_key: None,
        };
        let _ = ctx.run(Default::default(), |ctx| {
            result = render_game_state(ctx, &mut ui_state, GameState::Paused, &event_bus);
        });
        assert!(result.actions.is_empty());
    }
}
