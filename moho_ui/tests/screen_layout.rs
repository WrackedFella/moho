//! Layout regression tests: panel order decides placement because
//! `show_inside` consumes the root Ui's remaining space.

use moho_ui::MenuItem;
use moho_ui::UiComponent;
use moho_ui::prefs::Prefs;
use moho_ui::screens::{NewWorldMenu, SettingsMenu};

const SCREEN_W: f32 = 800.0;
const SCREEN_H: f32 = 600.0;

fn screen_input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(SCREEN_W, SCREEN_H),
        )),
        events,
        ..Default::default()
    }
}

fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn assert_in_lower_half(items: &[MenuItem]) {
    assert!(!items.is_empty(), "expected bottom-panel items");
    for item in items {
        let rect = item.rect.expect("menu item should carry a rect");
        assert!(rect.min.y > SCREEN_H / 2.0, "not in lower half: {rect:?}");
        assert!(rect.max.y <= SCREEN_H, "extends past screen: {rect:?}");
    }
}

#[test]
fn settings_menu_bottom_buttons_sit_in_lower_half_of_screen() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let ctx = egui::Context::default();

    let mut items = Vec::new();
    for _ in 0..2 {
        let _ = ctx.run_ui(screen_input(Vec::new()), |ui| {
            items = menu.render(ui);
        });
    }

    assert_in_lower_half(&items);
}

/// `NewWorldMenu` reports items only for clicked buttons, so each button is
/// clicked where the bottom panel should place it; a misplaced panel misses
/// the click and returns nothing.
fn new_world_items_after_click(at: egui::Pos2) -> Vec<MenuItem> {
    let mut menu = NewWorldMenu::new();
    let ctx = egui::Context::default();

    let mut items = Vec::new();
    let frames = [
        Vec::new(),
        Vec::new(),
        vec![egui::Event::PointerMoved(at), pointer_button(at, true)],
        vec![pointer_button(at, false)],
    ];
    for events in frames {
        let _ = ctx.run_ui(screen_input(events), |ui| {
            items = menu.render(ui);
        });
    }
    items
}

#[test]
fn new_world_continue_button_sits_in_lower_half_of_screen() {
    let items = new_world_items_after_click(egui::pos2(500.0, 566.0));

    assert_in_lower_half(&items);
}

#[test]
fn new_world_back_button_sits_in_lower_half_of_screen() {
    let items = new_world_items_after_click(egui::pos2(370.0, 566.0));

    assert_in_lower_half(&items);
}
