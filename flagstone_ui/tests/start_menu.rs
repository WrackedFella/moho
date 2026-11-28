use flagstone_ui::StartMenu;
use flagstone_ui::UiComponent;
use std::path::PathBuf;

#[test]
fn start_menu_returns_expected_action_and_rects() {
    let mut menu = StartMenu::new();
    let ctx = egui::Context::default();

    // Call render() inside `ctx.run_ui` so egui's internal state (available_rect, etc)
    // is initialized properly. Capture the returned menu items.
    let mut items: Vec<flagstone_ui::MenuItem> = Vec::new();
    let _ = run_ui(&ctx, egui::RawInput::default(), |ui| {
        items = menu.render(ui);
    });

    assert_eq!(items.len(), 4, "expected 4 menu items from StartMenu");
    for item in &items {
        assert!(item.rect.is_some(), "expected item.rect to be present");
        assert!(item.rect.unwrap().is_positive());
        assert!(!item.clicked, "nothing was clicked");
    }
    let actions: Vec<_> = items.iter().map(|i| i.action.clone()).collect();
    assert_eq!(
        actions,
        [
            flagstone_ui::MenuAction::LoadScene(PathBuf::from("saves/scene.bin")),
            flagstone_ui::MenuAction::NewWorld,
            flagstone_ui::MenuAction::ShowMenu("settings".to_string()),
            flagstone_ui::MenuAction::Exit,
        ]
    );

    // Verify menu metadata
    assert_eq!(menu.name(), "start");
}

#[test]
fn hit_test_helper_press_release_and_release_only() {
    use flagstone_ui::input_handling::{hit_test_menu_items, rect_from_min_max};
    use flagstone_ui::{MenuAction, MenuItem};

    // Construct four menu items with deterministic rects in logical points
    let cont = MenuItem {
        action: MenuAction::LoadScene(PathBuf::from("saves/scene.bin")),
        rect: Some(rect_from_min_max(8.0, 8.0, 128.0, 40.0)),
        enabled: true,
        clicked: false,
        hovered: false,
    };
    let neww = MenuItem {
        action: MenuAction::NewWorld,
        rect: Some(rect_from_min_max(8.0, 48.0, 128.0, 80.0)),
        enabled: true,
        clicked: false,
        hovered: false,
    };
    let set = MenuItem {
        action: MenuAction::ShowMenu("settings".to_string()),
        rect: Some(rect_from_min_max(8.0, 88.0, 128.0, 120.0)),
        enabled: true,
        clicked: false,
        hovered: false,
    };
    let exit = MenuItem {
        action: MenuAction::Exit,
        rect: Some(rect_from_min_max(8.0, 128.0, 128.0, 160.0)),
        enabled: true,
        clicked: false,
        hovered: false,
    };
    let items = vec![cont, neww, set, exit];

    // Simulate a press and release inside the Settings rect
    // Use coordinates that fall inside the `set` rect above
    let press = Some((32.0_f32, 96.0_f32));
    let release = (32.0_f32, 96.0_f32);
    let action = hit_test_menu_items(
        press, release, &items, None, /*coords_are_physical=*/ false, 0.0,
    )
    .expect("expected an action");
    match action {
        MenuAction::ShowMenu(name) => assert_eq!(name, "settings"),
        other => panic!("unexpected action: {other:?}"),
    }

    // Simulate a release-only on Continue
    let action2 = hit_test_menu_items(
        None,
        (16.0_f32, 16.0_f32 + 24.0_f32),
        &items,
        None,
        /*coords_are_physical=*/ false,
        0.0,
    )
    .expect("expected an action");
    match action2 {
        MenuAction::LoadScene(p) => assert_eq!(p, PathBuf::from("saves/scene.bin")),
        other => panic!("unexpected action: {other:?}"),
    }
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
