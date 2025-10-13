#![cfg(feature = "ui-egui-test")]
//! Smoke test for the egui adapter lifecycle.
//!
//! This test verifies basic adapter functionality and menu management.

use moho_ui::build_adapter;

#[test]
fn adapter_lifecycle_smoke() {
    let (mut adapter, receiver) = build_adapter(None);

    // Adapter should start with menus visible
    assert!(adapter.ui_visible);

    // Construction should not emit any events initially
    assert!(receiver.is_empty());

    // Test menu switching (we can't check internal state, but we can verify no panics)
    adapter.show_menu("settings");
    assert!(adapter.ui_visible);

    // Switch back to start menu
    adapter.show_menu("start");
    assert!(adapter.ui_visible);

    // Test hiding menus
    adapter.hide_menus();
    assert!(!adapter.ui_visible);

    // Test recall_staging_belt is safe to call
    adapter.recall_staging_belt();
    adapter.recall_staging_belt();
}

#[test]
fn menu_action_processing() {
    let (mut adapter, _receiver) = build_adapter(None);

    // Test that menu switching works without panicking
    adapter.show_menu("settings");
    adapter.show_menu("start");
    adapter.hide_menus();

    // Should be able to call multiple times safely
    adapter.recall_staging_belt();
}
