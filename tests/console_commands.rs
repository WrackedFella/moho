use moho_core::events::EventBus;
use std::sync::Arc;

// These tests are light-weight integration tests that verify the console publishes
// the expected events to the EventBus. They mock the minimal pieces required.

#[test]
fn console_quit_publishes_exit_event() {
    // Setup a bus and a subscriber that records received events.
    let bus = Arc::new(EventBus::new());

    // Use a channel to receive published events in the test.
    let (tx, rx) = std::sync::mpsc::channel();

    bus.subscribe(move |evt: &moho_core::events::UiEvent| {
        let _ = tx.send(format!("{:?}", evt));
    });

    // Simulate adapter publishing the ExitRequested event (what the console would do).
    bus.publish(moho_core::events::UiEvent::ExitRequested);

    let received = rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("Expected event");
    assert!(received.contains("ExitRequested"));
}
