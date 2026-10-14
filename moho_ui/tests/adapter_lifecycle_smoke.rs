#![cfg(feature = "ui-egui-test")]
//! Smoke test for the egui adapter lifecycle.
//!
//! This test verifies basic adapter functionality and menu management.

use moho_ui::build_adapter;
use moho_ui::prefs::Prefs;
use std::sync::Arc;

#[test]
fn adapter_lifecycle_smoke() {
    let event_bus = Arc::new(moho_core::EventBus::new());
    let mut adapter = build_adapter(None, event_bus.clone(), Prefs::default());

    // Adapter should start with menus visible
    assert!(adapter.is_visible());

    adapter.hide_menus();
    assert!(!adapter.is_visible());

    adapter.show_menu("settings");

    assert!(adapter.is_visible());
    assert_eq!(adapter.active_screen_name(), Some("settings"));
}
