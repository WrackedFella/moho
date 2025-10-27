# moho_core

Core engine types and utilities for the Moho game engine.

## Features

- **Event Bus System** - Type-safe pub/sub for decoupled system communication
- **Voxel System** - Chunk-based voxel terrain with face culling
- **Materials & Resources** - Registration and management systems
- **Camera** - Orbital camera with configurable controls
- **Input System** - Keyboard, mouse, and gamepad input handling

## Event Bus

The event bus provides thread-safe, type-safe communication between game systems:

```rust
use moho_core::events::{EventBus, UiEvent};
use std::sync::Arc;

let bus = Arc::new(EventBus::new());

bus.subscribe(|event: &UiEvent| {
    println!("UI Event: {:?}", event);
});

bus.publish(UiEvent::MenuShown { name: "main".to_string() });
```

**Performance**: <1% frame budget at 60 FPS, 11.4M events/second throughput

### Event Types

- `SystemEvent` - Lifecycle, frames, errors
- `UiEvent` - Menus, buttons, forms, overlays
- `AudioEvent` - Sounds, music, voice
- `InputEvent` - Keyboard, mouse, gamepad
- `GameEvent` - Player actions, objectives
- `PhysicsEvent` - Collisions, triggers
- `GraphicsEvent` - Rendering, camera
- `WorldEvent` - Chunk loading, streaming
- `DebugEvent` - Logging, profiling
- `NetworkEvent` - Multiplayer, sync

### Documentation

See `docs/engine_core/` for detailed documentation:
- **[EVENT_BUS_BEST_PRACTICES.md](../docs/engine_core/EVENT_BUS_BEST_PRACTICES.md)** - Usage patterns
- **[EVENT_BUS_PERFORMANCE.md](../docs/engine_core/EVENT_BUS_PERFORMANCE.md)** - Benchmarks
- **[CONCEPTS.md](../docs/engine_core/CONCEPTS.md)** - Architecture notes

## Tests

Unit tests are located in:
- `moho_core/src/events/tests.rs` - Event bus tests (12 tests)
- `moho_core/tests/` - Integration tests (instance POD, voxel system)

Run tests:
```sh
cargo test --package moho_core
```
