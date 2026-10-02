//! Tests for the menu system functionality

// See moho_ui/src/lib.rs's crate-level `#![allow(deprecated)]` for why
// `Context::run` (egui 0.34 deprecation) is still used here.
#![allow(deprecated)]

use moho_ui::UiComponent;
use moho_ui::{MenuAction, MenuItem, SettingsMenu};
use std::path::PathBuf;

#[test]
fn settings_menu_structure() {
    let mut menu = SettingsMenu::new();
    let ctx = egui::Context::default();

    // Call render() inside `ctx.run` so egui's internal state is initialized
    let mut items: Vec<MenuItem> = Vec::new();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        items = menu.render(ctx);
    });

    // Settings menu should return menu items
    assert!(!items.is_empty(), "SettingsMenu should return menu items");

    // Check that items have proper structure
    for item in &items {
        assert!(item.rect.is_some(), "Menu items should have rects");
        assert!(item.rect.unwrap().is_positive());
        assert!(item.enabled, "Menu items should be enabled by default");
    }

    // Verify menu metadata
    assert_eq!(menu.name(), "settings");
}

#[test]
fn menu_action_types() {
    // Test that the expected MenuAction variants exist and work
    let load_action = MenuAction::LoadScene(PathBuf::from("test.bin"));
    let new_world_action = MenuAction::NewWorld;
    let exit_action = MenuAction::Exit;
    let show_menu_action = MenuAction::ShowMenu("test".to_string());
    let close_action = MenuAction::Close;
    let none_action = MenuAction::None;

    // These should all be different variants
    assert_ne!(
        format!("{:?}", load_action),
        format!("{:?}", new_world_action)
    );
    assert_ne!(
        format!("{:?}", exit_action),
        format!("{:?}", show_menu_action)
    );
    assert_ne!(format!("{:?}", close_action), format!("{:?}", none_action));
}

#[test]
fn menu_item_creation() {
    use egui::Rect;

    let rect = Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 20.0));
    let item = MenuItem {
        action: MenuAction::Exit,
        rect: Some(rect),
        enabled: true,
        clicked: false,
        hovered: false,
    };

    assert_eq!(item.action, MenuAction::Exit);
    assert_eq!(item.rect, Some(rect));
    assert!(item.enabled);
    assert!(!item.clicked);
}
