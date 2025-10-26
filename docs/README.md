# Voxel Terrain System - Documentation Index

**Last Updated**: October 24, 2025  
**Branch**: `world-generation` → `custom-mesh-rendering` (next)

---

## 📚 Document Overview

> Cleanup notes and low-risk findings were added to `docs/CLEANUP_NOTES.md` on 2025-10-25. (Removed the previously archived third-party README from the repository root.)


### Current State & Next Steps
- **[CURRENT_STATE.md](CURRENT_STATE.md)** ⭐ START HERE
  - Complete status of implemented features
  - Detailed explanation of rendering blocker
  - Statistics and test results
  - What works vs. what's blocked

- **[CUSTOM_MESH_RENDERING_PLAN.md](CUSTOM_MESH_RENDERING_PLAN.md)** 🎯 NEXT PHASE
  - Detailed implementation plan for Option B
  - 5 phases with estimated hours
  - Code examples and architecture diagrams
  - Testing strategy and success criteria
  - **Read this before starting renderer work**

### Historical Plans (Completed)
- **[terrain_generation_plan_revised.md](terrain_generation_plan_revised.md)** 
  - Original comprehensive plan for voxel system
  - Architecture overview and design decisions
  - Implementation details for all phases
  - Reference for understanding system design

- **[phase3_rendering_plan.md](phase3_rendering_plan.md)**
  - Phase 3 specific plan (completed with blocker)
  - Face culling strategy and implementation
  - Chunk-based rendering approach
  - Now includes completion summary

- **[terrain_generation_plan.md](terrain_generation_plan.md)** (DEPRECATED)
  - Original plan using deformed mesh approach
  - Superseded by voxel-based system
  - Kept for historical reference

---

## 🗺️ Implementation Timeline

### ✅ Phase 1: Foundation (Complete)
- Added noise dependency
- Created voxel module with core structures
- Material and resource registries
- **Time**: ~2-3 hours

### ✅ Phase 2: Terrain Generation (Complete)
- Perlin noise integration
- Multi-octave sampling
- Terrain type variations
- Material/resource assignment
- Smoothing algorithm for ramps
- **Time**: ~4-5 hours

### ✅ Phase 3: Rendering Integration (Complete - Blocked)
- Face culling logic (87% triangle reduction)
- VoxelChunk component
- Chunk-based rendering structure
- ECS integration
- Camera positioning fix
- **Time**: ~4-5 hours
- **Blocker**: Renderer doesn't support custom meshes

### 🎯 Phase 4: Custom Mesh Rendering (Next)
- Extend renderer for custom geometry
- GPU buffer management per chunk
- Hybrid rendering (instances + custom meshes)
- Testing and validation
- **Estimated Time**: 6-8 hours
- **See**: `CUSTOM_MESH_RENDERING_PLAN.md`

---

## 🚀 Quick Start for Next Session

1. **Review Current State**
   ```bash
   # Read this first to understand what's done
   code docs/CURRENT_STATE.md
   ```

2. **Study the Plan**
   ```bash
   # Understand the approach before coding
   code docs/CUSTOM_MESH_RENDERING_PLAN.md
   ```

3. **Commit Current Work**
   ```bash
   git add -A
   git commit -m "Phase 3 complete: Voxel terrain generation with face culling"
   ```

4. **Create New Branch**
   ```bash
   git checkout -b custom-mesh-rendering
   ```

5. **Explore Renderer**
   ```bash
   # Study these files first
   code engine_renderer/src/lib.rs
   code engine_renderer/src/scene.rs
   code engine_renderer/src/gpu_types.rs
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
- `engine_core/Cargo.toml` - Added noise dependency
- `engine_core/src/lib.rs` - Exported voxel module
- `engine_core/src/voxel.rs` - Complete voxel system (644 lines)
- `engine_core/src/scene_builders.rs` - Terrain generation
- `engine_core/src/actors.rs` - Instance collection
- `src/main.rs` - Camera position and scene selection

### Key Files to Modify Next
- `engine_core/src/actors.rs` - Add `CustomMesh` trait
- `engine_renderer/src/scene.rs` - Add custom mesh manager
- `engine_renderer/src/lib.rs` - Integrate custom rendering
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
- `engine_core/src/voxel.rs` - Core voxel system
- `engine_core/src/scene_builders.rs` - Terrain generation
- `engine_core/src/actors.rs` - Renderable trait
- `engine_renderer/src/` - Rendering system (to be modified)

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
