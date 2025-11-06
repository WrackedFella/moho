# Phase 1: Critical Infrastructure Refactoring

**Milestone**: Clean Foundation  
**Goal**: Reduce complexity in core systems (App, SettingsMenu, Renderer)  
**Total Effort**: 34 SP (~3-4 sprints)  
**Status**: Planning

---

## Overview

Phase 1 targets the three most critical technical debt hotspots:
1. **App Initialization** (src/main.rs) - CC: 122 → Target: <20
2. **Settings Menu** (moho_ui/screens/settings/mod.rs) - CC: 126 → Target: <30
3. **Renderer Initialization** (moho_renderer/lib.rs) - CC: 80 → Target: <30

Each refactoring is broken into **small, testable increments** that can be completed independently.

---

## Track 1: App Initialization Refactoring

**Current State**: `App::new()` has CC of 122, initializes 15+ systems in one function  
**Target State**: Modular initialization with builder pattern, CC < 20  
**Total Effort**: 13 SP

### 1.1 Add Characterization Tests (2 SP) ✅ PREREQUISITE
**Goal**: Ensure we can detect regressions during refactoring

**Tasks**:
- [ ] Add test for App creation with default config
- [ ] Add test for App with all features enabled
- [ ] Add test for App with ui-egui feature disabled
- [ ] Document current initialization order and dependencies
- [ ] Verify all 57 existing tests still pass

**Acceptance Criteria**:
- 4 new characterization tests added
- Tests cover main initialization paths
- All tests pass on current code

**Files Changed**:
- `tests/app_initialization.rs` (new)

**Estimated Time**: 4-6 hours

---

### 1.2 Extract Event Bus Setup (1 SP)
**Goal**: Move event bus initialization to dedicated module

**Tasks**:
- [ ] Create `src/app/event_setup.rs`
- [ ] Extract event bus creation logic
- [ ] Extract event subscriber setup (UI, Audio, Graphics)
- [ ] Add module doc comments explaining subscriber patterns

**Acceptance Criteria**:
- Event bus setup in separate function: `setup_event_bus() -> EventBusSetup`
- Struct contains: `event_bus`, `ui_event_rx`, `audio_event_rx`, `graphics_event_rx`
- All tests still pass
- CC of App::new() reduced by ~5

**Files Changed**:
- `src/app/event_setup.rs` (new)
- `src/app/mod.rs` (new)
- `src/main.rs` (refactor App::new)

**Estimated Time**: 2-3 hours

---

### 1.3 Extract Audio System Initialization (1 SP)
**Goal**: Separate audio system setup into testable function

**Tasks**:
- [ ] Create `src/app/audio_init.rs`
- [ ] Extract `initialize_audio_system()` function
- [ ] Add error handling with proper logging
- [ ] Add unit test for audio init success/failure paths

**Acceptance Criteria**:
- Audio init returns `Result<Option<AudioSystem>, AudioInitError>`
- Proper error logging without panicking
- Test covers both success and failure cases
- CC of App::new() reduced by ~3

**Files Changed**:
- `src/app/audio_init.rs` (new)
- `src/main.rs` (refactor App::new)

**Estimated Time**: 2-3 hours

---

### 1.4 Extract Camera Setup (1 SP)
**Goal**: Move camera initialization to dedicated function

**Tasks**:
- [ ] Create `src/app/camera.rs`
- [ ] Extract default camera setup logic
- [ ] Add configurable camera parameters (eye, center, fov, aspect)
- [ ] Add builder pattern for camera configuration

**Acceptance Criteria**:
- `CameraBuilder::default().build()` creates standard camera
- Supports custom eye position, look-at target, FOV
- CC of App::new() reduced by ~2

**Files Changed**:
- `src/app/camera.rs` (new)
- `src/main.rs` (refactor App::new)

**Estimated Time**: 2 hours

---

### 1.5 Create AppConfig Struct (2 SP)
**Goal**: Centralize configuration that affects initialization

**Tasks**:
- [ ] Create `src/app/config.rs`
- [ ] Define `AppConfig` struct with all initialization parameters
- [ ] Extract Prefs loading logic
- [ ] Add `AppConfig::from_prefs()` method
- [ ] Add feature flag handling (#[cfg(feature = "ui-egui")])

**Acceptance Criteria**:
- Single source of truth for app configuration
- Config loaded once, passed to initializers
- CC of App::new() reduced by ~8

**Files Changed**:
- `src/app/config.rs` (new)
- `src/main.rs` (refactor App::new)

**Estimated Time**: 3-4 hours

---

### 1.6 Create AppInitializer Builder (3 SP)
**Goal**: Replace App::new() with staged builder pattern

**Tasks**:
- [ ] Create `src/app/initializer.rs`
- [ ] Implement `AppInitializer` struct with builder methods
- [ ] Add `with_event_bus()`, `with_audio()`, `with_camera()` methods
- [ ] Add `build() -> Result<App, AppInitError>` finalizer
- [ ] Refactor App::new() to use AppInitializer
- [ ] Add comprehensive tests for builder pattern

**Acceptance Criteria**:
- App creation uses: `AppInitializer::new(config).build()?`
- Each subsystem initialized by dedicated builder method
- Original App::new() delegates to initializer
- All 57+ tests still pass
- CC of App::new() now < 20

**Files Changed**:
- `src/app/initializer.rs` (new)
- `src/main.rs` (major refactor)

**Estimated Time**: 6-8 hours

---

### 1.7 Extract Run Loop Logic (3 SP)
**Goal**: Separate event loop handling from App struct

**Tasks**:
- [ ] Create `src/app/event_loop.rs`
- [ ] Extract window event handling to `EventLoopHandler`
- [ ] Split game loop into smaller functions:
  - `process_window_events()`
  - `process_game_events()`
  - `update_systems()`
  - `render_frame()`
- [ ] Add state machine for game state transitions

**Acceptance Criteria**:
- App::run() delegates to EventLoopHandler
- Each phase of the loop in separate function
- CC of App::run() reduced from 94 to < 30
- Game loop logic testable in isolation

**Files Changed**:
- `src/app/event_loop.rs` (new)
- `src/main.rs` (refactor App::run)

**Estimated Time**: 6-8 hours

---

**Track 1 Completion Checklist**:
- [ ] All 7 increments completed
- [ ] App::new() CC < 20
- [ ] App::run() CC < 30
- [ ] All tests passing (57 baseline + new tests)
- [ ] Code review completed
- [ ] Metrics re-run to validate improvements

---

## Track 2: Settings Menu Refactoring

**Current State**: SettingsMenu has CC of 126, 925 lines in one file  
**Target State**: Modular settings with data-driven binding registry, CC < 30  
**Total Effort**: 8 SP

### 2.1 Add Settings Menu Tests (1 SP) ✅ COMPLETED
**Goal**: Capture current behavior before refactoring

**Tasks**:
- [x] Fixed test imports to use public API
- [x] Added test helper methods to SettingsMenu
- [x] Made apply_key_code_while_listening public for testing
- [x] Fixed test isolation issue (using unbound key 'Q')
- [x] Verified all 10 tests pass

**Acceptance Criteria**:
- ✅ 10 tests covering core settings menu behavior
- ✅ Tests use `apply_key_code_while_listening()` testable API
- ✅ All tests pass on current code

**Files Changed**:
- `moho_ui/tests/settings_menu.rs` (fixed imports, updated one test)
- `moho_ui/src/screens/settings/mod.rs` (added test helper methods)

**Completed**: November 6, 2025

---

### 2.2 Define BindingId Enum (1 SP) ✅ COMPLETED
**Goal**: Replace magic numbers (0,1,2,3,4,5) with named enum

**Tasks**:
- [x] Added BindingId enum to types.rs with 6 variants
- [x] Implemented to_usize() and from_usize() conversions
- [x] Added display_name() method for UI strings
- [x] Added all() method for iteration
- [x] Exported BindingId from settings module
- [x] Refactored get_key_name() to use BindingId

**Acceptance Criteria**:
- ✅ No magic numbers in get_key_name()
- ✅ Type-safe binding identification
- ✅ All tests still pass

**Files Changed**:
- `moho_ui/src/screens/settings/types.rs` (added BindingId enum)
- `moho_ui/src/screens/settings/mod.rs` (exported BindingId, updated get_key_name)

**Completed**: November 6, 2025

---

### 2.3 Create BindingRegistry (2 SP) ✅ COMPLETED
**Goal**: Data-driven binding conflict detection

**Tasks**:
- [x] Created `binding_registry.rs` module with BindingRegistry struct
- [x] Implemented `from_prefs()` and `write_to_prefs()` for conversion
- [x] Implemented `find_conflict()` replacing 145+ line if-else chain
- [x] Implemented `update_binding()`, `get_binding()`, and `iter()` methods
- [x] Added 8 comprehensive unit tests for registry operations
- [x] Refactored `apply_key_code_while_listening()` to use registry
- [x] Refactored `capture_modifier_if_listening()` to use registry

**Acceptance Criteria**:
- ✅ Conflict detection in < 10 lines (was 145 lines) - reduced by ~93%!
- ✅ Registry tested in isolation (8 unit tests)
- ✅ SettingsMenu uses registry instead of manual checks
- ✅ CC of conflict detection logic < 5
- ✅ All tests still pass (10 settings tests + 8 registry tests)

**Files Changed**:
- `moho_ui/src/screens/settings/binding_registry.rs` (new - 243 lines)
- `moho_ui/src/screens/settings/mod.rs` (refactored conflict detection)

**Completed**: November 6, 2025

---

### 2.4 Extract State Management (1 SP)
**Goal**: Separate dirty tracking and staged changes

**Tasks**:
- [ ] Create `moho_ui/src/screens/settings/state.rs`
- [ ] Define `SettingsState` struct with `prefs`, `staged`, `dirty_fields`
- [ ] Add methods: `mark_dirty()`, `is_dirty()`, `apply_changes()`, `revert_changes()`
- [ ] Move state logic out of SettingsMenu

**Acceptance Criteria**:
- State management isolated and testable
- Clear API for state mutations
- SettingsMenu delegates to SettingsState
- Tests for state transitions

**Files Changed**:
- `moho_ui/src/screens/settings/state.rs` (new)
- `moho_ui/src/screens/settings/mod.rs` (refactor)

**Estimated Time**: 2-3 hours

---

### 2.5 Split Tab Rendering (2 SP)
**Goal**: Move tab-specific UI logic to separate modules

**Tasks**:
- [ ] Ensure `controls_tab.rs` and `audio_tab.rs` are fully self-contained
- [ ] Extract any remaining controls logic from mod.rs to controls_tab.rs
- [ ] Extract any remaining audio logic from mod.rs to audio_tab.rs
- [ ] Add `render()` method to each tab that takes context
- [ ] Create `TabRenderer` trait for consistency

**Acceptance Criteria**:
- Each tab renders itself independently
- SettingsMenu::render() just delegates to active tab
- Tab modules can be tested in isolation
- CC of SettingsMenu::render() < 15

**Files Changed**:
- `moho_ui/src/screens/settings/controls_tab.rs` (refactor)
- `moho_ui/src/screens/settings/audio_tab.rs` (refactor)
- `moho_ui/src/screens/settings/mod.rs` (refactor)

**Estimated Time**: 4-5 hours

---

### 2.6 Extract Modal Logic (1 SP)
**Goal**: Separate conflict modal into dedicated module

**Tasks**:
- [ ] Create `moho_ui/src/screens/settings/conflict_modal.rs`
- [ ] Define `ConflictModal` struct with state
- [ ] Add methods: `show()`, `is_visible()`, `confirm()`, `cancel()`
- [ ] Move modal rendering logic from SettingsMenu
- [ ] Add tests for modal state machine

**Acceptance Criteria**:
- Modal logic isolated and testable
- SettingsMenu just calls `modal.render(ctx)`
- Clear API for modal lifecycle
- CC of modal logic < 10

**Files Changed**:
- `moho_ui/src/screens/settings/conflict_modal.rs` (new)
- `moho_ui/src/screens/settings/mod.rs` (refactor)

**Estimated Time**: 2-3 hours

---

**Track 2 Completion Checklist**:
- [ ] All 6 increments completed
- [ ] SettingsMenu main impl CC < 30
- [ ] All settings tests passing
- [ ] Binding conflict detection is data-driven
- [ ] Code review completed
- [ ] Metrics re-run to validate improvements

---

## Track 3: Renderer Initialization Refactoring

**Current State**: Renderer::new() has CC of 80, 1,802 lines in lib.rs  
**Target State**: Modular initialization with builder pattern, CC < 30  
**Total Effort**: 13 SP

### 3.1 Add Renderer Tests (2 SP) ✅ PREREQUISITE
**Goal**: Ensure renderer behavior is preserved

**Tasks**:
- [ ] Add test for successful renderer creation (mock window)
- [ ] Add test for device initialization failure handling
- [ ] Add test for pipeline creation with standard config
- [ ] Document current initialization order
- [ ] Verify existing render tests still pass

**Acceptance Criteria**:
- 3 new renderer initialization tests
- Tests use mock window/surface when possible
- All tests pass on current code

**Files Changed**:
- `moho_renderer/tests/renderer_init.rs` (new)

**Estimated Time**: 4-5 hours

---

### 3.2 Extract Device Initialization (2 SP)
**Goal**: Separate wgpu device and surface setup

**Tasks**:
- [ ] Create `moho_renderer/src/device.rs`
- [ ] Define `DeviceSetup` struct with instance, device, queue, surface, config
- [ ] Extract device initialization to `DeviceBuilder::new(window).build()`
- [ ] Add proper error handling with `DeviceInitError`
- [ ] Add unit tests for device setup

**Acceptance Criteria**:
- Device init logic in separate module
- Returns strongly-typed result
- CC of device init < 20
- Renderer::new() delegates to DeviceBuilder

**Files Changed**:
- `moho_renderer/src/device.rs` (new)
- `moho_renderer/src/lib.rs` (refactor)

**Estimated Time**: 4-5 hours

---

### 3.3 Extract Pipeline Creation (3 SP)
**Goal**: Separate shader compilation and pipeline setup

**Tasks**:
- [ ] Create `moho_renderer/src/pipeline.rs`
- [ ] Define `PipelineSetup` struct with all pipelines
- [ ] Extract shader loading and compilation
- [ ] Extract render pipeline creation
- [ ] Extract depth texture and bind groups
- [ ] Add comprehensive error handling

**Acceptance Criteria**:
- Pipeline creation in separate module
- Shader errors clearly reported
- CC of pipeline setup < 25
- Can test pipeline creation independently

**Files Changed**:
- `moho_renderer/src/pipeline.rs` (new)
- `moho_renderer/src/lib.rs` (refactor)

**Estimated Time**: 6-8 hours

---

### 3.4 Extract Resource Allocation (2 SP)
**Goal**: Separate buffer and texture management

**Tasks**:
- [ ] Create `moho_renderer/src/resources.rs`
- [ ] Define `ResourcePool` struct for buffers and textures
- [ ] Extract vertex buffer creation
- [ ] Extract instance buffer creation
- [ ] Extract texture and sampler creation
- [ ] Add methods for dynamic resource allocation

**Acceptance Criteria**:
- Resource management isolated
- Clear ownership and lifetime management
- CC of resource allocation < 15
- Can allocate resources on-demand

**Files Changed**:
- `moho_renderer/src/resources.rs` (new)
- `moho_renderer/src/lib.rs` (refactor)

**Estimated Time**: 4-5 hours

---

### 3.5 Create RendererBuilder (2 SP)
**Goal**: Compose initialization with builder pattern

**Tasks**:
- [ ] Create `moho_renderer/src/builder.rs`
- [ ] Implement `RendererBuilder` with staged initialization:
  - `new(window) -> Self`
  - `init_device(self) -> DeviceBuilder`
  - `create_pipelines(self) -> PipelineBuilder`
  - `allocate_resources(self) -> Result<Renderer>`
- [ ] Refactor Renderer::new() to use builder
- [ ] Add integration tests for full builder chain

**Acceptance Criteria**:
- Builder pattern fully implemented
- Each stage returns typed builder
- Renderer::new() CC < 20
- All renderer tests still pass

**Files Changed**:
- `moho_renderer/src/builder.rs` (new)
- `moho_renderer/src/lib.rs` (major refactor)

**Estimated Time**: 4-5 hours

---

### 3.6 Refactor Render Method (2 SP)
**Goal**: Split rendering into smaller functions

**Tasks**:
- [ ] Extract `begin_render_pass()` method
- [ ] Extract `draw_instances()` method
- [ ] Extract `draw_skybox()` method
- [ ] Extract `finish_render_pass()` method
- [ ] Simplify main `render()` method to orchestrate

**Acceptance Criteria**:
- Render method is < 20 lines
- Each render phase in separate function
- CC of render method < 10
- Rendering still works correctly

**Files Changed**:
- `moho_renderer/src/lib.rs` (refactor render method)

**Estimated Time**: 3-4 hours

---

**Track 3 Completion Checklist**:
- [ ] All 6 increments completed
- [ ] Renderer::new() CC < 30
- [ ] Render method CC < 10
- [ ] All renderer tests passing
- [ ] Code review completed
- [ ] Metrics re-run to validate improvements

---

## Execution Strategy

### Approach: Parallel Tracks with Dependencies
- **Tracks can run in parallel** - they touch different files
- **Within each track**, follow increments sequentially
- **Each increment** should be a separate commit
- **Each increment** should pass all tests before moving to next

### Recommended Order
1. **Start with all prerequisite tests** (1.1, 2.1, 3.1) - can run in parallel
2. **Pick one track to complete first** (recommendation: Track 2 - smallest effort)
3. **Alternate between tracks** to maintain context and avoid burnout
4. **Complete each increment fully** before starting next

### Daily Cadence (Example)
**Sprint 1 (Week 1)**:
- Day 1: Complete prerequisites (1.1, 2.1, 3.1)
- Day 2: Track 2.2 + 2.3 (BindingId + Registry)
- Day 3: Track 2.4 + 2.5 (State + Tabs)
- Day 4: Track 2.6 (Modal) - **Track 2 Complete! 🎉**
- Day 5: Track 1.2 + 1.3 (Event Bus + Audio Init)

**Sprint 2 (Week 2)**:
- Day 1: Track 1.4 + 1.5 (Camera + Config)
- Day 2: Track 1.6 (AppInitializer Builder)
- Day 3: Track 1.7 (Event Loop Extraction)
- Day 4: **Track 1 Complete! 🎉** + Start Track 3.2
- Day 5: Track 3.2 completion + Track 3.3 start

**Sprint 3 (Week 3)**:
- Day 1-2: Track 3.3 (Pipeline Creation)
- Day 3: Track 3.4 (Resource Allocation)
- Day 4: Track 3.5 (RendererBuilder)
- Day 5: Track 3.6 (Render Method) - **Track 3 Complete! 🎉**

### After Each Increment
1. ✅ Run tests: `cargo test --workspace --all-features`
2. ✅ Run clippy: `cargo clippy --all-targets --all-features -- -D warnings`
3. ✅ Commit with descriptive message
4. ✅ Update this file to mark increment complete

### After Each Track
1. ✅ Run full test suite
2. ✅ Re-run metrics: `.\scripts\run-metrics.ps1 -Clean`
3. ✅ Update CODE_METRICS_ANALYSIS.md with progress
4. ✅ Git push and verify CI passes
5. ✅ Code review (if working with team)

---

## Success Metrics

### Quantitative Goals
- [ ] App::new() CC: 122 → < 20 (**84% reduction**)
- [ ] App::run() CC: 94 → < 30 (**68% reduction**)
- [ ] SettingsMenu CC: 126 → < 30 (**76% reduction**)
- [ ] Renderer::new() CC: 80 → < 30 (**62% reduction**)
- [ ] All 57+ tests passing
- [ ] No new clippy warnings

### Qualitative Goals
- [ ] Code is easier to understand (clear module boundaries)
- [ ] New developers can navigate initialization logic
- [ ] Subsystems can be tested in isolation
- [ ] Adding new features requires less code churn

---

## Risk Mitigation

### Risk: Breaking Existing Functionality
**Mitigation**: 
- Comprehensive tests before refactoring
- Small increments with frequent test runs
- Feature flags for large changes

### Risk: Scope Creep
**Mitigation**:
- Strict adherence to increment boundaries
- Defer nice-to-haves to Phase 2 or 3
- Track out-of-scope improvements in separate backlog

### Risk: Unforeseen Dependencies
**Mitigation**:
- Start with smallest track (Track 2) to gain experience
- Document surprises in this file
- Adjust estimates after each track completion

### Risk: Context Switching Overhead
**Mitigation**:
- Complete one increment fully before starting next
- Use descriptive commit messages for future context
- Take notes on design decisions in module docs

---

## Backlog (Out of Scope for Phase 1)

These improvements are valuable but deferred to Phase 2 or 3:
- [ ] Extract world generation logic from App
- [ ] Improve error messages in renderer
- [ ] Add configuration file for graphics settings
- [ ] Optimize buffer allocation strategies
- [ ] Add telemetry for initialization timing
- [ ] Create initialization progress UI

---

## Progress Tracking

**Track 1 (App)**: ⬜⬜⬜⬜⬜⬜⬜ (0/7 complete)  
**Track 2 (Settings)**: ✅✅✅⬜⬜⬜ (3/6 complete - 2.1, 2.2, 2.3 done)  
**Track 3 (Renderer)**: ⬜⬜⬜⬜⬜⬜ (0/6 complete)  

**Overall Phase 1**: 16% complete (3/19 increments)

**Significant Achievement**: Reduced conflict detection from 145 lines of manual if-else chains to < 10 lines using data-driven registry! 🎉

---

**Last Updated**: November 6, 2025 - Completed 2.3 (BindingRegistry)  
**Next Task**: 2.4 Extract State Management  
**Next Review**: After first track completion
