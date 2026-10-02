//! Integration tests for event bus
//!
//! Tests event flow between different systems and realistic usage patterns.

use moho_core::events::{AudioEvent, EventBus, InputEvent, SystemEvent, UiEvent};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[test]
fn test_ui_to_audio_event_flow() {
    let bus = Arc::new(EventBus::new());
    let audio_event_counter = Arc::new(AtomicU32::new(0));

    // Subscribe to audio events
    let counter = audio_event_counter.clone();
    bus.subscribe(move |_event: &AudioEvent| {
        counter.fetch_add(1, Ordering::Relaxed);
    });

    // Simulate UI triggering audio events
    bus.publish(AudioEvent::ButtonClick);
    bus.publish(AudioEvent::MenuNavigate);
    bus.publish(AudioEvent::Confirm);

    std::thread::sleep(std::time::Duration::from_millis(10));

    assert_eq!(audio_event_counter.load(Ordering::Relaxed), 3);
}

#[test]
fn test_frame_lifecycle_events() {
    let bus = Arc::new(EventBus::with_history(true, 100));
    let frame_counter = Arc::new(AtomicU32::new(0));

    // Subscribe to frame events to verify they're published
    let fc = frame_counter.clone();
    bus.subscribe(move |event: &SystemEvent| {
        if matches!(
            event,
            SystemEvent::FrameStart { .. } | SystemEvent::FrameEnd { .. }
        ) {
            fc.fetch_add(1, Ordering::Relaxed);
        }
    });

    // Simulate frame loop
    for frame in 0..5 {
        bus.publish(SystemEvent::FrameStart {
            frame_number: frame,
            delta_time: 0.016,
        });

        // Simulate some events during frame
        bus.publish(AudioEvent::ButtonClick);

        bus.publish(SystemEvent::FrameEnd {
            frame_number: frame,
        });
        bus.process_deferred();
    }

    std::thread::sleep(std::time::Duration::from_millis(10));

    // Verify frame events were published (5 FrameStart + 5 FrameEnd = 10)
    assert_eq!(frame_counter.load(Ordering::Relaxed), 10);

    let history = bus.history();

    // Frame events should NOT be in history (they override should_record to return false)
    // Only ButtonClick events should be recorded
    assert_eq!(
        history.len(),
        5,
        "Should have 5 ButtonClick events in history"
    );
    assert!(
        history.iter().all(|s| s.contains("ButtonClick")),
        "History should only contain ButtonClick events"
    );
}

#[test]
fn test_input_event_processing() {
    let bus = Arc::new(EventBus::new());
    let key_press_counter = Arc::new(AtomicU32::new(0));
    let mouse_counter = Arc::new(AtomicU32::new(0));

    let kpc = key_press_counter.clone();
    bus.subscribe(move |event: &InputEvent| {
        if matches!(event, InputEvent::KeyPressed { .. }) {
            kpc.fetch_add(1, Ordering::Relaxed);
        }
    });

    let mc = mouse_counter.clone();
    bus.subscribe(move |event: &InputEvent| {
        if matches!(event, InputEvent::MouseMoved { .. }) {
            mc.fetch_add(1, Ordering::Relaxed);
        }
    });

    // Simulate input events
    bus.publish(InputEvent::KeyPressed { code: 65, mods: 0 });
    bus.publish(InputEvent::MouseMoved {
        delta_x: 1.0,
        delta_y: -1.0,
    });
    bus.publish(InputEvent::KeyPressed { code: 87, mods: 0 });

    std::thread::sleep(std::time::Duration::from_millis(10));

    assert_eq!(key_press_counter.load(Ordering::Relaxed), 2);
    assert_eq!(mouse_counter.load(Ordering::Relaxed), 1);
}

#[test]
fn test_metrics_tracking() {
    let bus = EventBus::new();

    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    bus.subscribe(move |_event: &UiEvent| {
        c.fetch_add(1, Ordering::Relaxed);
    });

    bus.publish(UiEvent::MenuShown {
        name: "test".to_string(),
    });
    bus.publish(AudioEvent::ButtonClick);
    bus.publish(SystemEvent::Started);

    std::thread::sleep(std::time::Duration::from_millis(10));

    let metrics = bus.metrics();
    assert_eq!(metrics.total_published, 3);
    assert_eq!(metrics.total_processed, 1); // Only UiEvent subscriber
}

#[test]
fn test_real_world_game_loop() {
    let bus = Arc::new(EventBus::with_history(true, 100));

    let audio_events = Arc::new(AtomicU32::new(0));
    let ui_events = Arc::new(AtomicU32::new(0));

    // Set up subscribers
    let ae = audio_events.clone();
    bus.subscribe(move |_event: &AudioEvent| {
        ae.fetch_add(1, Ordering::Relaxed);
    });

    let ue = ui_events.clone();
    bus.subscribe(move |_event: &UiEvent| {
        ue.fetch_add(1, Ordering::Relaxed);
    });

    // Simulate game loop
    for frame in 0..3 {
        // Frame start
        bus.publish(SystemEvent::FrameStart {
            frame_number: frame,
            delta_time: 0.016,
        });

        // Simulate user clicking menu button
        if frame == 1 {
            bus.publish(UiEvent::MenuShown {
                name: "pause".to_string(),
            });
            bus.publish(AudioEvent::MenuNavigate);
        }

        // Frame end
        bus.publish(SystemEvent::FrameEnd {
            frame_number: frame,
        });
        bus.process_deferred();
    }

    std::thread::sleep(std::time::Duration::from_millis(20));

    assert_eq!(audio_events.load(Ordering::Relaxed), 1);
    assert_eq!(ui_events.load(Ordering::Relaxed), 1);

    // Check history
    let history = bus.history();
    assert!(!history.is_empty());
}
