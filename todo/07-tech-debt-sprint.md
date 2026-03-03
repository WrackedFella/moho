# Tech Debt Sprint — Foundation Hygiene

**Milestone:** Foundation Hygiene (~13 SP)
**Status:** COMPLETE
**Goal:** Shore up systemic tech debt before resuming Phase 2 roadmap work.
**Rationale:** Phase 2 tasks (RTS camera, HUD debug, fix spawn/lights) touch the modules with the most debt. Fixing foundations first makes that work materially easier.

## Stories

### 1. Consolidate Workspace Dependencies (1 SP) ✅
- [x] Move `legion`, `noise`, `thiserror`, `serde_json`, `crc32fast`, `crossbeam-channel` to `workspace.dependencies`
- [x] Remove unused deps (`winit` from moho_types, `rand`/`rand_chacha` from moho_sim, `glam` from moho_audio)
- [x] Ensure all crates use `workspace = true` for shared deps

### 2. Workspace-wide `Debug` Sweep (1 SP) ✅
- [x] Add `Debug` to 30+ public types missing it across all crates
- [x] Add `PartialEq` where appropriate (`AppState`)

### 3. Remove Dead/Unsafe Code (2 SP) ✅
- [x] Delete unsafe `Send`/`Sync` on `EventBus`
- [x] Remove `extern crate` statements (3 files)
- [x] Delete commented-out shadow code in moho_renderer (~170 lines)
- [x] Remove dead `RefractRecord` from moho_core
- [x] Clean up `input_routing.rs` — removed blanket `#[allow(dead_code)]`, added targeted allows
- [x] Fix `process_deferred()` — now dispatches events via captured closure
- [x] Remove 5 duplicate glam wrappers, migrate all callers to native methods
- [x] Clean up stale `#[allow(dead_code)]` annotations
- [x] Consolidate duplicate accessor methods in `GameState`

### 4. Eliminate Duplicate Types (2 SP) ✅
- [x] Unify `GameState` — moho_ui now re-exports `moho_types::GameState`
- [x] Unify `AudioCategory` — single enum in `audio_source`, `Stop(Option<AudioCategory>)`
- [x] Rename `lights::LightType` → `LightShape` to avoid collision with `shadow::LightType`
- [x] Rename `events::MaterialType` → `BlockMaterial` to avoid collision with `materials::MaterialType`
- [x] Remove duplicate `FaceDirection` in blocky.rs — now imports from `face.rs`
- [x] Remove duplicate `MaterialGpu` in lib.rs — now re-exports from `gpu_types`

### 5. Fix Library Panics (1 SP) ✅
- [x] Replace `expect()` in moho_sim `snapshot()`/`restore()`/`snapshot_bytes()` with `Result` returns
- [x] Fix `AudioCache::Default` — now creates empty cache without panicking

### 6. Fix Data-Loss Bugs (1 SP) ✅
- [x] Include `GameClock` in `SimulationSnapshot`
- [x] Fix `MusicVolumeChanged` → no longer stops music (`map_audio_event` returns `Option`)

### 7. Extract Magic Numbers (1 SP) ✅
- [x] Named constants in `controller.rs` (7 values: speed, pitch, FOV, aspect, near/far, offset)
- [x] Named constants in `shadow.rs` (14 values: PCSS sizes, bias, cascade multiplier, intensity threshold)
- [x] Named constants in `event_processor.rs` (7 values: raycast, spawn, torch ID, light defaults)

### 8. Encapsulate Key Structs (3 SP) — Partial ✅
- [x] `AudioSettings` — fields now private (builder pattern + `effective_volume()` unchanged)
- [x] `AudioSource` — fields now private, added getters, updated `audio_system.rs` callers
- [ ] `VoxelChunk`, `DeviceSetup`, `ResourcePool`, `Prefs`, `AppState` — deferred (high caller count, needs compile verification)

### 9. Add Crate-Level Docs (1 SP) ✅
- [x] `//!` documentation in `moho_core/src/lib.rs`
- [x] `//!` documentation in `moho_audio/src/lib.rs`
- [x] `//!` documentation in `moho_renderer/src/lib.rs`
- [x] `//!` documentation in `src/main.rs`

## Residual Items (future work)

- Encapsulate remaining structs (`VoxelChunk`, `DeviceSetup`, `ResourcePool`, `Prefs`) — needs compile-loop validation
- Centralize key codes from moho_input + moho_ui
- Integrate `InputRouter::dispatch()` into the input pipeline (replaces `InputDispatcher`)
- Implement runtime music volume adjustment (`MusicVolumeChanged` event)
- Track per-type event metrics in `EventBus::metrics()`
