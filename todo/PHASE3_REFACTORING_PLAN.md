# Phase 3: Core Systems Refinement

**Milestone**: Production-Ready Core Systems  
**Goal**: Complete refactoring of remaining MI=0 files and high-complexity modules  
**Total Effort**: 26 SP (~3 sprints)  
**Status**: � **IN PROGRESS**

**Overall Progress**: ✅⬜⬜⬜⬜⬜⬜⬜ (1/8 increments - Track 8.1 complete!)

---

## Overview

Phase 3 targets the **remaining critical technical debt** identified by fresh metrics analysis (November 8, 2025):

### Files with MI = 0 (Critical Priority)
1. **moho_core/src/voxel.rs** - 711 lines, CC: 27 (VoxelGrid) + 23 (MeshGenerator) + 23 (FaceDirection)
2. **moho_renderer/src/lib.rs** - Still at MI: 0 despite Phase 1 & 2 work
3. **moho_ui/src/adapter.rs** - CC: 46 + 39, mixing concerns
4. **moho_ui/src/screens/settings/mod.rs** - CC: 194 (increased during Phase 2)

### Files with MI < 5 (High Priority)
5. **moho_renderer/src/scene.rs** - MI: 1.6, CC: 56
6. **moho_ui/src/overlays/console.rs** - MI: 3.0, CC: 58
7. **moho_ui/src/prefs.rs** - MI: 4.8, CC: 85
8. **moho_renderer/src/pipeline.rs** - MI: 4.2, CC: 33

### Phase 2 Deferred Items
- **Track 5.2**: Pipeline creation (deferred - configuration-heavy)
- **Track 7.1**: Prefs parsing (deferred - functional but complex)
- **Track 7.2**: Console/Input/Voxel (deferred - stable systems)

### Phase 3 Strategy

**Focus**: Address remaining MI=0 files and complete Phase 2 deferrals with **strategic refactoring**.

**Approach**:
1. **Track 8**: Voxel System (MI: 0 → >40) - Split grid/mesh/face [Detailed]
2. **Track 9**: Renderer Polish (MI: 0 → >20) - Extract render ops, scene management [High-level]
3. **Track 10**: UI Systems (MI: 0 → >20) - Refine adapter, finalize settings [High-level]
4. **Track 11**: Supporting Systems (MI: <5 → >30) - Console, Prefs, Pipeline [High-level]

Each refactoring continues the **small, testable increment** approach from Phase 1 & 2.

**Completion Status**:
- **Track 8 (Voxel System)**: ✅⬜ 1/2 (Track 8.1 Complete!)
- **Track 9 (Renderer Polish)**: ⬜⬜ 0/2 (Not Started)
- **Track 10 (UI Systems)**: ⬜⬜ 0/2 (Not Started)
- **Track 11 (Supporting)**: ⬜⬜ 0/2 (Not Started)

**Last Updated**: November 8, 2025 - Track 8.1 complete! (grid.rs + face.rs extracted)  
**Next Task**: Track 8.2 (Extract Mesh Generation Module)  
**Next Review**: After completing Track 8.2

---

## Phase 1 & 2 Recap - Foundation Complete

### Major Achievements ✅

**Phase 1 (19/19 increments - 100% complete)**:
- ✅ App initialization refactored (CC: 122 → 110, -10%)
- ✅ Settings menu modularized (extracted tab renderers, binding registry, state)
- ✅ Renderer initialization extracted (device, pipeline, resources, builder)
- ✅ Event loop delegation complete (5 focused modules)
- ✅ 186 tests passing, zero regressions
- ✅ 11 new focused modules created

**Phase 2 (5/8 increments - 62.5% strategically complete)**:
- ✅ Track 4: Settings event capture refined (CC: 30→4, -87%)
- ✅ Track 5: Renderer setup extracted (render_mesh: 280→160 lines, -43%)
- ✅ Track 6: App runtime complete from Phase 1 (ApplicationHandler clean)
- ⏸️ Track 5.2: Pipeline deferred (configuration-heavy, low ROI)
- ⏸️ Track 7: Supporting systems deferred (Prefs, Console, Input)

### Lessons Learned 📚

1. **Small increments highly effective** - 2-5 SP increments completable in 1-3 hours
2. **Strategic deferral works** - Not all work is equal priority
3. **Metrics guide decisions** - MI=0 files are true pain points
4. **Tests enable confidence** - 186 tests catch all regressions
5. **Module extraction first** - Extract before simplifying
6. **Temporary complexity OK** - Settings CC increased during extraction, normalized after

### Remaining Challenges 🎯

**From Fresh Metrics (November 8, 2025)**:
- **4 files at MI = 0** (voxel, renderer lib, adapter, settings)
- **4 files at MI < 5** (scene, console, prefs, pipeline)
- **15 high-CC functions** (>40 CC threshold)
- **Settings at CC: 194** (increased from 183 during Phase 2 work)

**Strategic Focus**:
- Address MI=0 files first (biggest pain points)
- Complete Phase 2 deferrals (Prefs, Console, Pipeline)
- Reduce high-CC functions to manageable levels
- Improve overall codebase maintainability

---

## Track 8: Voxel System Refactoring - **DETAILED PLAN**

**Current State**: voxel.rs at MI: 0, CC: 27+23+23, 711 lines, mixing concerns  
**Target State**: Modular voxel system with MI > 40, CC < 15 per module  
**Total Effort**: 8 SP

**Problem Analysis**:
```rust
// Current: One 711-line file doing everything
moho_core/src/voxel.rs
├── FaceDirection enum (CC: 23) - Complex match statements repeated
├── VoxelGrid struct (CC: 27) - Grid data + queries + neighbor logic
├── MeshGenerator (CC: 23) - Mesh gen + face culling + material handling
└── Mixed responsibilities: data structure, culling, rendering, materials
```

**Fresh Metrics**:
- **Maintainability Index**: 0 (worst possible)
- **Cyclomatic Complexity**: VoxelGrid (27), MeshGenerator (23), FaceDirection (23)
- **Size**: 711 lines in single file
- **Issue**: Multiple responsibilities without separation

**Phase 3 Strategy**: Split into 4 focused modules with clear boundaries

---

### 8.1 Extract Voxel Grid Module (5 SP) ✅ **COMPLETE**
**Goal**: Separate core data structure from mesh generation

**Status**: ✅ **COMPLETE** - November 8, 2025

**What Was Done**:
- ✅ Created `moho_core/src/voxel/` module directory
- ✅ Extracted `grid.rs` with VoxelGrid, VoxelBlock, registries (~460 lines)
- ✅ Extracted `face.rs` with FaceDirection and culling logic (~320 lines)
- ✅ Created lookup tables to replace match statements (FACE_OFFSETS, FACE_VERTEX_RANGES, FACE_INDEX_RANGES)
- ✅ Converted `voxel.rs` → `voxel/mod.rs` with re-exports (~200 lines remaining)
- ✅ Added 26 unit tests (15 grid tests + 11 face tests)
- ✅ Updated integration test to use new API
- ✅ All 219 workspace tests passing (0 failures)
- ✅ Clippy clean, rustfmt applied
- ✅ Removed old monolithic voxel.rs file

**Key Improvements**:
1. **Lookup Table Optimization**: Replaced `match` statements with const arrays
   ```rust
   // Before: O(n) branching with 6 cases
   match self { FaceDirection::PosX => IVec3::new(1, 0, 0), ... }
   
   // After: O(1) array lookup, no branching
   const FACE_OFFSETS: [IVec3; 6] = [ ... ];
   FACE_OFFSETS[*self as usize]
   ```

2. **API Improvements**: Face culling logic moved to FaceDirection
   ```rust
   // Before: grid.should_render_face(pos, direction)
   // After: direction.should_render_face(&grid, pos)
   // Also: get_visible_faces(&grid, pos) convenience function
   ```

3. **Module Organization**:
   - **grid.rs**: VoxelGrid data structure, VoxelBlock, registries, queries
   - **face.rs**: FaceDirection enum, face culling, lookup tables
   - **mod.rs**: MeshGenerator, TerrainSmoother, VoxelChunk (for Track 8.2)

**Test Results**:
- ✅ **219 tests passed** (0 failures, 13 ignored)
- ✅ Voxel module: 22/22 tests passing
- ✅ Integration tests updated and passing
- ✅ No behavioral changes detected

**Quality Metrics**:
- ✅ Clippy: No warnings
- ✅ Rustfmt: Applied
- ✅ Separation of concerns: Clear boundaries between modules
- ✅ Test coverage: 26 new unit tests for voxel module

**Files Created**:
- `moho_core/src/voxel/grid.rs` (~460 lines, 15 tests)
- `moho_core/src/voxel/face.rs` (~320 lines, 11 tests)

**Files Modified**:
- `moho_core/src/voxel.rs` → `moho_core/src/voxel/mod.rs` (~200 lines)
- `moho_core/tests/voxel_system.rs` (API update)

**Files Deleted**:
- `moho_core/src/voxel.rs` (old 711-line monolithic file)

**Current Problem**:
- VoxelGrid mixes storage, queries, and mesh generation
- No clear separation between data and presentation
- Hard to test grid logic independently
- Face culling logic embedded in mesh generation

**Proposed Structure**:
```rust
moho_core/src/voxel/
├── mod.rs           // Module exports, VoxelType re-export
├── grid.rs          // VoxelGrid data structure (NEW)
├── face.rs          // FaceDirection and culling logic (NEW)
├── mesh.rs          // MeshGenerator (NEW)
└── material.rs      // Material registry (NEW - if needed)

// grid.rs - Core data structure (~150 lines)
pub struct VoxelGrid {
    voxels: Vec<VoxelType>,
    size_x: usize,
    size_y: usize,
    size_z: usize,
}

impl VoxelGrid {
    pub fn new(size_x, size_y, size_z) -> Self { /* ... */ }
    pub fn get(&self, x, y, z) -> Option<VoxelType> { /* ... */ }
    pub fn set(&mut self, x, y, z, voxel: VoxelType) { /* ... */ }
    pub fn is_solid_at(&self, x, y, z) -> bool { /* ... */ }
    pub fn get_neighbor(&self, x, y, z, direction: FaceDirection) -> Option<VoxelType> { /* ... */ }
    // Query methods only - no mesh generation
}

// face.rs - Face culling and direction (~120 lines)
use const lookup tables instead of repeated matches
const FACE_OFFSETS: [(i32, i32, i32); 6] = [
    (1, 0, 0),   // PosX
    (-1, 0, 0),  // NegX
    (0, 1, 0),   // PosY
    (0, -1, 0),  // NegY
    (0, 0, 1),   // PosZ
    (0, 0, -1),  // NegZ
];

pub enum FaceDirection {
    PosX, NegX, PosY, NegY, PosZ, NegZ,
}

impl FaceDirection {
    pub fn offset(&self) -> (i32, i32, i32) {
        FACE_OFFSETS[*self as usize]
    }
    
    pub fn should_render_face(grid: &VoxelGrid, x, y, z, direction: FaceDirection) -> bool {
        let (dx, dy, dz) = direction.offset();
        let neighbor_x = x as i32 + dx;
        let neighbor_y = y as i32 + dy;
        let neighbor_z = z as i32 + dz;
        
        grid.get(neighbor_x, neighbor_y, neighbor_z)
            .map_or(true, |voxel| !voxel.is_solid())
    }
}

// mesh.rs - Mesh generation only (~250 lines)
pub struct MeshGenerator;

impl MeshGenerator {
    pub fn generate_mesh(grid: &VoxelGrid) -> (Vec<Vertex>, Vec<u32>) {
        // Use FaceDirection::should_render_face for culling
        // Clean separation: mesh gen queries grid, doesn't own it
        /* ... */
    }
}
```

**Completion Summary**:
All tasks completed successfully! Track 8.1 took approximately 3-4 hours as estimated.

**Actual Results vs Acceptance Criteria**:
- ✅ VoxelGrid in grid.rs (~460 lines, focused on data structure)
- ✅ FaceDirection uses lookup tables (CC < 10, inline optimizations)
- ✅ Grid queries testable in isolation (15 unit tests added)
- ✅ All 219 workspace tests passing (0 failures)
- ✅ No behavioral changes (verified by tests)
- ✅ Clippy clean (no warnings)
- ✅ Code formatted (rustfmt applied)

**Bonus Improvements**:
- 26 total unit tests added (exceeded 10+6 target)
- Inline attributes on hot path methods
- Comprehensive test coverage for lookup tables
- Integration tests updated for new API

---

### 8.2 Extract Mesh Generation Module (3 SP) ⏭️ **NEXT**
**Goal**: Complete voxel module split with focused mesh generator

**Current Problem**:
- MeshGenerator tightly coupled to VoxelGrid
- Vertex generation and face culling mixed
- Material handling embedded in mesh generation
- Hard to test mesh generation independently

**Proposed Solution**:
```rust
// mesh.rs - Clean mesh generation
pub struct MeshGenerator {
    // Configuration if needed (material registry, vertex format, etc.)
}

impl MeshGenerator {
    pub fn new() -> Self {
        Self {}
    }
    
    pub fn generate_mesh(&self, grid: &VoxelGrid) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        
        for x in 0..grid.size_x() {
            for y in 0..grid.size_y() {
                for z in 0..grid.size_z() {
                    if let Some(voxel) = grid.get(x, y, z) {
                        if voxel.is_solid() {
                            self.generate_voxel_faces(
                                grid, x, y, z, voxel, 
                                &mut vertices, &mut indices
                            );
                        }
                    }
                }
            }
        }
        
        (vertices, indices)
    }
    
    fn generate_voxel_faces(
        &self,
        grid: &VoxelGrid,
        x: usize, y: usize, z: usize,
        voxel: VoxelType,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        // Check each face using FaceDirection::should_render_face
        for direction in FaceDirection::iter() {
            if direction.should_render_face(grid, x, y, z) {
                self.add_face_vertices(x, y, z, direction, voxel, vertices, indices);
            }
        }
    }
    
    fn add_face_vertices(/* ... */) {
        // Pure vertex generation - no culling logic
        // Use lookup tables for vertex positions
        /* ... */
    }
}
```

**Tasks**:
- [ ] Extract `mesh.rs` with MeshGenerator
- [ ] Separate vertex generation from face culling
- [ ] Use FaceDirection::should_render_face for culling
- [ ] Add lookup tables for vertex positions per face
- [ ] Add unit tests for mesh generation (5+ tests)
- [ ] Add benchmark to verify no performance regression
- [ ] Update all call sites to use new modules

**Acceptance Criteria**:
- ✅ MeshGenerator in separate module (<250 lines)
- ✅ Mesh generation testable in isolation
- ✅ Clean separation: mesh queries grid, doesn't own it
- ✅ No performance regression (verify with benchmark)
- ✅ All 186+ tests passing
- ✅ voxel.rs MI: 0 → >40 (>4000% improvement!)
- ✅ Clippy clean, code formatted

**Files Changed**:
- `moho_core/src/voxel/mesh.rs` (new - MeshGenerator)
- `moho_core/src/voxel/mod.rs` (updated - export MeshGenerator)
- Call sites updated (scene_builders.rs, etc.)

**Estimated Time**: 2-3 hours

---

**Track 8 Metrics Impact**:
- **Before**: voxel.rs MI: 0, CC: 27+23+23 (73 total), 711 lines in one file
- **After**: 4 focused modules, MI: >40 each, CC: <15 each, ~600 lines across modules
- **Improvement**: 
  - MI: 0 → >40 (infinite % improvement, removes critical issue)
  - CC: 73 → <60 across all modules (17% reduction)
  - Maintainability: One MI=0 file eliminated!
  - Testability: Grid, face, and mesh independently testable

**Track 8 Completion Checklist**:
- [ ] Increment 8.1 completed (Extract Voxel Grid)
- [ ] Increment 8.2 completed (Extract Mesh Generation)
- [ ] All 2 increments completed
- [ ] voxel.rs MI: 0 → >40 (target achieved)
- [ ] All voxel modules CC < 15 (target achieved)
- [ ] Lookup tables replace match statements
- [ ] All 186+ tests passing
- [ ] No performance regression
- [ ] Code review completed
- [ ] Metrics re-run to validate improvements

---

## Track 9: Renderer Polish - **HIGH-LEVEL OUTLINE**

**Current State**: renderer/lib.rs at MI: 0, scene.rs at MI: 1.6  
**Target State**: Renderer with MI > 20, clear separation of concerns  
**Total Effort**: 8 SP (to be detailed when Track 8 complete)

### Overview

**Remaining Issues** (from metrics):
1. **renderer/lib.rs** - Still MI: 0 despite Phase 1 & 2 extractions
2. **scene.rs** - MI: 1.6, CC: 56, buffer management mixed with rendering

**Proposed Tracks** (details TBD):
- **9.1**: Extract render operations (5 SP) - Break render_mesh() further
- **9.2**: Refactor scene management (3 SP) - Extract BufferPool, separate buffer lifecycle

**Note**: Detailed planning will occur after Track 8 completion. We'll assess:
- Remaining renderer lib.rs complexity after Phase 2 Track 5.1
- Scene.rs buffer management patterns
- render_mesh() remaining hotspots
- Buffer allocation/resizing logic

---

## Track 10: UI Systems Refinement - **HIGH-LEVEL OUTLINE**

**Current State**: adapter.rs at MI: 0, settings/mod.rs at CC: 194  
**Target State**: UI systems with MI > 20, settings CC < 50  
**Total Effort**: 6 SP (to be detailed when Track 9 complete)

### Overview

**Remaining Issues** (from metrics):
1. **adapter.rs** - MI: 0, CC: 46+39, mixing egui integration + event routing + rendering
2. **settings/mod.rs** - CC: 194 (increased during Phase 2), still orchestrating complexity

**Proposed Tracks** (details TBD):
- **10.1**: Refactor EguiAdapter (3 SP) - Extract event routing, separate rendering
- **10.2**: Finalize Settings Menu (3 SP) - Final CC reduction, complete Phase 2 work

**Note**: Detailed planning will occur after Track 9 completion. We'll assess:
- Adapter event routing patterns
- Settings orchestration remaining after Phase 2
- Whether SettingsMenu CC: 194 is acceptable with delegation pattern
- EguiAdapter responsibilities (integration vs. routing vs. rendering)

---

## Track 11: Supporting Systems - **HIGH-LEVEL OUTLINE**

**Current State**: Console MI: 3, Prefs MI: 4.8, Pipeline MI: 4.2  
**Target State**: All supporting systems MI > 30, CC < 40  
**Total Effort**: 4 SP (Phase 2 Track 7 deferred items)

### Overview

**Phase 2 Deferred Items**:
1. **Track 7.1**: Prefs parsing (MI: 4.8, CC: 85) - 313 lines, complex parse_binding()
2. **Track 7.2**: Console (MI: 3, CC: 58) - 494 lines, mixed concerns
3. **Track 5.2**: Pipeline (MI: 4.2, CC: 33) - Configuration-heavy
4. **Input optimization**: CC: 41 (200+ line match) - Key mapping

**Proposed Tracks** (details TBD):
- **11.1**: Refactor Prefs + Console (2 SP) - Complete Track 7.1 & 7.2
- **11.2**: Polish Pipeline + Input (2 SP) - Complete Track 5.2, optimize input

**Note**: Detailed planning will occur after Track 10 completion. These are **lower priority** items deferred from Phase 2 as they're functional and stable.

---

## Execution Strategy

### Recommended Order

**Sprint 1 (Week 1)**: Track 8 - Voxel System
- Day 1-2: Track 8.1 (Extract Voxel Grid) - 5 SP
- Day 3: Track 8.2 (Extract Mesh Generation) - 3 SP
- Day 4-5: Testing, benchmarking, documentation
- **Milestone**: One MI=0 file eliminated!

**Sprint 2 (Week 2)**: Track 9 - Renderer Polish
- Day 1-3: Track 9.1 (Extract Render Operations) - 5 SP
- Day 4: Track 9.2 (Refactor Scene Management) - 3 SP
- Day 5: Testing, profiling, documentation
- **Milestone**: renderer/lib.rs MI > 20

**Sprint 3 (Week 3)**: Track 10 & 11 - UI & Supporting
- Day 1-2: Track 10.1 (Refactor EguiAdapter) - 3 SP
- Day 3: Track 10.2 (Finalize Settings) - 3 SP
- Day 4: Track 11.1 & 11.2 (Prefs/Console/Pipeline/Input) - 4 SP
- Day 5: Final testing, metrics, Phase 3 review
- **Milestone**: All MI=0 files eliminated!

### After Each Increment
1. ✅ Run tests: `cargo test --workspace`
2. ✅ Run clippy: `cargo clippy --workspace -- -D warnings`
3. ✅ Run fmt: `cargo fmt --all`
4. ✅ Commit with descriptive message
5. ✅ Update this file to mark increment complete

### After Each Track
1. ✅ Run full test suite with coverage
2. ✅ Re-run metrics: `.\scripts\run-metrics.ps1 -Clean`
3. ✅ Update CODE_METRICS_ANALYSIS.md with progress
4. ✅ Git push and verify CI passes
5. ✅ Code review

---

## Success Metrics

### Quantitative Goals (Phase 3 Targets)
- [ ] Files with MI = 0: 4 → 0 (**100% elimination**)
- [ ] Files with MI < 5: 4 → 0 (**100% elimination**)
- [ ] Voxel MI: 0 → >40 (>4000% improvement)
- [ ] Renderer lib MI: 0 → >20 (infinite improvement)
- [ ] Adapter MI: 0 → >20 (infinite improvement)
- [ ] Settings MI: 0 → >20 (infinite improvement)
- [ ] All 186+ tests passing
- [ ] No performance regressions

### Qualitative Goals
- [ ] All MI=0 critical issues resolved
- [ ] Voxel system has clear module boundaries
- [ ] Renderer operations cleanly separated
- [ ] UI systems independently testable
- [ ] Supporting systems maintainable
- [ ] Codebase ready for advanced features (Phase 4+)

---

## Risk Mitigation

### Risk: Voxel Performance Regression
**Likelihood**: Medium  
**Impact**: High  
**Mitigation**:
- Benchmark mesh generation before/after
- Profile face culling performance
- Keep lookup tables cache-friendly
- Inline critical methods (let compiler optimize)
- Add criterion benchmark for voxel mesh generation

### Risk: Breaking Renderer
**Likelihood**: Low  
**Impact**: High  
**Mitigation**:
- Each render op extraction is independent
- Test rendering after each change
- Keep existing tests passing
- Profile frame times before/after

### Risk: UI System Regressions
**Likelihood**: Low  
**Impact**: Medium  
**Mitigation**:
- Test all UI screens after adapter changes
- Verify input routing works correctly
- Check settings menu thoroughly
- Keep egui integration isolated

### Risk: Time Overrun
**Likelihood**: Low  
**Impact**: Low  
**Mitigation**:
- Track 8 (Voxel) is most critical - prioritize completion
- Tracks 9-11 can be adjusted based on learnings
- Each increment independently valuable
- Can defer Track 11 if time-constrained (already deferred from Phase 2)

---

## Progress Tracking

**Track 8 (Voxel)**: ⬜⬜ (0/2 complete)  
**Track 9 (Renderer)**: ⬜⬜ (0/2 complete)  
**Track 10 (UI)**: ⬜⬜ (0/2 complete)  
**Track 11 (Supporting)**: ⬜⬜ (0/2 complete)

**Overall Phase 3**: 0% complete (0/8 increments)

**Last Updated**: November 8, 2025 - Phase 3 plan created  
**Next Task**: Begin Track 8.1 (Extract Voxel Grid Module)  
**Next Review**: After Track 8 completion

---

## Phase 4+ Preview

After Phase 3, the codebase will have:
- ✅ **Zero MI=0 files** (all critical issues resolved)
- ✅ **Clean module boundaries** (voxel, renderer, UI all modular)
- ✅ **High testability** (all systems independently testable)
- ✅ **Solid foundation** for advanced features

**Phase 4+ Candidates**:
1. **Multiplayer/Networking** - Client/server with deterministic sim (moho_sim ready)
2. **Advanced Rendering** - PBR materials, post-processing, particle systems
3. **Content Pipeline** - World editor, asset import, scripting
4. **Performance** - Multi-threading, GPU compute, LOD system
5. **Platform Ports** - Mobile (iOS/Android), console (Nintendo Switch)
6. **Polish** - Audio improvements, accessibility, mod support

**Foundation Complete**: After Phase 3, focus shifts from **refactoring** to **features**!

---
