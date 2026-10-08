//! Tests for the menu system functionality

// See moho_ui/src/lib.rs's crate-level `#![allow(deprecated)]` for why
// `Context::run` (egui 0.34 deprecation) is still used here.
#![allow(deprecated)]

use moho_ui::UiComponent;
use moho_ui::prefs::Prefs;
use moho_ui::{MenuItem, SettingsMenu};

#[test]
fn settings_menu_structure() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let ctx = egui::Context::default();

    // Call render() inside `ctx.run` so egui's internal state is initialized
    let mut items: Vec<MenuItem> = Vec::new();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        items = menu.render(ctx);
    });

    // Settings menu returns its bottom-panel items: save and back/cancel
    assert_eq!(items.len(), 2, "SettingsMenu should return 2 menu items");

    // Check that items have proper structure
    for item in &items {
        assert!(item.rect.is_some(), "Menu items should have rects");
        assert!(item.rect.unwrap().is_positive());
        assert!(item.enabled, "Menu items should be enabled by default");
    }

    // Verify menu metadata
    assert_eq!(menu.name(), "settings");
}
