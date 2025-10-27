#![cfg(feature = "ui-egui")]
use moho_ui::EguiUi;
use std::sync::Arc;

#[test]
fn staging_belt_recall_no_panic() {
    let event_bus = Arc::new(moho_core::EventBus::new());
    let mut ui = EguiUi::new(None, event_bus);

    // Simulate calling recall_staging_belt which should be a no-op
    // if the belt is empty. This verifies no panics occur.
    ui.recall_staging_belt();
    ui.recall_staging_belt();
}
