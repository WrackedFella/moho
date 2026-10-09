[![CI](https://github.com/WrackedFella/moho/actions/workflows/ci.yml/badge.svg)](https://github.com/WrackedFella/moho/actions/workflows/ci.yml)

# Moho — A Modular Voxel Game Engine (Rust)

## Overview

Moho is a modular Rust workspace containing engine components designed for voxel-based games:

- **moho_core** - Event bus, materials, preferences, persistence
- **moho_voxel** - Voxel terrain for the strategy line: grid, meshing, lighting, streaming
- **moho_renderer** - WGPU-based rendering backend  
- **moho_audio** - Audio playback (rodio-based)
- **moho_ui** - egui integration for menus and overlays
- **moho_input** - Input mapping and state management

The project emphasizes **separation of concerns** between simulation and rendering, making testing, snapshots, and future multiplayer functionality easier.

### Key Features

✅ **Day/Night Cycle** - 24-hour clock with moving sun/moon, dynamic sky colors, and moonlight  
✅ **Event Bus System** - Type-safe pub/sub for decoupled system communication  
✅ **Voxel Terrain** - Chunk-based world with face culling (87% triangle reduction)  
✅ **Cascaded Shadow Maps** - High-quality shadows with 4 cascades (4096×4096)  
✅ **Deterministic Simulation** - Snapshot/restore with CRC validation  
✅ **ECS Architecture** - Legion-based entity component system  
✅ **Cross-platform** - Windows, macOS, Linux support via WGPU

## Quick Start

### Prerequisites
- Rust toolchain (stable 2024 edition or later)
- Vulkan/WGPU-compatible graphics drivers

### Running the Game

```sh
cargo run
```

### Testing

```sh
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test --package moho_core
```

### Performance Benchmarks

```sh
# Run benchmarks (requires criterion)
cargo bench --bench event_bus_bench

# View results
start target/criterion/report/index.html
```

### Code Quality

```sh
# Format code
cargo fmt --all

# Run linter
cargo clippy --all-targets --all-features -- -D warnings
```

## Documentation

### Library Documentation
Each workspace crate has comprehensive documentation in its README:
- **[moho_core/README.md](moho_core/README.md)** - Event bus, materials, preferences
- **[moho_voxel/README.md](moho_voxel/README.md)** - Voxel grid, chunks, meshing
- **[moho_renderer/README.md](moho_renderer/README.md)** - WGPU rendering, CSM shadows, lighting
- **[moho_audio/README.md](moho_audio/README.md)** - Audio playback and event handling
- **[moho_ui/README.md](moho_ui/README.md)** - egui menus, settings, console, overlays
- **[moho_input/README.md](moho_input/README.md)** - Input mapping and key bindings

### Architecture Documentation
- **[Event Bus Best Practices](wiki/reference/event-bus-best-practices.md)** - Usage patterns and common pitfalls
- **[Event Bus Performance](wiki/reference/event-bus-performance.md)** - Benchmark results (11M events/sec)
- **[Console Architecture](wiki/architecture/console.md)** - Debug console system design
- **[Adding Console Commands](wiki/guides/adding-console-commands.md)** - Extending the console

### Technical Reference
- **[GPU ABI](wiki/reference/gpu-abi.md)** - Shader/CPU data layout requirements (Material, Camera, Lighting, CSM)
- **[Preferences Format](wiki/reference/prefs-format.md)** - Configuration file structure
- **[Documentation Index](wiki/README.md)** - Complete documentation overview

### Day/Night Cycle

The game features a complete day/night cycle with realistic celestial mechanics:

- **24-Hour Clock** - Configurable day/night lengths (default: 10 min day, 7 min night)
- **Dynamic Sky** - Time-based colors: dawn (warm orange), day (robin's egg blue), dusk (red/orange), night (dark blue)
- **Sun & Moon** - Realistic movement across the sky (sun rises east, peaks south, sets west; moon opposite)
- **Moonlight** - Moon provides blue-silver directional lighting at night (intensity 0.4)
- **Dynamic Lighting** - Ambient lighting adjusts with time (0.05-0.15 intensity)
- **Console Control** - Use `time <0-24>` to jump to any time (e.g., `time 0` = midnight, `time 12` = noon)

**Configuration** (per world):
```rust
let spec = moho_core::scene_builders::WorldSpec {
    name: "My World".to_string(),
    seed: Some(12345),
    size_xz: 64,
    day_length_seconds: 600.0,    // 10 minutes
    night_length_seconds: 420.0,   // 7 minutes
    initial_time_of_day: 6.0,      // Start at dawn
};
```

See [moho_core/README.md](moho_core/README.md) for GameClock API details.

### Debug Console (Developer)

- Toggle: Press the backtick (`) while in the Playing state to open the debug console overlay; press Escape to close it.
- Purpose: Lightweight in-game console for diagnostics, executing debug commands, and publishing events to the application's `EventBus`.
- Commands: `help`, `clear`, `quit`, `god`, `noclip`, `time <0-24>` (the console is extensible; commands publish `UiEvent`/`DebugEvent` variants).

Example: subscribe to console events via the `EventBus`:

```rust
use std::sync::Arc;
use moho_core::events::EventBus;

let bus: Arc<EventBus> = Arc::new(EventBus::new());
bus.subscribe(|evt: &moho_core::events::DebugEvent| {
    println!("DebugEvent: {:?}", evt);
});
```

See `wiki/architecture/console.md` and `wiki/guides/adding-console-commands.md` for architecture and extension guidance.

## Architecture

### Workspace Structure

```
moho/
├── benches/              # Performance benchmarks
├── wiki/                 # Project documentation
├── src/                  # Main application
├── tests/                # Integration tests
├── moho_core/            # Core types, events
├── moho_voxel/           # Voxel terrain (strategy line)
├── moho_renderer/        # WGPU rendering
├── moho_audio/           # Audio playback
├── moho_ui/              # egui UI
└── moho_input/           # Input handling
```

### Event Bus System

The event bus provides type-safe, decoupled communication between systems:

```rust
use moho_core::events::{EventBus, UiEvent};

let bus = Arc::new(EventBus::new());

// Subscribe to events
bus.subscribe(|event: &UiEvent| {
    println!("UI Event: {:?}", event);
});

// Publish events
bus.publish(UiEvent::MenuShown { name: "main".to_string() });
```

**Performance**: <1% frame budget at 60 FPS, 11.4M events/second throughput

See [EVENT_BUS_BEST_PRACTICES.md](wiki/reference/event-bus-best-practices.md) for detailed usage patterns.

## Build & Release

### Development Build
```sh
cargo build
```

### Release Build
```sh
cargo build --release
```

## CI
- `.github/workflows/ci.yml` runs the same `just` recipes as local work (`just check`, `just deny`, `just mutants`, `just cov`), with `Swatinem/rust-cache` caching dependencies. PRs into `dev` run the gate on Linux only; `main` and `gh workflow run CI --ref <branch>` run it on all three OSes.
- For detailed design notes and development guides, see the `wiki/` folder.
