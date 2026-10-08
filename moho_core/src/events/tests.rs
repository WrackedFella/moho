#[cfg(test)]
mod event_bus_tests {
    use crate::events::{AudioEvent, Event, EventBus, InputEvent, SystemEvent, UiEvent};
    use std::any::Any;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    // Test event types
    #[derive(Clone, Debug)]
    struct TestEvent {
        value: i32,
    }

    impl Event for TestEvent {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Clone, Debug)]
    struct HighPriorityEvent {
        _message: String,
    }

    impl Event for HighPriorityEvent {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn priority(&self) -> i32 {
            -10 // Higher priority (lower number)
        }
    }

    #[derive(Clone, Debug)]
    struct HighFrequencyEvent {
        _count: u32,
    }

    impl Event for HighFrequencyEvent {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn should_record(&self) -> bool {
            false // Don't record in history
        }
    }

    #[test]
    fn test_basic_publish_subscribe() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        bus.subscribe(move |event: &TestEvent| {
            counter_clone.fetch_add(event.value as u32, Ordering::Relaxed);
        });

        bus.publish(TestEvent { value: 5 });
        assert_eq!(counter.load(Ordering::Relaxed), 5);

        bus.publish(TestEvent { value: 3 });
        assert_eq!(counter.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn test_multiple_subscribers() {
        let bus = EventBus::new();
        let counter1 = Arc::new(AtomicU32::new(0));
        let counter2 = Arc::new(AtomicU32::new(0));

        let c1 = counter1.clone();
        let c2 = counter2.clone();

        bus.subscribe(move |event: &TestEvent| {
            c1.fetch_add(event.value as u32, Ordering::Relaxed);
        });

        bus.subscribe(move |event: &TestEvent| {
            c2.fetch_add(event.value as u32 * 2, Ordering::Relaxed);
        });

        bus.publish(TestEvent { value: 10 });

        assert_eq!(counter1.load(Ordering::Relaxed), 10);
        assert_eq!(counter2.load(Ordering::Relaxed), 20);
    }

    #[test]
    fn test_handler_priority() {
        let bus = EventBus::new();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        let order1 = order.clone();
        let order2 = order.clone();
        let order3 = order.clone();

        // Subscribe with different priorities
        bus.subscribe_with_priority(
            move |_event: &TestEvent| {
                order1.lock().unwrap().push(3);
            },
            10, // Low priority
        );

        bus.subscribe_with_priority(
            move |_event: &TestEvent| {
                order2.lock().unwrap().push(1);
            },
            -10, // High priority
        );

        bus.subscribe_with_priority(
            move |_event: &TestEvent| {
                order3.lock().unwrap().push(2);
            },
            0, // Normal priority
        );

        bus.publish(TestEvent { value: 42 });

        let execution_order = order.lock().unwrap();
        assert_eq!(*execution_order, vec![1, 2, 3]);
    }

    #[test]
    fn test_different_event_types() {
        let bus = EventBus::new();
        let test_counter = Arc::new(AtomicU32::new(0));
        let high_priority_counter = Arc::new(AtomicU32::new(0));

        let tc = test_counter.clone();
        let hpc = high_priority_counter.clone();

        bus.subscribe(move |_event: &TestEvent| {
            tc.fetch_add(1, Ordering::Relaxed);
        });

        bus.subscribe(move |_event: &HighPriorityEvent| {
            hpc.fetch_add(1, Ordering::Relaxed);
        });

        bus.publish(TestEvent { value: 1 });
        bus.publish(HighPriorityEvent {
            _message: "test".to_string(),
        });
        bus.publish(TestEvent { value: 2 });

        assert_eq!(test_counter.load(Ordering::Relaxed), 2);
        assert_eq!(high_priority_counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_event_history() {
        let bus = EventBus::with_history(true, 100);

        bus.publish(TestEvent { value: 1 });
        bus.publish(TestEvent { value: 2 });
        bus.publish(HighFrequencyEvent { _count: 100 }); // Should not be recorded

        let history = bus.history();

        assert_eq!(
            history,
            ["TestEvent { value: 1 }", "TestEvent { value: 2 }"]
        );
    }

    #[test]
    fn test_clear_history() {
        let bus = EventBus::with_history(true, 100);

        bus.publish(TestEvent { value: 1 });
        bus.publish(TestEvent { value: 2 });

        assert!(!bus.history().is_empty());

        bus.clear_history();
        assert!(bus.history().is_empty());
    }

    #[test]
    fn test_metrics() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        bus.subscribe(move |_event: &TestEvent| {
            c.fetch_add(1, Ordering::Relaxed);
        });

        bus.publish(TestEvent { value: 1 });
        bus.publish(TestEvent { value: 2 });
        bus.publish(TestEvent { value: 3 });

        let metrics = bus.metrics();
        assert_eq!(metrics.total_published, 3);
        assert_eq!(metrics.total_processed, 3);
    }

    #[test]
    fn test_thread_safety() {
        let bus = Arc::new(EventBus::new());
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        bus.subscribe(move |event: &TestEvent| {
            c.fetch_add(event.value as u32, Ordering::Relaxed);
        });

        let mut handles = vec![];

        // Spawn multiple threads publishing events
        for i in 0..10 {
            let bus_clone = bus.clone();
            let handle = std::thread::spawn(move || {
                for _ in 0..10 {
                    bus_clone.publish(TestEvent { value: i });
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Each thread publishes 10 events with values 0-9
        // Total = 0*10 + 1*10 + 2*10 + ... + 9*10 = (0+1+2+...+9) * 10 = 45 * 10 = 450
        assert_eq!(counter.load(Ordering::Relaxed), 450);
    }

    #[test]
    fn test_deferred_events() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        bus.subscribe(move |event: &TestEvent| {
            c.fetch_add(event.value as u32, Ordering::Relaxed);
        });

        // Publish deferred events
        bus.publish_deferred(TestEvent { value: 1 });
        bus.publish_deferred(TestEvent { value: 2 });

        assert_eq!(counter.load(Ordering::Relaxed), 0);

        bus.process_deferred();
        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_no_subscribers() {
        let bus = EventBus::new();

        // Should not panic when publishing with no subscribers
        bus.publish(TestEvent { value: 42 });
        bus.publish_deferred(TestEvent { value: 100 });
        bus.process_deferred();

        let metrics = bus.metrics();
        assert_eq!(metrics.total_published, 2);
        assert_eq!(metrics.total_processed, 0);
    }

    #[test]
    fn test_subscriber_after_publish() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicU32::new(0));

        // Publish before subscribing
        bus.publish(TestEvent { value: 1 });

        let c = counter.clone();
        bus.subscribe(move |event: &TestEvent| {
            c.fetch_add(event.value as u32, Ordering::Relaxed);
        });

        // This event should be received
        bus.publish(TestEvent { value: 5 });

        // Only the second event should be counted
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn frame_and_mouse_move_events_are_not_recorded() {
        let bus = EventBus::with_history(true, 100);

        bus.publish(SystemEvent::FrameStart {
            frame_number: 0,
            delta_time: 0.016,
        });
        bus.publish(SystemEvent::FrameEnd { frame_number: 0 });
        bus.publish(SystemEvent::Started);
        bus.publish(InputEvent::MouseMoved {
            delta_x: 1.0,
            delta_y: -1.0,
        });
        bus.publish(InputEvent::KeyPressed { code: 65, mods: 0 });

        assert_eq!(
            bus.history(),
            ["Started", "KeyPressed { code: 65, mods: 0 }"]
        );
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

        let metrics = bus.metrics();
        assert_eq!(metrics.total_published, 3);
        assert_eq!(metrics.total_processed, 1); // Only the UiEvent subscriber
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn history_is_off_by_default() {
        let bus = EventBus::new();

        bus.publish(TestEvent { value: 1 });
        bus.publish_deferred(TestEvent { value: 2 });

        assert_eq!(bus.history(), Vec::<String>::new());
    }

    #[test]
    fn history_cap_evicts_oldest() {
        let bus = EventBus::with_history(true, 3);

        for value in 1..=5 {
            bus.publish(TestEvent { value });
        }

        assert_eq!(
            bus.history(),
            [
                "TestEvent { value: 3 }",
                "TestEvent { value: 4 }",
                "TestEvent { value: 5 }"
            ]
        );
    }

    #[test]
    fn deferred_history_cap_evicts_oldest() {
        let bus = EventBus::with_history(true, 3);

        for value in 1..=5 {
            bus.publish_deferred(TestEvent { value });
        }

        assert_eq!(
            bus.history(),
            [
                "TestEvent { value: 3 }",
                "TestEvent { value: 4 }",
                "TestEvent { value: 5 }"
            ]
        );
    }

    #[test]
    fn deferred_events_dispatch_in_fifo_order_once() {
        let bus = EventBus::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        bus.subscribe(move |event: &TestEvent| {
            sink.lock().unwrap().push(event.value);
        });

        bus.publish_deferred(TestEvent { value: 1 });
        bus.publish_deferred(TestEvent { value: 2 });
        bus.publish_deferred(TestEvent { value: 3 });
        assert_eq!(*seen.lock().unwrap(), Vec::<i32>::new());

        bus.process_deferred();
        assert_eq!(*seen.lock().unwrap(), vec![1, 2, 3]);

        bus.process_deferred();
        assert_eq!(*seen.lock().unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn deferred_event_is_recorded_when_deferred() {
        let bus = EventBus::with_history(true, 100);

        bus.publish_deferred(TestEvent { value: 7 });

        assert_eq!(bus.history(), ["TestEvent { value: 7 }"]);
        bus.process_deferred();
        assert_eq!(bus.history(), ["TestEvent { value: 7 }"]);
    }
}
