# Phase 4 Refactoring Plan — Data-Driven Architecture & Final Cleanup

**Created**: November 12, 2025  
**Status**: IN PROGRESS  
**Branch**: time-of-day  
**Prerequisites**: Phase 3 + Track 11 100% complete (36 SP, all tests passing)

**Last Updated**: November 12, 2025 - Track 12.1 Complete

---

## Executive Summary

Phase 4 focuses on **addressing remaining critical technical debt** through:
1. **Eliminating accidental complexity** (god objects, tangled logic, mixed concerns)
2. **Documenting inherent complexity** (legitimate enum mappings, domain-specific state)
3. **Strategic cleanup** (scene management, event loop, render coordination)

**Key Principle**: Not all complexity is technical debt. Some complexity is inherent to the problem domain (e.g., mapping 40+ keyboard keys). Phase 4 distinguishes between:
- **Accidental Complexity**: Tangled logic, god objects, poor separation → **REFACTOR**
- **Inherent Complexity**: Domain mappings, exhaustive enums → **ACCEPT & DOCUMENT**

**Estimated Total**: 19 story points across 7 tracks  
**Target Completion**: 2-3 sprints  
**Risk Level**: Medium (involves core rendering and input systems)

---

## Current State Assessment (Post-Track 11)

### Critical Issues Remaining 🔴

| File | MI | CC | Lines | Issue Type |
|------|----|----|-------|------------|
| **main.rs** | 0 | 110 | ~900 | **Accidental** - God object, event loop monolith |
| **keybind_capture.rs** | 0 | 90 | 672 | **Mixed** - Improved (-50% CC), orchestration complexity |
| **scene.rs** | 3 | 51 | ~400 | **Accidental** - Mixed concerns, render coordination |
| **audio_system.rs** | 10 | 57 | ~350 | **Mixed** - State machine could be cleaner |
| **key_mapping.rs** | 10 | 47 | 225 | **Inherent** - 40+ key enum mapping (legitimate) |
| **lib.rs (renderer)** | 0 | 55 | ~500 | **Accidental** - render_mesh() too long |

### Inherent Complexity (Accept & Document) ✅

| Function | CC | Rationale |
|----------|----|-----------| 
| **key_to_code()** | 47 | Exhaustive mapping of 40+ egui::Key variants (external enum) |
| **physical_key_to_binding_code()** | 41 | Exhaustive mapping of 40+ PhysicalKey variants |
| **binding_to_string()** | 27 | Serialization of binding combinations (modifiers × keys) |

These represent **domain complexity** (keyboards have many keys), not technical debt.
Compiled to jump tables by Rust compiler (equivalent to O(1) array lookup).

### High-Value Targets 🟡

| File | MI | CC | Opportunity |
|------|----|----|-------------|
| **audio_system.rs** | 10 | 57 | Audio state machine could be cleaner |
| **physical_key_to_binding_code** | N/A | 41 | moho_input giant match (external crate impact) |
| **actors.rs** | 8 | N/A | Actor management could be extracted |

### Health Metrics

- **Average MI**: 28 (target: 32-35)
- **Files with MI < 20**: 34 files (target: <20)
- **Functions with CC > 40**: 6 functions (target: 2-3 after distinguishing inherent complexity)
- **Total test coverage**: 336 tests passing ✅

**Metric Interpretation Note**: CC > 40 is acceptable when it represents **inherent complexity** (exhaustive enum matching, domain state machines). Focus is on eliminating **accidental complexity** (tangled logic, god objects).

---

## Phase 4 Track Breakdown

### **Track 12: Data-Driven Input System** (6 SP) 🔴 HIGH PRIORITY

**Objective**: Optimize input system with PHF lookups where applicable, document inherent complexity

#### Track 12.1: Key Mapping Lookup Tables (3 SP) ✅ **COMPLETE**

**Status**: ✅ Complete (November 12, 2025)

**Problem**: 
- `key_mapping.rs::key_to_code()` CC: 47 (40+ match arms)
- `key_mapping.rs::binding_label()` CC: 28 (30+ match arms)
- Both are pure lookups, not logic

**Solution Implemented**: Used `phf` (compile-time perfect hash maps) for reverse lookups

```rust
// Added phf dependency (updated to latest stable)
phf = { version = "0.13.1", features = ["macros"] }

// Created CODE_TO_LABEL_MAP for binding_label() optimization
static CODE_TO_LABEL_MAP: phf::Map<u32, &'static str> = phf::phf_map! {
    0x100u32 => "ArrowUp",
    0x101u32 => "ArrowDown",
    // ... all special keys
};

// binding_label() now uses lookup instead of match
if let Some(&label) = CODE_TO_LABEL_MAP.get(&b.code) {
    s.push_str(label);
    return s;
}
```

**Key Learning**: 
- ✅ **PHF works excellently for code→string lookups** (binding_label)
- ⚠️ **PHF cannot optimize enum→code mappings** without enum internals (key_to_code)
- The `key_to_code()` match statement represents **inherent complexity** - it's a legitimate 40-variant mapping that cannot be simplified without controlling egui::Key's representation
- This is a **data structure problem**, not a code problem - the complexity comes from the external enum's design

**Actual Results**:
- ✅ `binding_label()` optimized: PHF lookup replaces 10+ match arms
- ✅ `key_to_code()` documented as **inherent complexity** (40+ enum variants)
- ✅ `key_mapping.rs` better organized with clear separation of concerns
- ✅ All 336+ tests passing
- ✅ Zero regressions

**Metrics Impact**:
- `binding_label()`: Now uses O(1) PHF lookup for special keys
- `key_to_code()`: CC: 47 accepted as inherent complexity (exhaustive enum mapping)
- Code is cleaner, better documented, and properly organized

**Conclusion**: Track 12.1 successfully demonstrated PHF for static data lookups and identified that enum→primitive mappings represent legitimate domain complexity, not technical debt.

---

#### Track 12.2: Binding Serialization Optimization (3 SP) 🔄 **ADJUSTED SCOPE**

**Status**: Not Started

**Problem**:
- `prefs/parser.rs::binding_to_string()` CC: 27
- Serializes binding combinations (keys + modifiers)

**Revised Approach** (based on Track 12.1 learnings):
- Focus on **reverse lookup optimization** (string→code parsing)
- Document **forward serialization** as inherent complexity
- Use PHF for string→code mappings where beneficial

**Solution**: Optimize string parsing with PHF lookup tables

```rust
// moho_ui/src/prefs/key_names.rs (new module)
use phf::phf_map;

pub static KEY_NAME_TO_CODE: phf::Map<&'static str, u32> = phf_map! {
    "A" => 'A' as u32,
    "B" => 'B' as u32,
    "ArrowUp" => 0x100,
    "ArrowDown" => 0x101,
    // ... all key names for parsing
};

// Optimize parse_binding() using PHF lookup
pub fn parse_key_name(name: &str) -> Option<u32> {
    KEY_NAME_TO_CODE.get(name).copied()
}
```

**Expected Results** (revised):
- ✅ `parse_binding()` optimization: string lookups via PHF
- ⚠️ `binding_to_string()` CC: 27 → 20-25 (limited improvement, inherent complexity)
- ✅ Faster preference file parsing
- ✅ Better code organization

**Risk**: Low (parsing optimization, serialization already well-tested)

---

### **Track 13: Scene Management Extraction** (5 SP) 🔴 HIGH PRIORITY

**Objective**: Extract scene preparation and render coordination to eliminate accidental complexity

**Current Issue**: `scene.rs` has **accidental complexity**—mixed concerns (preparation + coordination + state management) in one 400-line file.

#### Track 13.1: Scene Preparation Extraction (3 SP)

**Problem**:
- `scene.rs::Scene` CC: 51, MI: 3
- Combines instance collection, shadow calculation, buffer management in one impl

**Solution**: Extract ScenePreparation struct

```rust
// moho_renderer/src/scene/preparation.rs (new module)
pub struct ScenePreparation {
    instances: Vec<GpuInstance>,
    shadow_matrices: ShadowMatrices,
    camera_state: CameraState,
    dirty_chunks: Vec<ChunkId>,
}

impl ScenePreparation {
    pub fn prepare(scene: &Scene, camera: &Camera, sun: &SunLight) -> Self {
        // Extract ~200 lines of preparation logic here
    }
}

// scene.rs becomes thinner orchestrator
impl Scene {
    pub fn prepare(&self, camera: &Camera, sun: &SunLight) -> ScenePreparation {
        ScenePreparation::prepare(self, camera, sun)
    }
}
```

**Expected Results**:
- `scene.rs` lines: 400 → 250
- `scene.rs` CC: 51 → 25-30
- `scene.rs` MI: 3 → 15-20
- New `scene/preparation.rs` MI: 30+

**Risk**: Medium (core rendering path, requires careful extraction)

---

#### Track 13.2: Render Coordination Extraction (2 SP)

**Problem**:
- Scene coordinates 4 render passes (shadow, CSM, main, skybox)
- Pass orchestration mixed with scene state management

**Solution**: Extract RenderCoordinator

```rust
// moho_renderer/src/render_coordinator.rs (new module)
pub struct RenderCoordinator {
    shadow_pass: ShadowPass,
    csm_pass: CsmPass,
    main_pass: MainPass,
    skybox_pass: SkyboxPass,
}

impl RenderCoordinator {
    pub fn render(
        &self,
        encoder: &mut CommandEncoder,
        preparation: &ScenePreparation,
        // ... GPU resources
    ) {
        // Orchestrate render passes
    }
}
```

**Expected Results**:
- `scene.rs` CC: 25 → 15-20
- `scene.rs` MI: 15 → 25+
- New `render_coordinator.rs` MI: 35+

**Risk**: Low (extraction of orchestration logic, well-defined boundaries)

---

### **Track 14: Main Event Loop Refactoring** (3 SP) 🟡 MEDIUM PRIORITY

**Objective**: Extract App event handling to eliminate god object pattern (accidental complexity)

**Problem**:
- `main.rs::App` CC: 110, MI: 0, ~900 lines - **Accidental complexity**
- God object anti-pattern: Handles 20+ event types in one massive impl block
- Mixed concerns: UI events, game events, system events in single method

**Solution**: EventRouter + AppState separation

```rust
// src/app/event_router.rs (new module)
pub struct EventRouter {
    ui_handler: UiEventHandler,
    game_handler: GameEventHandler,
    system_handler: SystemEventHandler,
}

impl EventRouter {
    pub fn route_event(&self, event: &Event, app: &mut AppState) -> ControlFlow {
        match event {
            Event::WindowEvent { event, .. } => self.ui_handler.handle(event, app),
            Event::UserEvent(game_event) => self.game_handler.handle(game_event, app),
            // ... delegate to specialized handlers
        }
    }
}

// main.rs::App becomes thin wrapper
impl App {
    fn handle_event(&mut self, event: Event) {
        self.router.route_event(&event, &mut self.state)
    }
}
```

**Expected Results**:
- `main.rs::App` CC: 110 → 30-40
- `main.rs::App` lines: 900 → 400
- `main.rs` MI: 0 → 15-20
- New event handlers MI: 30+ each

**Risk**: Medium (core event loop, requires careful testing)

---

### **Track 15: Renderer Cleanup** (2 SP) 🟢 LOW PRIORITY

**Objective**: Extract render_mesh() stages to improve organization (accidental complexity)

**Problem**:
- `lib.rs::Renderer::render_mesh()` ~150 lines - **Accidental complexity**
- Mixed concerns: mesh rendering stages combined in one method

**Solution**: Extract rendering stages

```rust
// moho_renderer/src/mesh_renderer.rs (new module)
pub struct MeshRenderer;

impl MeshRenderer {
    pub fn prepare_instances(/* ... */) -> PreparedInstances { /* ... */ }
    pub fn bind_resources(/* ... */) { /* ... */ }
    pub fn draw_indexed(/* ... */) { /* ... */ }
}

// lib.rs delegates to mesh_renderer
impl Renderer {
    pub fn render_mesh(&mut self, /* ... */) {
        let instances = MeshRenderer::prepare_instances(/* ... */);
        MeshRenderer::bind_resources(/* ... */);
        MeshRenderer::draw_indexed(/* ... */);
    }
}
```

**Expected Results**:
- `lib.rs::render_mesh()` lines: 150 → 50
- `lib.rs::Renderer` CC: 55 → 30-35
- `lib.rs` MI: 0 → 10-15
- New `mesh_renderer.rs` MI: 35+

**Risk**: Low (well-defined extraction, comprehensive tests exist)

---

### **Track 16: Audio System State Machine** (2 SP) 🟢 LOW PRIORITY

**Objective**: Simplify audio_system.rs state management (mixed complexity)

**Problem**:
- `audio_system.rs::AudioSystem` CC: 57, MI: 10
- State machine has some accidental complexity from mixed concerns

**Solution**: Extract state management

```rust
// moho_audio/src/playback_state.rs (new module)
pub enum PlaybackState {
    Playing { sink: Sink, volume: f32 },
    Paused { sink: Sink, resume_volume: f32 },
    Stopped,
}

impl PlaybackState {
    pub fn play(&mut self, source: AudioSource) { /* ... */ }
    pub fn pause(&mut self) { /* ... */ }
    pub fn resume(&mut self) { /* ... */ }
    pub fn stop(&mut self) { /* ... */ }
}

// audio_system.rs becomes thinner
impl AudioSystem {
    pub fn play_sound(&mut self, id: SoundId) {
        if let Some(source) = self.sources.get(id) {
            self.state.play(source.clone());
        }
    }
}
```

**Expected Results**:
- `audio_system.rs` CC: 57 → 30-35
- `audio_system.rs` MI: 10 → 20-25
- New `playback_state.rs` MI: 35+

**Risk**: Low (audio system is well-isolated, minimal dependencies)

---

## Success Criteria

### Quantitative Goals (Revised)

| Metric | Current | Target | Stretch | Rationale |
|--------|---------|--------|---------|-----------|
| **Average MI** | 28 | **32-35** | 38+ | Realistic improvement focusing on accidental complexity |
| **Files with MI < 20** | 34 | **<20** | <15 | Target files with mixed concerns |
| **Functions with CC > 40** | 6 | **2-3** | 1 | Accept inherent complexity, eliminate accidental |
| **Critical MI: 0 files** | 3 | **1** | 0 | main.rs, scene.rs are accidental complexity |
| **Test coverage** | 336 tests | 370+ tests | 400+ tests | Add tests for extracted modules |

**Note**: Targets adjusted to reflect distinction between inherent complexity (accept) and accidental complexity (refactor).

### Qualitative Goals

1. **Distinguish complexity types**: Clearly document inherent vs. accidental complexity
2. **Eliminate accidental complexity**: God objects, tangled logic, mixed concerns refactored
3. **Accept inherent complexity**: Exhaustive enum matches, domain state machines documented as legitimate
4. **Clean separation**: Orchestration vs. implementation clearly separated
5. **Testability**: All extracted modules have comprehensive unit tests
6. **Zero regressions**: All 336+ tests passing throughout
7. **Clippy clean**: Zero warnings maintained

---

## Risk Assessment

### High-Risk Work (Requires Extra Care)

1. **Track 13 (Scene Management)**: Core rendering pipeline
   - **Mitigation**: Incremental extraction with tests after each step
   - **Validation**: Visual regression testing (screenshot comparisons)

2. **Track 14 (Event Loop)**: Application foundation
   - **Mitigation**: Extract handlers one at a time, validate between extractions
   - **Validation**: Full gameplay testing after each handler extraction

### Medium-Risk Work

- **Track 12.2 (Binding Serialization)**: Parsing optimization, well-tested
- **Track 13.1 (Scene Preparation)**: Clear boundaries, straightforward extraction

### Low-Risk Work

- **Track 15 (Renderer Cleanup)**: Isolated mesh rendering stages
- **Track 16 (Audio State Machine)**: Audio is well-isolated from other systems

---

## Sequencing Strategy (Revised)

### Sprint 1 (8 SP) — Focus on Accidental Complexity
1. **Track 13.1**: Scene Preparation Extraction (3 SP) - **HIGH ROI**
2. **Track 15**: Renderer Cleanup (2 SP) - **Related to Track 13**
3. **Track 16**: Audio System State Machine (2 SP) - **Low risk**
4. **Track 12.2**: Binding Serialization (1 SP partial) - **Start investigation**

**Rationale**: Lead with highest-value extractions (scene/renderer). Scene management is clear accidental complexity with well-defined boundaries. Delay remaining Track 12.2 work until pattern validated.

### Sprint 2 (7 SP) — Core System Extraction
1. **Track 12.2**: Binding Serialization (2 SP remaining) - **Complete with learnings**
2. **Track 13.2**: Render Coordination Extraction (2 SP) - **Builds on 13.1**
3. **Track 14**: Event Loop Refactoring (3 SP) - **Highest complexity, most prep**

**Rationale**: Complete Track 12.2 with Sprint 1 learnings. Extract render coordination after scene preparation validated. Begin event loop work (highest risk, benefits from prior experience).

### Sprint 3 (4 SP) — Polish & Documentation
1. **Track 14 (validation)**: Event Loop testing & refinement (1 SP)
2. **Documentation**: Update all metrics analysis and complexity audits (2 SP)
3. **Validation**: Full integration testing across all changes (1 SP)

**Rationale**: Final validation of highest-risk work. Comprehensive documentation of inherent vs. accidental complexity for future reference.

---

## Testing Strategy

### Unit Testing
- **Every extracted module**: Minimum 5-10 unit tests
- **Lookup tables**: Comprehensive key coverage tests
- **State machines**: All state transitions tested

### Integration Testing
- **Input system**: Test all 40+ keys with all modifier combinations
- **Rendering**: Visual regression tests (screenshot comparisons)
- **Audio**: Playback state transitions validated

### Manual Testing Checklist
- [ ] All keybinds work in settings menu
- [ ] Prefs save/load correctly
- [ ] Scene renders identically (visual check)
- [ ] Audio plays/pauses/stops correctly
- [ ] Event loop handles all event types
- [ ] No performance regressions (FPS check)

---

## Dependencies & Blockers

### External Dependencies
- **phf crate**: Required for compile-time lookup tables
  - Version: 0.13.1 (latest stable, zero issues)
  - Build impact: Minimal (compile-time code generation)
  - Use case: Static data lookups (code→string mappings)

### Internal Dependencies
- **Track 12.1 Complete**: PHF integration validated, patterns established ✅
- **Track 13.1 → 13.2**: Scene preparation must complete before coordination extraction
- **Track 14**: Benefits from completing Tracks 13 & 15 first (pattern recognition)
- **All other tracks**: Can proceed independently

### Blockers
- **None identified**: All prerequisite work (Phase 3 + Track 11) complete

---

## Rollback Strategy

### Per-Track Rollback
- Each track is a separate branch
- If track fails validation, revert branch and document lessons learned
- No impact on other tracks (independent work)

### Validation Gates
1. **After Track 12.1**: ✅ Complete - PHF validated, inherent complexity documented
2. **After Track 13**: Rendering validation (visual regression tests, metrics check)
3. **After Track 14**: Full application validation (all systems tested, final metrics)
4. **After each track**: Run full test suite + metrics + document actual vs. expected

### Emergency Rollback
- If critical bug discovered: revert to last known good commit
- Document issue in GitHub issue tracker
- Plan remediation work as Phase 4.1

---

## Post-Phase 4 Goals

### Expected Final State (Revised Targets)

| Metric | Current | Phase 4 Target | Notes |
|--------|---------|----------------|-------|
| **Average MI** | 28 | **32-35** | Focus on accidental complexity elimination |
| **Critical files (MI: 0)** | 3 | **1** | main.rs and scene.rs are primary targets |
| **High CC functions (>40)** | 6 | **2-3** | Accept inherent complexity (enum mappings) |
| **Test coverage** | 336 tests | 370+ tests | New tests for extracted modules |
| **Codebase health** | Good | **Excellent** | Clear separation, documented complexity |
| **Inherent complexity** | Undocumented | **Documented** | Clear rationale for high-CC functions |

### Future Work (Phase 5+)

After Phase 4, consider:

1. **Performance Optimization**
   - Profile hot paths (voxel mesh generation, rendering)
   - Optimize data structures (spatial indexing, caching)

2. **Feature Development**
   - Multiplayer networking (planned)
   - Advanced lighting (shadow improvements)
   - World generation enhancements

3. **Technical Improvements**
   - Async resource loading
   - GPU compute for terrain generation
   - Audio system 3D positioning

4. **Tooling & DevEx**
   - Hot reloading for shaders
   - In-game debugging UI
   - Performance profiling tools

---

## Conclusion

Phase 4 represents the **final major refactoring effort** before shifting focus to feature development. The work is well-scoped, risk-assessed, and sequenced for success.

**Key Principles**:
1. **Distinguish complexity types**: Inherent (accept & document) vs. Accidental (refactor)
2. **Focus on impact**: Target god objects and mixed concerns, not legitimate domain mappings
3. **Incremental progress**: Small, testable changes with validation gates
4. **Zero regressions**: Maintain all existing functionality
5. **Realistic targets**: Metrics adjusted to reflect complexity realities
6. **Quality first**: Don't rush, validate thoroughly

**Phase 4 Philosophy**: Not all complexity is bad. Some complexity is inherent to the problem domain (keyboards have 40+ keys, applications handle 20+ event types). Phase 4 eliminates **accidental complexity** (poor organization, god objects, tangled logic) while **documenting and accepting inherent complexity** (exhaustive enums, domain state machines).

**Estimated Timeline**: 3 sprints (19 SP total)  
**Confidence Level**: High (building on successful Phase 3 + Track 11 experience, with realistic expectations)

Let's finish strong! 🚀
