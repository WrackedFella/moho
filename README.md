[![CI](https://github.com/WrackedFella/moho/actions/workflows/ci.yml/badge.svg)](https://github.com/WrackedFella/moho/actions/workflows/ci.yml)

# Moho — A Modular Voxel Game Engine (Rust)

## Overview

Moho is a modular Rust workspace containing engine components designed for voxel-based games:

- **moho_core** - Event bus, voxel types, camera, materials, core utilities
- **moho_renderer** - WGPU-based rendering backend  
- **moho_audio** - Audio playback (rodio-based)
- **moho_ui** - egui integration for menus and overlays
- **moho_input** - Input mapping and state management
- **moho_sim** - Deterministic headless simulation for testing and multiplayer

The project emphasizes **separation of concerns** between simulation and rendering, making testing, snapshots, and future multiplayer functionality easier.

### Key Features

✅ **Event Bus System** - Type-safe pub/sub for decoupled system communication  
✅ **Voxel Terrain** - Chunk-based world with face culling (87% triangle reduction)  
✅ **Deterministic Simulation** - Snapshot/restore with CRC validation  
✅ **ECS Architecture** - Legion-based entity component system  
✅ **Cross-platform** - Windows, macOS, Linux support via WGPU

## Quick Start

### Prerequisites
- Rust toolchain (stable 2024 edition or later)
- Vulkan/WGPU-compatible graphics drivers

### Running the Game

```sh
cargo run --features "backend-wgpu,ui-egui"
```

### Testing

```sh
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test --package moho_core

# Run integration tests
cargo test --test event_bus_integration
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

### Event Bus
- **[Best Practices](docs/engine_core/EVENT_BUS_BEST_PRACTICES.md)** - Usage patterns and common pitfalls
- **[Performance Analysis](docs/engine_core/EVENT_BUS_PERFORMANCE.md)** - Benchmark results (11M events/sec)
- **[Testing Notes](docs/engine_core/EVENT_BUS_TESTING_NOTES.md)** - Test findings and limitations
- **[Complete Summary](docs/engine_core/EVENT_BUS_SUMMARY.md)** - Full implementation details

### Core Concepts
- **[moho_core Concepts](docs/engine_core/CONCEPTS.md)** - Architecture and design decisions
- **[moho_renderer Concepts](docs/engine_renderer/CONCEPTS.md)** - Rendering system overview
- **[moho_audio Concepts](docs/engine_audio/CONCEPTS.md)** - Audio system design
- **[moho_ui Concepts](docs/moho_ui/CONCEPTS.md)** - UI system integration

### Other Docs
- **[Quick Reference](docs/QUICK_REFERENCE.md)** - Common tasks and patterns
- **[GPU ABI](docs/gpu_abi.md)** - Shader/CPU data layout requirements
- **[Preferences Format](docs/prefs_format.md)** - Configuration file structure

## Architecture

### Workspace Structure

```
moho/
├── benches/              # Performance benchmarks
├── docs/                 # Documentation
├── src/                  # Main application
├── tests/                # Integration tests
├── moho_core/            # Core types, events, voxels
├── moho_renderer/        # WGPU rendering
├── moho_audio/           # Audio playback
├── moho_ui/              # egui UI
├── moho_input/           # Input handling
└── moho_sim/             # Deterministic simulation
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

See [EVENT_BUS_BEST_PRACTICES.md](docs/engine_core/EVENT_BUS_BEST_PRACTICES.md) for detailed usage patterns.

## Build & Release

### Development Build
```sh
cargo build --features "backend-wgpu,ui-egui"
```

### Release Build
```sh
cargo build --release --features "backend-wgpu,ui-egui"
```

## Docker / CI
- Build the CI Docker image (used by GitHub Actions; local debugging):

```sh
docker build -f .github/docker/Dockerfile -t moho-ci:latest .
```

- Run a container (example, Linux/macOS):

```sh
docker run --rm -it -v "$(pwd)":/workspace -w /workspace moho-ci:latest bash
```

On Windows (PowerShell), replace the volume mount syntax with an absolute path, for example:

```ps1
docker run --rm -it -v C:\path\to\repo:/workspace -w /workspace moho-ci:latest bash
```

Developer setup (short)
These steps help you run hermetic builds locally and reproduce CI behavior.

1) Local docker-compose (quick start)

- There is a helper compose file at `.github/docker/docker-compose.yml` for local experimentation. To build and run the CI image locally:

```sh
docker compose -f .github/docker/docker-compose.yml build
docker compose -f .github/docker/docker-compose.yml run --rm builder
```

Replace `docker compose` with `docker-compose` if your Docker installation uses the legacy CLI.

2) cargo-chef prepare/cook (hermetic Rust caching)

- Generate a recipe that captures dependency recipes (run once or when `Cargo.toml` changes):

```sh
cargo chef prepare --recipe-path recipe.json --recipe-cache target/recipe-cache
```

- Use the recipe to cook dependencies in a hermetic environment (this mirrors CI `cook` step):

```sh
cargo chef cook --recipe-path recipe.json --target-dir target/chef
```

3) Build and optionally publish the CI Docker image

- Build locally:

```sh
docker build -f .github/docker/Dockerfile -t moho-ci:latest .
```

- Publish to GitHub Container Registry (GHCR) — replace `OWNER`/`REPO` and authenticate first (see GHCR docs):

```sh
docker tag moho-ci:latest ghcr.io/OWNER/moho-ci:latest
docker push ghcr.io/OWNER/moho-ci:latest
```

Notes
- For faster local iteration you can combine `cargo chef prepare` + `docker build` so that CI image contains pre-cooked dependencies.
- If you use Windows, prefer absolute paths for docker volume mounts. Use WSL (Windows Subsystem for Linux) for a POSIX-like experience.

Notes & pointers
- For detailed design notes and development guides, see the `docs/` folder.
- The `moho_sim` crate contains the headless simulation, deterministic tests, and snapshot/restore utilities (bincode + CRC).
- CI caching and build details: `.github/CICACHE.md` and `.github/workflows/ci.yml`.
