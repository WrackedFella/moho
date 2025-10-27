# Event Bus Best Practices

## Quick Start

### Basic Usage

```rust
use moho_core::events::{EventBus, UiEvent};
use std::sync::Arc;

// Create event bus
let bus = Arc::new(EventBus::new());

// Subscribe to events
let bus_clone = bus.clone();
bus_clone.subscribe(move |event: &UiEvent| {
    println!("UI Event: {:?}", event);
});

// Publish events
bus.publish(UiEvent::MenuShown { name: "main".to_string() });
```

### With Priority

```rust
// High priority handler (runs first)
bus.subscribe_with_priority(
    |event: &UiEvent| {
        // Critical handler
    },
    -10  // Negative = high priority
);

// Normal priority
bus.subscribe(|event: &UiEvent| {
    // Regular handler
});

// Low priority handler (runs last)
bus.subscribe_with_priority(
    |event: &UiEvent| {
        // Cleanup handler
    },
    10  // Positive = low priority
);
```

### With History (Debugging)

```rust
// Enable history for debugging
let bus = EventBus::with_history(true, 100);

// ... publish events ...

// Check recent events
let history = bus.recent_history(10);
for event in history {
    println!("{}", event);
}
```

---

## Design Patterns

### ✅ Pattern 1: Channel Collection (Recommended for Cascading)

**Problem**: Publishing from a handler causes deadlock.

**Solution**: Use channels to defer event publishing.

```rust
use std::sync::mpsc;

let (tx, rx) = mpsc::channel();

// Handler sends to channel instead of publishing
bus.subscribe(move |event: &UiEvent| {
    if matches!(event, UiEvent::MenuShown { .. }) {
        tx.send(AudioEvent::MenuNavigate).ok();
    }
});

// In frame loop, publish collected events
for audio_event in rx.try_iter() {
    bus.publish(audio_event);
}
```

### ✅ Pattern 2: Atomic Counters for State

```rust
use std::sync::atomic::{AtomicU32, Ordering};

let counter = Arc::new(AtomicU32::new(0));
let c = counter.clone();

bus.subscribe(move |_event: &UiEvent| {
    c.fetch_add(1, Ordering::Relaxed);
});

// Later, check counter
println!("UI events: {}", counter.load(Ordering::Relaxed));
```

### ✅ Pattern 3: Frame-Consistent Processing

```rust
// At start of frame
bus.publish(SystemEvent::FrameStart {
    frame_number: frame,
    delta_time: dt,
});

// ... game logic publishes events ...

// At end of frame, process deferred
bus.publish(SystemEvent::FrameEnd { frame_number: frame });
bus.process_deferred();  // Future: will execute deferred events
```

### ✅ Pattern 4: Non-Send Types (Audio System)

**Problem**: `AudioSystem` is not `Send + Sync` (rodio limitation).

**Solution**: Keep on main thread, use channels for events.

```rust
// Create channel for audio events
let (audio_tx, audio_rx) = mpsc::channel();

// Subscribe to audio events, forward to channel
bus.subscribe(move |event: &AudioEvent| {
    audio_tx.send(event.clone()).ok();
});

// In main thread game loop
for audio_event in audio_rx.try_iter() {
    audio_system.handle_event(&audio_event);  // Main thread only
}
```

---

## Event Design Guidelines

### ✅ DO: Use Small Payloads for High-Frequency Events

```rust
// Good - small, fast to clone
#[derive(Clone, Debug)]
pub enum InputEvent {
    KeyPressed { code: u32, mods: u8 },  // 5 bytes
    MouseMoved { delta_x: f32, delta_y: f32 },  // 8 bytes
}
```

### ✅ DO: Use Static Strings When Possible

```rust
// Good - no allocation
pub enum AudioEvent {
    PlaySound { sound_id: &'static str },  // Zero-cost clone
}

// Use like this
bus.publish(AudioEvent::PlaySound { sound_id: "button_click" });
```

### ❌ DON'T: Use Large Strings in High-Frequency Events

```rust
// Bad - allocates every frame
pub enum DebugEvent {
    Log { message: String },  // Expensive clone
}

// If needed, use Cow or &'static str
use std::borrow::Cow;

pub enum DebugEvent {
    Log { message: Cow<'static, str> },  // Better
}
```

### ✅ DO: Disable History for High-Frequency Events

```rust
impl Event for SystemEvent {
    fn should_record(&self) -> bool {
        // Don't record frame events (60+/sec)
        !matches!(self, SystemEvent::FrameStart { .. })
    }
}
```

### ✅ DO: Use Entity IDs Instead of Full Objects

```rust
// Good - just an ID
pub enum GameEvent {
    EntitySpawned { entity_id: u64 },
}

// Bad - cloning entire entity data
pub enum GameEvent {
    EntitySpawned { entity: Entity },  // Expensive!
}
```

---

## Performance Best Practices

### Frame Budget Awareness

At 60 FPS (16.66ms per frame):
- Event bus overhead: **~0.9 µs per 5 events** (0.0054% of frame)
- Budget remaining: 99.99% for game logic

**Guideline**: Keep events under 1000/frame for <1% overhead.

### History Overhead

```rust
// Disable history in production
let bus = EventBus::new();  // History disabled by default

// Enable only for debugging
#[cfg(debug_assertions)]
let bus = EventBus::with_history(true, 100);

#[cfg(not(debug_assertions))]
let bus = EventBus::new();
```

### Subscriber Counts

- **1-5 subscribers**: Optimal (~400-450 ns per event)
- **10 subscribers**: Good (~480 ns per event)
- **20 subscribers**: Acceptable (~590 ns per event)
- **50+ subscribers**: Consider event filtering or redesign

### Thread Safety

```rust
// Event bus is Send + Sync, safe to share
let bus = Arc::new(EventBus::new());

// Can publish from multiple threads
let bus_clone = bus.clone();
std::thread::spawn(move || {
    bus_clone.publish(AudioEvent::ButtonClick);  // Safe!
});
```

---

## Common Pitfalls

### ❌ PITFALL 1: Publishing from Handler (Deadlock)

```rust
// DON'T DO THIS - DEADLOCK!
let bus_clone = bus.clone();
bus.subscribe(move |_event: &UiEvent| {
    bus_clone.publish(AudioEvent::Confirm);  // 💀 DEADLOCK
});
```

**Solution**: Use channel pattern (see Pattern 1 above).

### ❌ PITFALL 2: Capturing Mutable State

```rust
// DON'T DO THIS - Won't compile
let mut count = 0;
bus.subscribe(move |_event: &UiEvent| {
    count += 1;  // ❌ Error: closure may outlive current function
});
```

**Solution**: Use Arc<AtomicU32> (see Pattern 2 above).

### ❌ PITFALL 3: Forgetting to Clone Arc

```rust
// DON'T DO THIS
fn setup_handlers(bus: EventBus) {  // Takes ownership!
    bus.subscribe(|_| {});  // Moves bus here
    // bus is now moved, can't use elsewhere
}

// DO THIS
fn setup_handlers(bus: Arc<EventBus>) {  // Shared ownership
    bus.subscribe(|_| {});  // Arc is Clone
    // bus still usable
}
```

### ❌ PITFALL 4: Over-using History

```rust
// DON'T DO THIS in production
let bus = EventBus::with_history(true, 10000);  // Wastes 10K×String memory

// DO THIS
let bus = EventBus::new();  // Or small history for debugging
```

---

## Testing Patterns

### Unit Test with Atomic Counter

```rust
#[test]
fn test_my_event() {
    let bus = EventBus::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    
    bus.subscribe(move |_event: &MyEvent| {
        c.fetch_add(1, Ordering::Relaxed);
    });
    
    bus.publish(MyEvent::Something);
    
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
}
```

### Integration Test with Multiple Systems

```rust
#[test]
fn test_ui_to_audio_flow() {
    let bus = Arc::new(EventBus::new());
    let (audio_tx, audio_rx) = mpsc::channel();
    
    // UI system triggers audio
    bus.subscribe(move |event: &UiEvent| {
        if matches!(event, UiEvent::MenuShown { .. }) {
            audio_tx.send(AudioEvent::MenuNavigate).ok();
        }
    });
    
    bus.publish(UiEvent::MenuShown { name: "test".to_string() });
    
    let audio_event = audio_rx.recv_timeout(Duration::from_millis(100)).unwrap();
    assert!(matches!(audio_event, AudioEvent::MenuNavigate));
}
```

---

## Debugging Tips

### 1. Enable History

```rust
let bus = EventBus::with_history(true, 100);

// ... run code ...

// Print all events
for event in bus.history() {
    println!("{}", event);
}
```

### 2. Check Metrics

```rust
let metrics = bus.metrics();
println!("Published: {}", metrics.total_published);
println!("Processed: {}", metrics.total_processed);

// If processed < published, some events have no subscribers
```

### 3. Priority Debugging

```rust
// Add debug handlers at different priorities
bus.subscribe_with_priority(|e: &UiEvent| {
    println!("[HIGH] {:?}", e);
}, -10);

bus.subscribe_with_priority(|e: &UiEvent| {
    println!("[LOW] {:?}", e);
}, 10);

// Output shows execution order
```

---

## Migration Guide

### From Manual Channels

**Before**:
```rust
let (ui_tx, ui_rx) = mpsc::channel();

// In UI code
ui_tx.send(UiEvent::MenuShown).ok();

// In main loop
for event in ui_rx.try_iter() {
    handle_ui_event(event);
}
```

**After**:
```rust
let bus = Arc::new(EventBus::new());

// Subscribe once
bus.subscribe(|event: &UiEvent| {
    handle_ui_event(event);
});

// In UI code
bus.publish(UiEvent::MenuShown { name: "menu".to_string() });
```

---

## Performance Targets

Based on benchmark results:

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Single event | <500 ns | 389 ns | ✅ |
| 1000 events | <100 µs | 87.6 µs | ✅ |
| 20 subscribers | <1 µs | 588 ns | ✅ |
| Frame budget (60 FPS) | <1% | 0.005% | ✅ |
| Concurrent 8 threads | <2 ms | 702 µs | ✅ |

**Conclusion**: Event bus meets or exceeds all performance targets.

---

## Further Reading

- **Testing Notes**: `docs/engine_core/EVENT_BUS_TESTING_NOTES.md`
- **Performance Analysis**: `docs/engine_core/EVENT_BUS_PERFORMANCE.md`
- **Implementation Plan**: `todo/EVENT_BUS.md`
- **API Documentation**: Run `cargo doc --open`

---

**Last Updated**: October 26, 2025  
**Status**: Production Ready
