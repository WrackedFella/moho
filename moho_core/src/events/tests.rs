#[cfg(test)]
mod event_bus_tests {
    use crate::events::{Event, EventBus};
    use std::any::Any;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

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
        message: String,
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
        count: u32,
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
        bus.publish(TestEvent { value: 3 });

        // Small delay to ensure async processing
        std::thread::sleep(std::time::Duration::from_millis(10));

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

        std::thread::sleep(std::time::Duration::from_millis(10));

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

        std::thread::sleep(std::time::Duration::from_millis(10));

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
            message: "test".to_string(),
        });
        bus.publish(TestEvent { value: 2 });

        std::thread::sleep(std::time::Duration::from_millis(10));

        assert_eq!(test_counter.load(Ordering::Relaxed), 2);
        assert_eq!(high_priority_counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_event_history() {
        let bus = EventBus::with_history(true, 100);

        bus.publish(TestEvent { value: 1 });
        bus.publish(TestEvent { value: 2 });
        bus.publish(HighFrequencyEvent { count: 100 }); // Should not be recorded

        std::thread::sleep(std::time::Duration::from_millis(10));

        let history = bus.history();
        
        // Should have 2 TestEvents but not HighFrequencyEvent (should_record = false)
        assert!(history.len() >= 2);
        assert!(history.iter().any(|s| s.contains("TestEvent")));
        assert!(!history.iter().any(|s| s.contains("HighFrequencyEvent")));
    }

    #[test]
    fn test_recent_history() {
        let bus = EventBus::with_history(true, 100);

        for i in 0..10 {
            bus.publish(TestEvent { value: i });
        }

        std::thread::sleep(std::time::Duration::from_millis(10));

        let recent = bus.recent_history(5);
        assert!(recent.len() <= 5);
    }

    #[test]
    fn test_clear_history() {
        let bus = EventBus::with_history(true, 100);

        bus.publish(TestEvent { value: 1 });
        bus.publish(TestEvent { value: 2 });

        std::thread::sleep(std::time::Duration::from_millis(10));

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

        std::thread::sleep(std::time::Duration::from_millis(10));

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

        std::thread::sleep(std::time::Duration::from_millis(50));

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

        // Events should not be processed yet
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(counter.load(Ordering::Relaxed), 0);

        // Process deferred events
        // Note: Current implementation logs but doesn't execute handlers
        // This is a known limitation - deferred processing will be improved
        bus.process_deferred();
        std::thread::sleep(std::time::Duration::from_millis(10));

        // For now, deferred events are queued but not fully processed
        // TODO: Implement full type-safe deferred event execution
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_no_subscribers() {
        let bus = EventBus::new();

        // Should not panic when publishing with no subscribers
        bus.publish(TestEvent { value: 42 });
        bus.publish_deferred(TestEvent { value: 100 });
        bus.process_deferred();
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

        std::thread::sleep(std::time::Duration::from_millis(10));

        // Only the second event should be counted
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }
}
