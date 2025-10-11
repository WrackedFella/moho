use moho_ui::EguiUi;
use moho_ui::UiEvent;
use std::path::PathBuf;

#[test]
fn fallback_press_release_load_triggers_loadscene() {
    let (mut ui, receiver) = EguiUi::new(None);

    // Simulate rects for load and exit buttons in logical pixels
    let load_rect = egui::Rect::from_min_size(egui::pos2(8.0, 8.0), egui::vec2(120.0, 32.0));
    let exit_rect = egui::Rect::from_min_size(egui::pos2(8.0, 48.0), egui::vec2(120.0, 32.0));
    ui.test_set_button_rects(load_rect, exit_rect);

    // Simulate a press at (16,16) and release at (16,16) between frames
    ui.test_simulate_press_release((16.0, 16.0), (16.0, 16.0));
    let events = ui.test_collect_fallback_events();

    // Expect events: LoadScene then OverlayToggled(false)
    assert_eq!(events.len(), 2);
    match &events[0] {
        UiEvent::LoadScene(p) => assert_eq!(p.as_path(), PathBuf::from("saves/scene.bin")),
        other => panic!("expected LoadScene, got {:?}", other),
    }
    match &events[1] {
        UiEvent::OverlayToggled(false) => {},
        other => panic!("expected OverlayToggled(false), got {:?}", other),
    }
}

#[test]
fn fallback_press_release_exit_triggers_exit() {
    let (mut ui, _receiver) = EguiUi::new(None);

    let load_rect = egui::Rect::from_min_size(egui::pos2(8.0, 8.0), egui::vec2(120.0, 32.0));
    let exit_rect = egui::Rect::from_min_size(egui::pos2(8.0, 48.0), egui::vec2(120.0, 32.0));
    ui.test_set_button_rects(load_rect, exit_rect);

    // Simulate a press/release inside the exit button
    ui.test_simulate_press_release((16.0, 56.0), (16.0, 56.0));
    let events = ui.test_collect_fallback_events();

    assert_eq!(events.len(), 1);
    match &events[0] {
        UiEvent::Exit => {},
        other => panic!("expected Exit, got {:?}", other),
    }
}
