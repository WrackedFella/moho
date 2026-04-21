# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build
cargo build --release

# Run
cargo run
RUST_LOG=debug cargo run                    # with logging
cargo run --features ui-egui-debug          # with egui debug panels

# Test
cargo test --workspace                      # all tests
cargo test --package moho_core              # single crate
cargo test --test event_bus_integration     # single integration test
cargo test some_fn_name                     # single test by name
cargo test -p moho_ui --features ui-egui-test  # UI integration tests

# Lint & format
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# Benchmarks
cargo bench --bench event_bus_bench
```

## Architecture

Moho is a Rust workspace split into 7 crates plus a main binary. The crates have a strict layering: the main binary depends on all crates; the UI and renderer depend on core; crates don't form circular dependencies.

```
src/main.rs          — winit event loop, ApplicationHandler, wires everything together
moho_core/           — EventBus, GameClock (day/night), voxel grid, materials, camera, input accumulation
moho_renderer/       — wgpu backend: mesh rendering, CSM shadows (4 cascades), skybox, scene serialization
moho_ui/             — egui integration: menus, settings, debug console, HUD overlays, preferences (config/prefs.ini)
moho_audio/          — rodio audio: music/SFX/UI/voice categories, event-driven triggering
moho_sim/            — deterministic headless simulation, snapshot/restore with CRC, replay foundations
moho_input/          — shared key→binding-code mapping (prevents duplication between binary and UI)
moho_types/          — shared AppState enum and StateCoordinator (avoids circular imports)
```

### Key Patterns

**Event Bus** (`moho_core::events`): The primary communication mechanism between systems. Type-safe pub/sub with 10 event categories (System, UI, Audio, Input, Game, Physics, Graphics, World, Debug, Network). Prefer publishing events over direct coupling.

**ECS**: Uses `legion` for entity/component management. World lives in the main binary; systems interact with it via the event bus or direct access passed from `main.rs`.

**Input flow**: `winit` → `InputDispatcher` (UI gets priority) → `InputRouter` → game actions → events. Physical keys map to `u32` binding codes via `moho_input`.

**UI architecture**: `ScreenSpec` trait for full-screen UIs; `Overlay` trait for in-game overlays (HUD, console). `UiStateManager` tracks which screen/overlays are active. Settings persist via `moho_ui::prefs::Prefs` to `config/prefs.ini`.

**Rendering pipeline**: Preparation → opaque mesh rendering → transparency. Cascaded Shadow Maps use 4 cascades at 4096×4096. Sun/moon lighting is driven by `GameClock` time.

**Simulation separation**: `moho_sim` uses integer-only math for determinism. It's kept headless (no wgpu/egui deps) to support future multiplayer lockstep.

### Code Conventions

- **Domain-Driven Design**: Name types and functions after domain concepts, not implementation details. Keep bounded contexts (rendering, input, audio, physics) with their own vocabularies.
- **Engine vs game-specific**: Prioritize quality and future-proofing for engine/foundation work. Pragmatic solutions are fine for game-specific mechanics.
- Minimize `unwrap()`/`expect()` — use `Result`/`?` propagation. No `panic!` in library crates.
- Avoid `unsafe` without documentation.
- All public types must implement `Debug`.
- Use `workspace.dependencies` in Cargo.toml for shared deps; don't duplicate version specs.
- Lines ≤ 100 characters (`rustfmt.toml` enforces this).
- Comments explain *why*, not *what*. Doc comments (`///`) only for public APIs. Never add comments purely for documentation without permission.
- Don't create new doc files without asking first.
