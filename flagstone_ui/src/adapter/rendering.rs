//! Rendering coordination for the UI adapter
//!
//! This module handles:
//! - Menu state rendering based on game state
//! - Console rendering
//! - Progress overlay rendering
use crate::adapter::ProgressState;
use crate::app_state::GameState;
use crate::screens::MenuAction;
use crate::ui_state::UiStateManager;

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
) -> MenuRenderResult {
    match game_state {
        GameState::Menu => render_menu(ui, ui_state),
        GameState::ConsoleOpen => {
            render_overlays(ui, ui_state);
            ui_state.console.show(ui.ctx());
            MenuRenderResult {
                actions: Vec::new(),
                hovered_key: None,
            }
        }
        GameState::Paused => {
            ui_state.overlay_manager.render_paused(ui);
            render_menu(ui, ui_state)
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
        let input = crate::overlays::test_support::input(events);
        let mut result = MenuRenderResult {
            actions: Vec::new(),
            hovered_key: None,
        };

        let output = crate::overlays::test_support::run_ui(ctx, input, |ui| {
            result = render_game_state(ui, ui_state, game_state);
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
        let mut ui_state = UiStateManager::new(moho_core::prefs::Prefs::default());
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
    fn paused_returns_the_actions_of_the_active_screen() {
        let ctx = egui::Context::default();
        let mut ui_state = menu_with_button(true);
        for _ in 0..2 {
            run_frame(&ctx, &mut ui_state, GameState::Paused, Vec::new());
        }

        let (_, result) = run_frame(
            &ctx,
            &mut ui_state,
            GameState::Paused,
            click_at(egui::pos2(18.0, 18.0)),
        );

        assert_eq!(result.actions, vec![MenuAction::Exit]);
    }

    fn strategy_hud(level: i32) -> crate::overlays::HudData {
        crate::overlays::HudData {
            is_fps_mode: false,
            time_of_day: 7.25,
            view_height: Some(crate::overlays::ViewHeightHud {
                floor: 0,
                top: 20,
                level,
            }),
            ..Default::default()
        }
    }

    fn fps_hud() -> crate::overlays::HudData {
        crate::overlays::HudData {
            is_fps_mode: true,
            time_of_day: 7.25,
            hotbar: vec![(1, 5)],
            ..Default::default()
        }
    }

    /// The text of the frame after the one that sized the areas.
    fn settled_text(
        ctx: &egui::Context,
        ui_state: &mut UiStateManager,
        game_state: GameState,
    ) -> String {
        run_frame(ctx, ui_state, game_state, Vec::new());
        screen_text(&run_frame(ctx, ui_state, game_state, Vec::new()).0)
    }

    fn assert_no_gameplay_hud(text: &str) {
        assert!(!text.contains("07:15"), "clock drawn: {text}");
        assert!(!text.contains("R1"), "hotbar drawn: {text}");
        assert!(!text.contains('6'), "view level drawn: {text}");
    }

    #[test]
    fn paused_frame_draws_no_gameplay_hud() {
        for hud in [strategy_hud(6), fps_hud()] {
            let ctx = egui::Context::default();
            let mut ui_state = UiStateManager::new(moho_core::prefs::Prefs::default());
            ui_state.overlay_manager.update_data(hud);
            assert!(ui_state.show_screen("pause"));

            let playing = settled_text(&ctx, &mut ui_state, GameState::Playing);
            let paused = settled_text(&ctx, &mut ui_state, GameState::Paused);

            assert!(playing.contains("07:15"), "the HUD draws at all: {playing}");
            assert_no_gameplay_hud(&paused);
            assert!(paused.contains("Paused"), "pause window missing: {paused}");
        }
    }

    #[test]
    fn settings_from_pause_draws_no_gameplay_hud() {
        for hud in [strategy_hud(6), fps_hud()] {
            let ctx = egui::Context::default();
            let mut ui_state = UiStateManager::new(moho_core::prefs::Prefs::default());
            ui_state.overlay_manager.update_data(hud);
            assert!(ui_state.show_screen("settings"));

            let text = settled_text(&ctx, &mut ui_state, GameState::Paused);

            assert_no_gameplay_hud(&text);
        }
    }

    #[test]
    fn hud_returns_after_pause_with_the_same_view_level() {
        let ctx = egui::Context::default();
        let mut ui_state = UiStateManager::new(moho_core::prefs::Prefs::default());
        ui_state.overlay_manager.update_data(strategy_hud(6));
        assert!(ui_state.show_screen("pause"));

        let paused = settled_text(&ctx, &mut ui_state, GameState::Paused);
        let playing = settled_text(&ctx, &mut ui_state, GameState::Playing);

        assert!(!paused.contains('6'), "paused text: {paused}");
        assert!(playing.contains('6'), "view level lost: {playing}");
        assert!(playing.contains("07:15"), "clock missing: {playing}");
    }

    #[test]
    fn paused_frame_still_draws_the_debug_hud_when_on() {
        let ctx = egui::Context::default();
        let mut ui_state = UiStateManager::new(moho_core::prefs::Prefs::default());
        ui_state.overlay_manager.update_data(strategy_hud(6));
        ui_state.overlay_manager.set_visible("debug", true);
        assert!(ui_state.show_screen("pause"));

        let text = settled_text(&ctx, &mut ui_state, GameState::Paused);

        assert!(text.contains("Pos:"), "debug HUD hidden: {text}");
    }

    #[test]
    fn render_game_state_by_state() {
        let ctx = egui::Context::default();
        let mut ui_state = UiStateManager::new(moho_core::prefs::Prefs::default());

        let (playing, playing_result) =
            run_frame(&ctx, &mut ui_state, GameState::Playing, Vec::new());
        assert!(ui_state.show_screen("pause"));
        run_frame(&ctx, &mut ui_state, GameState::Paused, Vec::new());
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
