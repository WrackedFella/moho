//! Rendering coordination for the UI adapter
//!
//! This module handles:
//! - Menu state rendering based on game state
//! - Console overlay rendering
//! - Progress overlay rendering
//! - Pause overlay rendering
use crate::adapter::ProgressState;
use crate::app_state::GameState;
use crate::screens::MenuAction;
use crate::ui_state::UiStateManager;
use moho_core::EventBus;

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
    ui: &mut egui::Ui,
    ui_state: &mut UiStateManager,
    game_state: GameState,
    event_bus: &EventBus,
) -> MenuRenderResult {
    match game_state {
        GameState::Menu => render_menu(ui, ui_state),
        GameState::ConsoleOpen => {
            render_overlays(ui, ui_state);
            render_console(ui, ui_state, event_bus);
            MenuRenderResult {
                actions: Vec::new(),
                hovered_key: None,
            }
        }
        GameState::Paused => {
            render_overlays(ui, ui_state);
            render_pause_overlay(ui);
            MenuRenderResult {
                actions: Vec::new(),
                hovered_key: None,
            }
        }
        GameState::Playing => {
            render_overlays(ui, ui_state);
            MenuRenderResult {
                actions: Vec::new(),
                hovered_key: None,
            }
        }
    }
}

/// Render all active overlay layers (HUDs, debug info)
fn render_overlays(ui: &mut egui::Ui, ui_state: &mut UiStateManager) {
    ui_state.overlay_manager.render_all(ui);
}

/// Render active menu screen and collect clicked actions and hover state
fn render_menu(ui: &mut egui::Ui, ui_state: &mut UiStateManager) -> MenuRenderResult {
    let mut actions = Vec::new();
    let mut hovered_key: Option<String> = None;

    if let Some(screen) = ui_state.active_screen_mut() {
        let items = screen.render(ui);

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
fn render_console(ui: &mut egui::Ui, ui_state: &mut UiStateManager, event_bus: &EventBus) {
    let console_action = ui_state.console.render(ui);

    // Process console action through event routing
    super::event_routing::process_console_action(console_action, event_bus);
}

/// Render pause overlay (simple centered message)
fn render_pause_overlay(ui: &mut egui::Ui) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
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
pub fn render_progress_overlay(ui: &mut egui::Ui, progress: &mut ProgressState) {
    use egui::{Align2, RichText};

    egui::Area::new("progress_overlay_area".into())
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ui.ctx(), |ui| {
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

    use crate::screens::{MenuItem, Screen, ScreenSpec, UiComponent};

    /// One button at a fixed position, reporting a real click through its `MenuItem`.
    struct ButtonScreen {
        action: MenuAction,
        enabled: bool,
        spec: ScreenSpec,
    }

    impl UiComponent for ButtonScreen {
        fn name(&self) -> &'static str {
            "button_screen"
        }

        fn render(&mut self, ui: &mut egui::Ui) -> Vec<MenuItem> {
            let mut items = Vec::new();
            egui::Area::new("button_screen_area".into())
                .fixed_pos(egui::pos2(10.0, 10.0))
                .show(ui.ctx(), |ui| {
                    let response = ui.button("Go");
                    items.push(MenuItem {
                        action: self.action.clone(),
                        rect: Some(response.rect),
                        enabled: self.enabled,
                        clicked: response.clicked(),
                        hovered: response.hovered(),
                    });
                });
            items
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    impl Screen for ButtonScreen {
        fn spec(&self) -> &ScreenSpec {
            &self.spec
        }
    }

    fn screen_text(output: &egui::FullOutput) -> String {
        crate::overlays::test_support::texts(output).join("\n")
    }

    fn run_frame(
        ctx: &egui::Context,
        ui_state: &mut UiStateManager,
        game_state: GameState,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, MenuRenderResult) {
        let bus = EventBus::new();
        let input = crate::overlays::test_support::input(events);
        let mut result = MenuRenderResult {
            actions: Vec::new(),
            hovered_key: None,
        };

        let output = ctx.run_ui(input, |ui| {
            result = render_game_state(ui, ui_state, game_state, &bus);
        });

        (output, result)
    }

    fn click_at(pos: egui::Pos2) -> Vec<egui::Event> {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![egui::Event::PointerMoved(pos), button(true), button(false)]
    }

    fn menu_with_button(enabled: bool) -> UiStateManager {
        let mut ui_state = UiStateManager::new();
        ui_state.add_screen(
            "button_screen".to_string(),
            Box::new(ButtonScreen {
                action: MenuAction::Exit,
                enabled,
                spec: ScreenSpec::default(),
            }),
        );
        assert!(ui_state.show_screen("button_screen"));
        ui_state
    }

    fn clicked_menu_actions(enabled: bool) -> Vec<MenuAction> {
        let ctx = egui::Context::default();
        let mut ui_state = menu_with_button(enabled);
        for _ in 0..2 {
            run_frame(&ctx, &mut ui_state, GameState::Menu, Vec::new());
        }

        let (_, result) = run_frame(
            &ctx,
            &mut ui_state,
            GameState::Menu,
            click_at(egui::pos2(18.0, 18.0)),
        );

        result.actions
    }

    #[test]
    fn render_game_state_by_state() {
        let ctx = egui::Context::default();
        let mut ui_state = UiStateManager::new();

        let (playing, playing_result) =
            run_frame(&ctx, &mut ui_state, GameState::Playing, Vec::new());
        let (paused, paused_result) = run_frame(&ctx, &mut ui_state, GameState::Paused, Vec::new());

        assert!(
            !screen_text(&playing).contains("Paused"),
            "playing text: {}",
            screen_text(&playing)
        );
        assert!(
            screen_text(&paused).contains("Paused"),
            "paused text: {}",
            screen_text(&paused)
        );
        assert!(playing_result.actions.is_empty());
        assert!(paused_result.actions.is_empty());
        assert_eq!(clicked_menu_actions(true), vec![MenuAction::Exit]);
        assert!(clicked_menu_actions(false).is_empty());
    }
}
