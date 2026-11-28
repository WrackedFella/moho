//! The Settings screen read the way a player does: controls found by role and label in the
//! accessibility tree, clicked through the game's own input routing, with no display or GPU.

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use moho_app::GameClock;
use moho_core::prefs::Prefs;
use moho_input::key::MouseButton;

use super::tests::game_with_a_moving_enemy;
use super::*;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 720.0);

fn main_menu_game(path: &Path) -> SearchlightGame {
    game_with_a_moving_enemy()
        .with_main_menu()
        .with_debug_settings(DebugSettings::new(Prefs::default(), path.to_path_buf()))
}

fn harness_of(game: SearchlightGame) -> Harness<'static, SearchlightGame> {
    let mut harness = Harness::builder().with_size(SIZE).build_ui_state(
        |ui, game: &mut SearchlightGame| game.stage.shell().paint(ui.ctx()),
        game,
    );
    harness.run();
    harness
}

/// A left click at the centre of the control found by `role` and `label`, routed as a player's
/// click is, then the frame settles.
fn click_found(harness: &mut Harness<'static, SearchlightGame>, role: Role, label: &str) {
    let centre = harness.get_by_role_and_label(role, label).rect().center();
    let game = harness.state_mut();
    game.stage
        .shell()
        .cursor_moved([f64::from(centre.x), f64::from(centre.y)], 1.0);
    let clock = GameClock::default();
    let mut ctx = EventContext::new(&clock, None, None);
    game.route_input(&mut ctx, Input::Mouse(MouseButton::Left), true, false);
    game.route_input(&mut ctx, Input::Mouse(MouseButton::Left), false, false);
    harness.run();
}

fn toggled_of(harness: &Harness<'static, SearchlightGame>, label: &str) -> Option<Toggled> {
    harness
        .get_by_role_and_label(Role::CheckBox, label)
        .accesskit_node()
        .toggled()
}

#[test]
fn invert_mouse_y_found_by_its_label_reads_checked_and_is_applied() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("searchlight.ini");
    let mut harness = harness_of(main_menu_game(&path));
    let saved_before = path.exists();
    click_found(&mut harness, Role::Button, "Settings");
    click_found(&mut harness, Role::Button, "Mouse");
    let before = toggled_of(&harness, "Invert mouse Y");

    click_found(&mut harness, Role::CheckBox, "Invert mouse Y");
    let after = toggled_of(&harness, "Invert mouse Y");
    harness.state_mut().stage.shell().menu_mut().scroll(1000.0);
    harness.run();
    click_found(&mut harness, Role::Button, "Apply");

    assert!(!saved_before, "nothing is saved before Apply");
    assert_eq!(before, Some(Toggled::False));
    assert_eq!(after, Some(Toggled::True));
    assert!(path.exists(), "Apply saves the prefs file");
    let (saved, warnings) = Prefs::load_from(&path);
    assert_eq!(warnings, vec![]);
    assert!(saved.invert_mouse_y(), "the saved prefs invert the mouse");
}
