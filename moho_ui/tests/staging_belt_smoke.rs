use moho_ui::EguiUi;

#[test]
fn staging_belt_recall_no_panic() {
    let (mut ui, _receiver) = EguiUi::new(None);

    // Simulate calling recall_staging_belt which should be a no-op
    // if the belt is empty. This verifies no panics occur.
    ui.recall_staging_belt();
    ui.recall_staging_belt();
}
