use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use moho_core::events::{AudioEvent, EventBus, InputEvent, SystemEvent, UiEvent};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use std::thread;

/// Benchmark: Publishing events with no subscribers (baseline overhead)
fn bench_publish_no_subscribers(c: &mut Criterion) {
    c.bench_function("publish_no_subscribers", |b| {
        let bus = EventBus::new();
        b.iter(|| {
            bus.publish(black_box(UiEvent::MenuShown {
                name: "test".to_string(),
            }));
        });
    });
}

/// Benchmark: Publishing events with 1 subscriber
fn bench_publish_single_subscriber(c: &mut Criterion) {
    let bus = EventBus::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c_clone = counter.clone();

    bus.subscribe(move |_event: &UiEvent| {
        c_clone.fetch_add(1, Ordering::Relaxed);
    });

    c.bench_function("publish_single_subscriber", |b| {
        b.iter(|| {
            bus.publish(black_box(UiEvent::MenuShown {
                name: "test".to_string(),
            }));
        });
    });
}

/// Benchmark: Publishing events with multiple subscribers (1, 5, 10, 20)
fn bench_publish_multiple_subscribers(c: &mut Criterion) {
    let mut group = c.benchmark_group("publish_multi_subscriber");

    for subscriber_count in [1, 5, 10, 20].iter() {
        let bus = EventBus::new();

        for _ in 0..*subscriber_count {
            let counter = Arc::new(AtomicU32::new(0));
            bus.subscribe(move |_event: &UiEvent| {
                counter.fetch_add(1, Ordering::Relaxed);
            });
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(subscriber_count),
            subscriber_count,
            |b, _| {
                b.iter(|| {
                    bus.publish(black_box(UiEvent::MenuShown {
                        name: "test".to_string(),
                    }));
                });
            },
        );
    }
    group.finish();
}

/// Benchmark: High-frequency event publishing (simulating frame loop)
fn bench_high_frequency_events(c: &mut Criterion) {
    let bus = Arc::new(EventBus::new());
    let counter = Arc::new(AtomicU32::new(0));
    let c_clone = counter.clone();

    bus.subscribe(move |_event: &SystemEvent| {
        c_clone.fetch_add(1, Ordering::Relaxed);
    });

    c.bench_function("high_frequency_1000_events", |b| {
        b.iter(|| {
            for i in 0..1000 {
                bus.publish(black_box(SystemEvent::FrameStart {
                    frame_number: i,
                    delta_time: 0.016,
                }));
            }
        });
    });
}

/// Benchmark: Multi-threaded concurrent publishing
fn bench_concurrent_publishing(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_publishing");

    for thread_count in [2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(thread_count),
            thread_count,
            |b, &thread_count| {
                b.iter(|| {
                    let bus = Arc::new(EventBus::new());
                    let counter = Arc::new(AtomicU32::new(0));
                    let c_clone = counter.clone();

                    bus.subscribe(move |_event: &AudioEvent| {
                        c_clone.fetch_add(1, Ordering::Relaxed);
                    });

                    let mut handles = vec![];

                    for _ in 0..thread_count {
                        let bus_clone = bus.clone();
                        let handle = thread::spawn(move || {
                            for _ in 0..100 {
                                bus_clone.publish(black_box(AudioEvent::ButtonClick));
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

/// Benchmark: Event history recording overhead
fn bench_event_history(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_history");

    // With history disabled
    group.bench_function("history_disabled", |b| {
        let bus = EventBus::with_history(false, 100);
        b.iter(|| {
            bus.publish(black_box(UiEvent::MenuShown {
                name: "test".to_string(),
            }));
        });
    });

    // With history enabled
    group.bench_function("history_enabled", |b| {
        let bus = EventBus::with_history(true, 100);
        b.iter(|| {
            bus.publish(black_box(UiEvent::MenuShown {
                name: "test".to_string(),
            }));
        });
    });

    group.finish();
}

/// Benchmark: Different event types (small vs large payloads)
fn bench_event_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_sizes");

    // Small event (no data)
    group.bench_function("small_event", |b| {
        let bus = EventBus::new();
        b.iter(|| {
            bus.publish(black_box(AudioEvent::ButtonClick));
        });
    });

    // Medium event (some data)
    group.bench_function("medium_event", |b| {
        let bus = EventBus::new();
        b.iter(|| {
            bus.publish(black_box(InputEvent::KeyPressed { code: 65, mods: 0 }));
        });
    });

    // Large event (String data)
    group.bench_function("large_event", |b| {
        let bus = EventBus::new();
        b.iter(|| {
            bus.publish(black_box(UiEvent::MenuShown {
                name: "very_long_menu_name_with_lots_of_characters".to_string(),
            }));
        });
    });

    group.finish();
}

/// Benchmark: Subscribe operation overhead
fn bench_subscribe_overhead(c: &mut Criterion) {
    c.bench_function("subscribe_operation", |b| {
        let bus = EventBus::new();
        let mut counter = 0;

        b.iter(|| {
            counter += 1;
            let c = counter;
            bus.subscribe(move |_event: &UiEvent| {
                black_box(c);
            });
        });
    });
}

/// Benchmark: Realistic game loop simulation
fn bench_realistic_game_loop(c: &mut Criterion) {
    c.bench_function("realistic_game_loop", |b| {
        let bus = Arc::new(EventBus::with_history(true, 100));

        // Setup typical game subscribers
        let audio_counter = Arc::new(AtomicU32::new(0));
        let ui_counter = Arc::new(AtomicU32::new(0));
        let input_counter = Arc::new(AtomicU32::new(0));

        let ac = audio_counter.clone();
        bus.subscribe(move |_event: &AudioEvent| {
            ac.fetch_add(1, Ordering::Relaxed);
        });

        let uc = ui_counter.clone();
        bus.subscribe(move |_event: &UiEvent| {
            uc.fetch_add(1, Ordering::Relaxed);
        });

        let ic = input_counter.clone();
        bus.subscribe(move |_event: &InputEvent| {
            ic.fetch_add(1, Ordering::Relaxed);
        });

        let sc = Arc::new(AtomicU32::new(0));
        bus.subscribe(move |_event: &SystemEvent| {
            sc.fetch_add(1, Ordering::Relaxed);
        });

        b.iter(|| {
            // Simulate one frame with typical events
            bus.publish(black_box(SystemEvent::FrameStart {
                frame_number: 1,
                delta_time: 0.016,
            }));

            // User input
            bus.publish(black_box(InputEvent::MouseMoved {
                delta_x: 1.0,
                delta_y: 1.0,
            }));

            // UI interaction
            bus.publish(black_box(UiEvent::MenuShown {
                name: "inventory".to_string(),
            }));

            // Audio feedback
            bus.publish(black_box(AudioEvent::MenuNavigate));

            bus.publish(black_box(SystemEvent::FrameEnd { frame_number: 1 }));
        });
    });
}

criterion_group!(
    benches,
    bench_publish_no_subscribers,
    bench_publish_single_subscriber,
    bench_publish_multiple_subscribers,
    bench_high_frequency_events,
    bench_concurrent_publishing,
    bench_event_history,
    bench_event_sizes,
    bench_subscribe_overhead,
    bench_realistic_game_loop,
);

criterion_main!(benches);
