# Moho Documentation Index

**Last Updated**: October 26, 2025  
**Status**: Event Bus Complete, Production Ready

---

## 📚 Quick Navigation

### ⭐ Start Here
- **[../README.md](../README.md)** - Main project README with quick start guide
- **[QUICK_REFERENCE.md](QUICK_REFERENCE.md)** - Common tasks and commands
- **[CLEANUP_NOTES.md](CLEANUP_NOTES.md)** - Low-risk improvements and findings

### 🎯 Event Bus (Latest - Production Ready)
- **[Best Practices](engine_core/EVENT_BUS_BEST_PRACTICES.md)** ⭐ Usage patterns, common pitfalls, migration guide
- **[Performance Analysis](engine_core/EVENT_BUS_PERFORMANCE.md)** - Benchmark results (11.4M events/sec, <1% frame budget)
- **[Testing Notes](engine_core/EVENT_BUS_TESTING_NOTES.md)** - Test findings, limitations, code examples
- **[Complete Summary](engine_core/EVENT_BUS_SUMMARY.md)** - Full implementation details and statistics

### 🏗️ Core Systems
- **[engine_core/CONCEPTS.md](engine_core/CONCEPTS.md)** - Core architecture and design
- **[engine_renderer/CONCEPTS.md](engine_renderer/CONCEPTS.md)** - Rendering system
- **[engine_audio/CONCEPTS.md](engine_audio/CONCEPTS.md)** - Audio system
- **[moho_ui/CONCEPTS.md](moho_ui/CONCEPTS.md)** - UI system integration

### 📖 Reference
- **[gpu_abi.md](gpu_abi.md)** - Shader/CPU data layout requirements
- **[prefs_format.md](prefs_format.md)** - Configuration file structure

---

## 🗂️ Architecture Overview

### Workspace Crates

| Crate | Purpose | Key Features |
|-------|---------|--------------|
| **moho_core** | Core types, events, voxels | Event bus, materials, camera, voxel system |
| **moho_renderer** | WGPU rendering | Custom mesh support, instance rendering |
| **moho_audio** | Audio playback | rodio-based, event-driven |
| **moho_ui** | egui integration | Menus, overlays, settings |
| **moho_input** | Input handling | Keyboard, mouse, gamepad mapping |
| **moho_sim** | Deterministic simulation | Snapshot/restore, CRC validation |

### Event System

The event bus is the primary communication mechanism between systems:

```rust
// 10 event type modules
SystemEvent  - Lifecycle, frames, errors
UiEvent      - Menus, buttons, forms
AudioEvent   - Sounds, music, voice
InputEvent   - Keyboard, mouse, gamepad
GameEvent    - Player actions, objectives
PhysicsEvent - Collisions, triggers
GraphicsEvent - Rendering, camera
WorldEvent   - Chunk loading, streaming
DebugEvent   - Logging, profiling
NetworkEvent - Multiplayer, sync
```

**Performance**: Sub-microsecond latency, 11.4M events/sec, <0.01% frame budget

See [EVENT_BUS_BEST_PRACTICES.md](engine_core/EVENT_BUS_BEST_PRACTICES.md) for usage patterns.

---

## 🧪 Testing

### Test Coverage
- **27 event bus tests** (12 unit + 8 integration + 7 doc tests) - 100% pass rate
- **Voxel system tests** - Face culling, grid operations
- **Simulation tests** - Determinism, snapshot roundtrip
- **Renderer tests** - Scene serialization, shader validation

### Running Tests

```sh
# All tests
cargo test --workspace

# Event bus tests only
cargo test --package moho_core events::tests
cargo test --test event_bus_integration

# Performance benchmarks
cargo bench --bench event_bus_bench
```

---

## � Current Status

### ✅ Completed Features

**Event Bus System** (October 2025)
- Type-safe pub/sub architecture
- Priority-based handler execution
- Metrics tracking and event history
- 27 comprehensive tests
- Performance exceeds commercial engines

**Voxel Terrain**
- Chunk-based world generation
- Face culling (87% triangle reduction)
- Material and resource systems
- Multi-octave Perlin noise

**ECS Integration**
- Legion-based entity system
- Deterministic simulation
- Snapshot/restore with CRC validation

**UI System**
- egui-based menus
- Settings persistence
- Event-driven interaction

**Audio System**
- rodio playback
- Event-driven triggering
- Background music support

### 🎯 Next Steps

**Debug Console Overlay** (Planned)
- Leverage event bus for command execution
- Use DebugEvent type for logging
- Implement command history
- Save command for debugging

**Multiplayer Foundation**
- Network event serialization
- Snapshot synchronization
- Input replay system

---

## 📝 Development Notes

### Performance Targets
- **60 FPS**: 16.66ms frame budget
- **Event bus**: <0.01% frame time
- **Voxel rendering**: TBD based on chunk count
- **Simulation**: Deterministic, snapshot-friendly

### Design Principles
1. **Separation of concerns**: Simulation vs rendering
2. **Type safety**: Compile-time event routing
3. **Performance**: Sub-millisecond latencies
4. **Testability**: Comprehensive test coverage
5. **Documentation**: Clear patterns and examples

### Known Limitations
1. **Event cascading**: Publishing from handler causes deadlock (use channels)
2. **AudioSystem threading**: Not Send/Sync (stays on main thread)
3. **History overhead**: 4.9× cost when enabled (disabled by default)

See [EVENT_BUS_TESTING_NOTES.md](engine_core/EVENT_BUS_TESTING_NOTES.md) for detailed findings.

---

## � Tools & Commands

### Code Quality
```sh
# Format
cargo fmt --all

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Test
cargo test --workspace

# Bench
cargo bench --bench event_bus_bench
```

### Documentation
```sh
# Generate API docs
cargo doc --open --no-deps

# View benchmark reports
start target/criterion/report/index.html
```

---

## 📂 Historical Documents

These documents are kept for reference but may be outdated:

- **[terrain_generation_plan.md](terrain_generation_plan.md)** - Original terrain plan (deprecated)
- **[terrain_generation_plan_revised.md](terrain_generation_plan_revised.md)** - Voxel-based system plan
- **[phase3_rendering_plan.md](phase3_rendering_plan.md)** - Phase 3 rendering (completed)
- **[CURRENT_STATE.md](CURRENT_STATE.md)** - Previous state snapshot
- **[CUSTOM_MESH_RENDERING_PLAN.md](CUSTOM_MESH_RENDERING_PLAN.md)** - Custom mesh plan

---

**Maintained By**: Moho Development Team  
**Last Major Update**: Event Bus Implementation (October 2025)  
**Branch**: `event-bus` → merge to `dev`

4. **Create New Branch**
   ```bash
   git checkout -b custom-mesh-rendering
   ```

5. **Explore Renderer**
   ```bash
   # Study these files first
   code moho_renderer/src/lib.rs
   code moho_renderer/src/scene.rs
   code moho_renderer/src/gpu_types.rs
   ```

6. **Start Phase 1 of Custom Mesh Plan**
   - Define `CustomMesh` trait
   - Implement for `VoxelChunk`
   - Create collection function
   - **Reference**: Lines 48-135 in `CUSTOM_MESH_RENDERING_PLAN.md`

---

## 🧭 Recent UI & Input Changes

- InputDispatcher: added a prioritized input dispatcher that routes `winit::WindowEvent` events to registered subscribers in descending priority order and stops propagation when an event is consumed. This lets the keybind capture UI preempt game input reliably.
- `moho_input` crate: extracted the physical-key -> binding-code mapping into a small shared crate `moho_input` so both the binary and `moho_ui` use a single canonical mapping. See `moho_input/src/lib.rs` for the mapping and unit tests.
- Wheel consumption policy: the UI now has precedence for mouse-wheel events. Wheel events are only forwarded to the game when the UI overlay is not visible. The forwarder is conservative: if the UI mutex cannot be acquired (poisoned/would-block), the event is NOT forwarded to avoid accidental input leakage while menus may be active.
- Tests: added unit tests for the dispatcher ordering/consumption and for the settings keybind UI. See `src/input_dispatcher.rs` and `moho_input/src/lib.rs` for tests and examples.

These changes improve input predictability and make keybind capture robust across UI and game layers.


## 📊 Key Metrics

### Current Implementation
- **Code Size**: ~644 lines in voxel.rs + ~140 lines in scene_builders.rs
- **Chunks Generated**: 16 non-empty chunks
- **Terrain Size**: 64×64 XZ plane (4,096 block positions)
- **Height Range**: 0-32 blocks
- **Face Culling**: ~87% triangle reduction
- **Build Status**: ✅ Clean compilation
- **Runtime Status**: ✅ No errors, ❌ No rendering

### After Custom Mesh Rendering (Expected)
- **Draw Calls**: 16 (one per chunk)
- **Triangles**: ~6,390 (with face culling) vs. ~49,152 (without)
- **GPU Memory**: ~125 KB total for all chunks
- **Performance Target**: >30 FPS with full terrain
- **Visual Quality**: Face-culled optimized mesh with proper lighting

---

## 🔧 Technical Reference

### Key Files Modified
- `moho_core/Cargo.toml` - Added noise dependency
- `moho_core/src/lib.rs` - Exported voxel module
- `moho_core/src/voxel.rs` - Complete voxel system (644 lines)
- `moho_core/src/scene_builders.rs` - Terrain generation
- `moho_core/src/actors.rs` - Instance collection
- `src/main.rs` - Camera position and scene selection

### Key Files to Modify Next
- `moho_core/src/actors.rs` - Add `CustomMesh` trait
- `moho_renderer/src/scene.rs` - Add custom mesh manager
- `moho_renderer/src/lib.rs` - Integrate custom rendering
- Main render loop - Call custom mesh collection

### Dependencies
- `noise = "0.9"` - Perlin noise
- `glam` - Math types
- `legion` - ECS
- `wgpu` - GPU backend
- `bytemuck` - GPU data marshalling

---

## 🎯 Goals by Document

| Document | Primary Goal | Audience |
|----------|-------------|----------|
| CURRENT_STATE.md | Understand what's done and what's next | Starting new session |
| CUSTOM_MESH_RENDERING_PLAN.md | Step-by-step implementation guide | Implementing renderer changes |
| terrain_generation_plan_revised.md | System architecture reference | Understanding design decisions |
| phase3_rendering_plan.md | Historical record of Phase 3 | Understanding what was attempted |

---

## 💡 Tips

1. **Always start with CURRENT_STATE.md** - It has the latest status
2. **CUSTOM_MESH_RENDERING_PLAN.md is your roadmap** - Follow it phase by phase
3. **Commit frequently** - This is experimental renderer work
4. **Test with single chunk first** - Simplify debugging
5. **Keep world-generation branch stable** - Use feature branch for renderer work

---

## 📝 Notes

- All voxel terrain code is **production ready** and well-tested
- The rendering blocker is **architectural, not a bug** in voxel code
- Face culling optimization is **already implemented** and will work once rendering works
- Camera position **already fixed** for terrain viewing
- Terrain generation **takes ~11 seconds** for smoothing pass (acceptable)

---

## 🔗 Related Files

### Source Code
- `moho_core/src/voxel.rs` - Core voxel system
- `moho_core/src/scene_builders.rs` - Terrain generation
- `moho_core/src/actors.rs` - Renderable trait
- `moho_renderer/src/` - Rendering system (to be modified)

### Shaders
- `shaders/vertex.wgsl` - Vertex shader (check compatibility)
- `shaders/fragment.wgsl` - Fragment shader
- `shaders/common.wgsl` - Shared structures

### Configuration
- Default terrain: GentleHills, amplitude 8.0, frequency 0.05
- Chunk size: 64×64×64 blocks
- Block coloring: Top faces = green, Side faces = light brown
- Materials: 0=grass, 1=dirt, 2=stone (used for physics/behavior)
- Resources: 0=stone, 1=iron ore (10% spawn rate)

---

**Ready to proceed? Start with `CURRENT_STATE.md` then move to `CUSTOM_MESH_RENDERING_PLAN.md`** 🚀

