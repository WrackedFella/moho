//! Tests for the menu system functionality

use flagstone_ui::UiComponent;
use flagstone_ui::prefs::Prefs;
use flagstone_ui::{MenuAction, MenuItem, SettingsMenu};

#[test]
fn settings_menu_structure() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let ctx = egui::Context::default();

    // Call render() inside `ctx.run_ui` so egui's internal state is initialized
    let mut items: Vec<MenuItem> = Vec::new();
    let _ = run_ui(&ctx, egui::RawInput::default(), |ui| {
        items = menu.render(ui);
    });

    assert_eq!(items.len(), 2, "SettingsMenu should return 2 menu items");
    for item in &items {
        assert!(item.rect.is_some(), "Menu items should have rects");
        assert!(item.rect.unwrap().is_positive());
        assert!(item.enabled, "Menu items should be enabled by default");
        assert!(!item.clicked, "nothing was clicked");
    }
    assert_eq!(
        items[0].action,
        MenuAction::None,
        "Save emits nothing unclicked"
    );
    assert_eq!(items[1].action, MenuAction::ShowMenu("start".to_string()));

    // Verify menu metadata
    assert_eq!(menu.name(), "settings");
}

/// Runs one frame and clears its texture delta; egui asserts in debug builds
/// that a non-empty delta is not dropped, and these tests never paint it.
fn run_ui(
    ctx: &egui::Context,
    input: egui::RawInput,
    add_contents: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, add_contents);
    output.textures_delta.clear();
    output
}
