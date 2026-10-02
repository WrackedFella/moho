#![cfg(feature = "ui-egui-test")]
//! Smoke test for the egui adapter lifecycle.
//!
//! This test verifies basic adapter functionality and menu management.

use moho_ui::build_adapter;
use std::sync::Arc;

#[test]
fn adapter_lifecycle_smoke() {
    let event_bus = Arc::new(moho_core::EventBus::new());
    let mut adapter = build_adapter(None, event_bus.clone());

    // Adapter should start with menus visible
    assert!(adapter.is_visible());

    // Test menu switching (we can't check internal state, but we can verify no panics)
    adapter.show_menu("settings");
    assert!(adapter.is_visible());

    // Switch back to start menu
    adapter.show_menu("start");
    assert!(adapter.is_visible());

    // Test hiding menus
    adapter.hide_menus();
    assert!(!adapter.is_visible());

    // Test recall_staging_belt is safe to call
    adapter.recall_staging_belt();
    adapter.recall_staging_belt();
}
