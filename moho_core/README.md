# moho_core

Core engine types and foundational systems for the Moho game engine. This crate provides the fundamental building blocks used across the engine.

## Core Systems

### Event Bus

Thread-safe, type-safe publish/subscribe system for decoupled communication between engine systems.

**Key Features:**
- Zero external dependencies (std lib only)
- Priority-based handler execution
- Optional event history for debugging
- 11.4M events/second throughput
- <1% frame budget at 60 FPS

**Quick Example:**
```rust
use moho_core::events::{EventBus, UiEvent};
use std::sync::Arc;

let bus = Arc::new(EventBus::new());

// Subscribe to events
bus.subscribe(|event: &UiEvent| {
    println!("Received: {:?}", event);
});

// Publish events
bus.publish(UiEvent::MenuShown { name: "main".to_string() });
```

**Event Types:**
- `SystemEvent` - Lifecycle, frames, errors
- `UiEvent` - Menus, buttons, forms, overlays
- `AudioEvent` - Sounds, music, voice
- `InputEvent` - Keyboard, mouse, gamepad
- `GameEvent` - Player actions, objectives
- `PhysicsEvent` - Collisions, triggers
- `GraphicsEvent` - Rendering, camera, lighting
- `WorldEvent` - Chunk loading, streaming
- `DebugEvent` - Logging, profiling
- `NetworkEvent` - Multiplayer, sync

**Priority System:**
```rust
// High priority (-10) runs first
bus.subscribe_with_priority(|event: &InputEvent| {
    // UI input capture
}, -10);

// Normal priority (0) - default
bus.subscribe(|event: &InputEvent| {
    // Game input handling
});

// Low priority (10) runs last
bus.subscribe_with_priority(|event: &InputEvent| {
    // Logging/analytics
}, 10);
```

**Important Limitation:** Publishing from within a handler causes RwLock deadlock. Use channels to defer cascading events:

```rust
use std::sync::mpsc;

let (tx, rx) = mpsc::channel();

bus.subscribe(move |event: &UiEvent| {
    if matches!(event, UiEvent::ButtonClicked { .. }) {
        tx.send(AudioEvent::ButtonClick).ok();
    }
});

// In frame loop:
for audio_event in rx.try_iter() {
    bus.publish(audio_event);
}
```

See `../docs/engine_core/EVENT_BUS_BEST_PRACTICES.md` for detailed usage patterns.

### GameClock

24-hour day/night cycle system with asymmetric timescale (day vs night).

**Features:**
- Configurable day/night duration (default: 600s day, 420s night)
- Celestial body calculations (sun and moon positioning)
- Time-of-day queries and manipulation
- Parabolic arc for realistic sun/moon elevation

**Usage:**
```rust
use moho_core::game_clock::GameClock;

let mut clock = GameClock::new(600.0, 420.0); // 10 min day, 7 min night

// Advance time
clock.tick(delta_time);

// Query time
let hour = clock.time_of_day(); // 0.0-24.0
let time_str = clock.time_string(); // "14:30"

// Check day/night
if clock.is_daytime() {
    // Daytime logic (5:00-21:00)
} else {
    // Nighttime logic (21:00-5:00)
}

// Get celestial positions
let (sun_dir, moon_dir) = clock.celestial_directions();
```

**Time Manipulation:**
```rust
clock.set_time_of_day(12.0); // Set to noon
clock.set_time_of_day(0.0);  // Set to midnight
```

### Voxel System

Chunk-based voxel terrain with face culling optimization.

**Architecture:**
- `VoxelGrid` - Stores block data in 3D grid
- `VoxelChunk` - Fixed-size chunk (typically 64×64×64)
- `VoxelBlock` - Individual block with material/resource data
- Face culling - Only render visible faces (87% triangle reduction)

**Usage:**
```rust
use moho_core::voxel::{VoxelGrid, VoxelBlock, Material};

// Create a grid
let mut grid = VoxelGrid::new(64, 64, 64);

// Set blocks
grid.set_block(32, 10, 32, VoxelBlock {
    material: Material::Grass,
    resource: None,
});

// Check neighbors for culling
if grid.should_render_face(x, y, z, FaceDirection::Top) {
    // Render this face
}

// Extract visible faces for rendering
let visible_faces = grid.extract_visible_faces(chunk_x, chunk_y, chunk_z);
```

**Face Culling Logic:**
- Faces adjacent to solid blocks are culled
- Only exposed faces are rendered
- Significant performance improvement for dense terrain

### Camera

Orbital camera with configurable movement and rotation.

**Features:**
- Target-based orbiting
- Configurable movement speed
- Mouse look and WASD controls
- View/projection matrix generation

**Usage:**
```rust
use moho_core::camera::Camera;

let mut camera = Camera::new(
    glam::Vec3::new(0.0, 50.0, 100.0), // position
    glam::Vec3::ZERO,                   // target
);

// Update based on input
camera.process_movement(forward, right, up, delta_time);
camera.process_mouse(delta_x, delta_y);

// Get matrices for rendering
let view = camera.view_matrix();
let proj = camera.projection_matrix(aspect_ratio);
```

### Materials & Resources

Type-safe material and resource registration system.

**Materials:**
```rust
pub enum Material {
    Air,
    Grass,
    Dirt,
    Stone,
    // ... more materials
}
```

**Resources:**
```rust
pub enum ResourceType {
    None,
    Stone,
    IronOre,
    Coal,
    // ... more resources
}
```

### Input System

Low-level input accumulation and sensitivity scaling.

**Usage:**
```rust
use moho_core::input::InputState;

let mut input = InputState::new();

// Accumulate input each frame
input.add_movement(delta_x, delta_y);

// Apply sensitivity and reset
if input.has_pending_input() {
    let (scaled_x, scaled_y) = input.take_movement(sensitivity);
    camera.process_mouse(scaled_x, scaled_y);
}
```

## Testing

```bash
# All tests
cargo test --package moho_core

# Event bus only
cargo test --package moho_core events::tests

# Voxel system only
cargo test --test voxel_system

# Performance benchmarks
cargo bench --bench event_bus_bench
```

**Test Coverage:**
- 24 unit tests (event bus, game clock, input)
- 6 integration tests (voxel system, instance POD)
- 8 doc tests (event bus examples)

## Performance Characteristics

- **Event Bus**: <0.01% frame time (60 FPS target)
- **Face Culling**: 87% triangle reduction
- **GameClock**: <0.1ms per frame
- **Voxel Queries**: O(1) block access

## Thread Safety

- **EventBus**: `Send + Sync` (Arc<RwLock<_>>)
- **GameClock**: Not thread-safe (single-threaded update)
- **VoxelGrid**: Not thread-safe (chunk generation is parallelizable externally)
- **Camera**: Not thread-safe (owned by main thread)
