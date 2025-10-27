use moho_core::events::EventBus;
use std::sync::{Arc, Mutex};

#[test]
fn eventbus_multiple_subscribers_receive_events() {
    let bus = Arc::new(EventBus::new());

    let received_a = Arc::new(Mutex::new(false));
    let received_b = Arc::new(Mutex::new(false));

    let a = received_a.clone();
    bus.subscribe(move |_evt: &moho_core::events::DebugEvent| {
        let mut g = a.lock().unwrap();
        *g = true;
    });

    let b = received_b.clone();
    bus.subscribe(move |_evt: &moho_core::events::DebugEvent| {
        let mut g = b.lock().unwrap();
        *g = true;
    });

    // Publish a debug event
    bus.publish(moho_core::events::DebugEvent::ToggleGodMode { enabled: true });

    // Allow subscribers a moment to run (they run inline in current implementation)
    std::thread::sleep(std::time::Duration::from_millis(50));

    assert!(*received_a.lock().unwrap());
    assert!(*received_b.lock().unwrap());
}
