use moho_ui::EguiUi;

#[test]
fn staging_belt_finish_and_recall_no_panic() {
    let (mut ui, _receiver) = EguiUi::new(None);
    // Ensure a staging belt is installed for the adapter
    ui.test_set_staging_belt();

    // Simulate finishing the belt path: call recall_staging_belt which should
    // be a no-op if the belt is empty. This verifies no panics occur.
    ui.recall_staging_belt();
}
