# Moho Voxel Engine: Lighting & Shadow System Implementation Plan

## Overview

This document outlines a phased implementation strategy for upgrading the lighting and shadow system in the Moho voxel engine. The plan supports **hybrid geometry** (smoothed terrain + blocky structures) with modern lighting techniques and **runtime terrain modification**. 

### Target Features
- Physically-based soft shadows (distance-dependent softness via PCSS)
- Multiple light sources
- Light propagation (torch lighting, emissives)
- Volumetric lighting (god rays)
- Environment effects affecting lighting (weather, time of day)
- Runtime terrain modification with responsive updates

### Current State
The engine already has:
- 4-cascade CSM with split distances at [20, 50, 100, 200] units
- PCF 3x3 soft shadows with slope-scale depth bias
- Sun/moon dual lighting with day/night cycle
- Basic ambient occlusion approximation (contact shadows)
- Voxel chunk system with face culling

### Key Architectural Decisions

1. **Unified rendering pipeline** with material-driven distinction between smooth terrain and blocky structures.  Both geometry types share the same shaders, shadows, and lighting—only mesh generation and ambient occlusion sourcing differ.

2.  **Async mesh generation** for smooth terrain to maintain frame rate during modifications. 

3. **Incremental light propagation** to avoid full recomputation on every block change.

4. **Percentage Closer Soft Shadows (PCSS)** for physically-accurate shadow softness based on occluder-to-receiver distance. 

---

## Phase 0: Terrain Modification Infrastructure

**Status: ✅ COMPLETE**

**Goal:** Establish the systems needed for runtime terrain changes before implementing lighting features.

### Tasks

1. **Block modification API** ✅
   - Implement `set_block(pos, block)` with automatic chunk dirtying
   - Implement `remove_block(pos)` with cleanup
   - Support batch modifications for bulk edits (explosions, world gen)

2. **Chunk state management** ✅
   - Add dirty flags: `terrain_mesh_dirty`, `structure_mesh_dirty`, `light_dirty`
   - Define generation states: `Idle`, `Queued`, `Generating`, `Ready`
   - Implement pending mesh buffer for double-buffering

3. **Background worker system** ✅
   - Create thread pool for mesh generation jobs
   - Implement job queue with priority (distance to player)
   - Support job cancellation when player moves away

4. **Neighbor chunk invalidation** ✅
   - Detect when block changes affect chunk boundaries
   - Automatically dirty adjacent chunks for edge blocks
   - Propagate invalidation for both mesh and lighting

5. **Mesh double-buffering** ✅
   - Render old mesh while new one generates in background
   - Atomic swap when new mesh is ready
   - GPU buffer reuse/pooling to reduce allocations

### Deliverables ✅
- `ChunkState` struct with dirty flags and generation state
- `BlockModifier` API with automatic neighbor propagation
- `MeshJobQueue` with priority and cancellation tokens
- Double-buffered mesh swap system in `BufferManager`

### Implementation Summary

| Component | File | Description |
|-----------|------|-------------|
| `ChunkState` | `moho_core/src/voxel/state.rs` | State machine with `DirtyFlags` and `GenerationState` |
| `BlockModifier` | `moho_core/src/voxel/modification.rs` | High-level API wrapping `VoxelGrid` with auto-dirtying |
| `MeshJobQueue` | `moho_core/src/voxel/jobs.rs` | Priority queue with `CancellationToken` support |
| World events | `moho_core/src/events/types/world.rs` | `BlockPlaced`, `BlockRemoved`, `ChunkMeshDirty`, etc. |
| Double-buffering | `moho_renderer/src/buffer_manager.rs` | `upload_pending_mesh()`, `swap_chunk_mesh()`, buffer pooling |

### Performance Targets

| Operation | Target |
|-----------|--------|
| Single blocky block edit | < 5ms (can be synchronous) |
| Single smooth block edit | < 1 frame stutter (async required) |
| Chunk mesh swap | < 1ms |

---

## Phase 1: Dual Mesh Generation

**Status: ✅ COMPLETE (Visually Tested & Validated)**

**Goal:** Implement hybrid mesh generation supporting both smooth terrain and blocky structures with modification support.

### Tasks

1. **Block smoothness determination** ✅
   - Add `is_smooth()` method to block based on material ID
   - Natural materials (dirt, stone, grass) → smooth
   - Crafted materials (planks, bricks, metal) → blocky
   - Simple ID range check or lookup table

2. **Implement isosurface extraction for terrain** ✅
   - Add Marching Cubes algorithm for smooth geometry
   - Compute normals from density field gradient using central differences
   - Structure blocks act as solid boundaries in the density field

3. **Retain greedy meshing for structures** ✅
   - Keep existing face culling and mesh merging for blocky geometry
   - Compute per-vertex AO using 4-corner neighbor occupancy lookup

4. **Handle mixed chunks** ✅
   - Detect chunks containing both terrain and structure blocks
   - Generate sub-meshes independently (can dirty one without the other)
   - Concatenate into single vertex buffer for rendering

5. **Integrate with async system** ✅
   - Mesh generation runs on background threads
   - Smooth terrain always async; blocky can be sync for single edits
   - Priority based on player distance and visibility

### Deliverables ✅
- `VoxelBlock::is_smooth()` method and `BlockCategory` enum
- `MarchingCubes` using isosurface extraction with density fields
- `BlockyMeshGenerator` with per-vertex AO computation (4-corner method)
- `HybridMeshGenerator` with chunk content classification
- Integration with Phase 0 job queue via `create_hybrid_generator()`
- All 85 voxel tests passing
- **Visual validation complete: Smooth terrain renders correctly with proper surface continuity**

### Implementation Summary

| Component | File | Description |
|-----------|------|-------------|
| Block classification | `moho_core/src/voxel/grid.rs` | `BlockCategory` enum, `is_smooth()` method |
| Marching Cubes | `moho_core/src/voxel/mesh/marching_cubes.rs` | Smooth terrain mesh generation (triangle indexing bug fixed) |
| Blocky mesh + AO | `moho_core/src/voxel/mesh/blocky.rs` | Per-vertex AO using neighbor lookup |
| Hybrid generator | `moho_core/src/voxel/mesh/hybrid.rs` | Content analysis and mesh concatenation |
| Job queue integration | `moho_core/src/voxel/jobs.rs` | `create_hybrid_generator()` helper |
| VoxelChunk constructor | `moho_core/src/voxel/chunk.rs` | `from_grid_hybrid()` method |
| Scene generation | `moho_core/src/scene_builders.rs` | Updated to use hybrid system with chunk_size=16 |

**Critical Bug Fix:** Triangle indexing in Marching Cubes required moving `base_vertex` calculation inside triangle loop to ensure correct vertex references for multi-triangle cells. This resolved holes in terrain rendering.

### Mesh Generation Performance

| Geometry Type | Sync Feasible | Async Required |
|---------------|---------------|----------------|
| Blocky (single block) | Yes | No |
| Blocky (chunk rebuild) | Maybe | Recommended |
| Smooth (any change) | No | Yes |
| Mixed chunk | No | Yes |

---

## Phase 2: Extended Vertex Format & Shader Foundation

**Goal:** Extend the vertex format to carry ambient occlusion and geometry type data through the pipeline.

### Tasks

1. **Extend vertex attributes**
   - Add `ao: f32` (0. 0–1.0, per-vertex AO for blocky; 1.0 for smooth)
   - Add `geometry_type: u32` (0 = smooth, 1 = blocky)

2. **Update vertex shader**
   - Pass new attributes through to fragment shader
   - Use `@interpolate(flat)` for geometry_type to avoid interpolation artifacts

3. **Prepare fragment shader for hybrid AO**
   - Add uniform/flag for AO mode (will be used when SSAO is added)
   - Structure lighting code to accept AO as a multiplier

4. **Update mesh registration**
   - Modify `register_indexed_mesh` to handle extended vertex layout
   - Update vertex buffer descriptor in pipeline setup

### Deliverables
- Extended `VoxelVertex` struct in mesh generation
- Updated WGSL vertex/fragment shaders with new attributes
- Pipeline vertex buffer layout changes

---

## Phase 3: Screen-Space Ambient Occlusion (GTAO)

**Goal:** Implement Ground Truth Ambient Occlusion for smooth geometry and additional detail on blocky geometry. 

SSAO is particularly valuable with modifiable terrain because it requires no recalculation when geometry changes—it's computed per-frame from the depth buffer.

### Tasks

1. **Add G-buffer or reuse existing buffers**
   - Depth buffer (already exists)
   - Normal buffer (may need separate pass or encode in existing output)

2. **Implement GTAO compute/fragment pass**
   - Sample depth buffer in screen space
   - Compute horizon-based occlusion using view-space positions
   - Output to AO texture

3. **Add spatial blur pass**
   - Bilateral blur to reduce noise while preserving edges
   - 4x4 or similar kernel

4. **Integrate into main lighting**
   - Sample AO texture in fragment shader
   - Blend with per-vertex AO based on geometry type:
     - Blocky: `final_ao = vertex_ao * ssao`
     - Smooth: `final_ao = ssao`

5. **Add quality settings**
   - Sample count (low/medium/high)
   - AO radius and intensity uniforms

### Deliverables
- GTAO shader pass (compute or fragment)
- Blur pass shader
- AO texture and sampler bindings
- Quality setting uniforms

---

## Phase 4: Percentage Closer Soft Shadows (PCSS)

**Goal:** Implement physically-based soft shadows where shadow softness varies automatically with distance from occluder to receiver.  Objects close to shadow casters produce hard shadows; distant shadows become naturally soft.

### Physical Basis

Real lights have physical size (area lights).  The penumbra (soft shadow region) grows with distance:

```
penumbra_width = light_size × (d_receiver - d_blocker) / d_blocker
```

Where:
- `light_size` = angular size of light source (artistic parameter)
- `d_receiver` = depth of surface being shaded
- `d_blocker` = depth of shadow-casting geometry

### Tasks

1. **Implement PCSS blocker search**
   - Sample shadow map in Poisson disk pattern around fragment
   - Configurable search radius based on light size
   - Compute average blocker depth from samples that contain occluders
   - Early-out when no blockers found (full light, skip PCF)

2.  **Implement penumbra estimation**
   - Add `light_size` uniform representing angular diameter of sun
   - Calculate penumbra width from depth difference ratio
   - Clamp to minimum (avoids aliasing) and maximum (performance limit)

3. **Implement variable-radius PCF**
   - Poisson disk sampling with radius scaled by penumbra estimate
   - Sample count scales with penumbra size:
     - Small penumbra → fewer samples (shadow is sharp anyway)
     - Large penumbra → more samples (need smooth gradient)
   - Rotate disk per-pixel using screen-space noise to reduce banding

4. **Add normal offset bias**
   - Offset shadow sample position along surface normal
   - Reduces peter-panning and acne on curved surfaces
   - Complements existing slope-scale bias

5.  **Implement cascade blending**
   - Detect fragments near cascade boundaries
   - Sample both cascades with PCSS independently
   - Blend results based on distance to boundary
   - Eliminates visible seams on smooth terrain

6. **Per-cascade quality scaling**
   - Cascade 0 (near): Full PCSS quality, highest sample counts
   - Cascade 1-2 (mid): Reduced blocker search samples
   - Cascade 3 (far): Simplified PCF acceptable (shadows less noticeable at distance)

7. **Quality presets**
   - Low: Fixed PCF 3x3 (no PCSS, for low-end hardware)
   - Medium: PCSS with 8 blocker + 16 PCF samples
   - High: PCSS with 16 blocker + 32 PCF samples
   - `light_size` exposed as artistic tuning parameter

### Algorithm Overview

```
PCSS Shadow Calculation
         │
         ▼
┌─────────────────────────┐
│ 1. Blocker Search       │ Sample shadow map in disk pattern
│    Find avg depth of    │ around current fragment
│    occluding geometry   │
└───────────┬─────────────┘
            │
            ▼ No blockers?  → Return 1.0 (full light)
            │
            ▼
┌─────────────────────────┐
│ 2.  Penumbra Estimation  │ penumbra = light_size ×
│    Calculate filter     │   (receiver_depth - blocker_depth)
│    radius from depths   │   / blocker_depth
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│ 3. Variable PCF         │ Sample shadow map with kernel
│    Filter with scaled   │ radius = penumbra estimate
│    kernel size          │
└─────────────────────────┘
```

### Visual Behavior

| Scenario | Shadow Appearance |
|----------|-------------------|
| Tree shadow on its own trunk | Hard (leaves close to trunk) |
| Tree shadow 10m from tree | Soft (leaves far from ground) |
| Character shadow at feet | Hard (body close to ground) |
| Character shadow of raised arm | Softer (arm farther from ground) |
| Building shadow at base | Hard |
| Building shadow across street | Very soft |
| Overhead sun, flat ground | Uniformly hard (parallel rays) |
| Low sun, long shadows | Soft at tips, hard near base |

### Deliverables
- PCSS blocker search function
- Penumbra estimation with `light_size` uniform
- Variable-radius PCF with Poisson disk sampling
- Normal offset bias implementation
- Cascade blending with per-cascade PCSS
- Quality preset system
- Per-cascade quality scaling

---

## Phase 5: Multiple Light Sources

**Goal:** Support N dynamic point/spot lights beyond sun and moon. 

### Tasks

1. **Design light data structure**
   - Position, color, intensity, range
   - Light type (point, spot, directional)
   - Spot lights: direction, inner/outer cone angles
   - Attenuation parameters
   - Light size (for PCSS-style soft shadows on point/spot lights)

2. **Create light storage buffer**
   - SSBO with array of light structs
   - Light count uniform
   - Maximum light limit (e.g., 64–128)

3. **Implement light accumulation loop**
   - Iterate over active lights in fragment shader
   - Accumulate diffuse and specular contributions
   - Apply attenuation based on distance

4. **Culling and optimization**
   - CPU-side frustum culling of lights
   - Optional: tile-based or clustered light culling for many lights

5. **Shadow support for additional lights (optional/future)**
   - Point lights: cubemap shadows with PCSS adaptation
   - Spot lights: single shadow map per light with PCSS
   - Consider limiting shadowed lights (e.g., 4 max)

### Deliverables
- `LightGpu` struct and light buffer
- Light manager on CPU side
- Fragment shader light loop
- Basic frustum culling

---

## Phase 6: Light Propagation

**Goal:** Implement Minecraft-style light spreading for torches, emissives, and cave lighting with incremental updates for terrain modification.

### Tasks

1. **Add light level to voxel data**
   - `light_level: u8` per block (0–15 scale)
   - Separate sky light and block light channels (optional)

2. **Implement initial flood-fill propagation**
   - BFS from light-emitting blocks
   - Decrease light level by 1 per block traveled
   - Used for initial world load and large regenerations

3. **Implement incremental light addition**
   - When light source placed: BFS outward from source
   - Only update blocks that would receive more light
   - Stop when existing light is brighter

4. **Implement light removal algorithm**
   - When light source removed or block placed in light path:
     - Phase 1: BFS to find all blocks lit by removed source
     - Phase 2: Clear light values in affected region
     - Phase 3: BFS re-flood from all adjacent light sources
   - More complex than addition but necessary for correctness

5. **Chunk boundary handling**
   - Propagate across chunk boundaries
   - Mark neighbor chunks dirty when edge light changes
   - Handle async: queue neighbor updates

6. **Shader sampling**
   - For blocky geometry: direct grid lookup
   - For smooth geometry: trilinear interpolation of light grid
   - Multiply with surface lighting

7. **Async light updates**
   - Large changes (explosions) affect many chunks
   - Process incrementally over multiple frames
   - Priority queue based on player distance

8. **Dirty region tracking**
   - Track bounding box of light changes per chunk
   - Only upload affected regions to GPU
   - Reduces bandwidth for small edits

### Deliverables
- Light level storage in voxel grid
- Initial flood-fill algorithm
- Incremental light addition algorithm
- Light removal algorithm (two-phase BFS)
- Trilinear light grid sampling in shader
- Async light update queue
- Dirty region tracking for GPU upload

### Light Update Performance Targets

| Operation | Target |
|-----------|--------|
| Single torch place | < 2ms |
| Single torch remove | < 5ms |
| Block place (shadows existing light) | < 3ms |
| Large edit (16³ blocks) | Spread over multiple frames |

---

## Phase 7: Volumetric Lighting

**Goal:** Add god rays and atmospheric light scattering. 

Volumetrics sample the shadow map each frame, so terrain modifications are automatically reflected. 

### Tasks

1. **Implement ray marching pass**
   - March from camera toward sun direction
   - Fixed step count (32–64 steps typical)
   - Accumulate "in-light" samples by querying shadow cascade

2. **Shadow cascade sampling during march**
   - Transform each march position to light space
   - Sample CSM (cascade 0 or select by depth)
   - Accumulate unshadowed samples
   - Note: Use simple shadow test here, not full PCSS (performance)

3. **Apply as screen-space effect**
   - Output to volumetric texture (half or quarter resolution)
   - Bilateral upscale to full resolution
   - Additive blend with final image

4. **Temporal filtering (optional)**
   - Jitter ray start position per frame
   - Accumulate over multiple frames
   - Reduces noise with fewer samples

5. **Quality settings**
   - Step count (16/32/64)
   - Resolution scale (1/4, 1/2, full)
   - Intensity and decay uniforms

### Deliverables
- Volumetric ray march shader
- Half-res volumetric texture
- Upscale and composite pass
- Quality and intensity controls

---

## Phase 8: Environment Effects

**Goal:** Integrate weather, fog, and atmospheric conditions with lighting.

### Tasks

1. **Weather state system**
   - Define states: Clear, Cloudy, Overcast, Rain, Storm
   - Transition smoothing between states

2. **Weather affects lighting**
   - Modulate `sun_intensity` (reduce in clouds/rain)
   - Modulate `ambient` color and intensity
   - Increase `light_size` in overcast (larger apparent light source = softer shadows)

3. **Distance fog**
   - Exponential or exponential-squared fog
   - Fog density tied to weather state
   - Fog color from sky/ambient

4. **Sky rendering updates**
   - Cloud coverage affects sky gradient
   - Storm darkening
   - Extend existing time-of-day with weather

5. **Rain/snow particle effects (optional)**
   - Particle system for precipitation
   - Affected by lighting

### Deliverables
- Weather state enum and transition system
- Weather-to-lighting parameter mapping (including `light_size`)
- Fog uniforms and shader integration
- Extended skybox shader

---

## Phase 9: Polish & Optimization

**Goal:** Performance tuning, quality presets, and final integration with focus on modification responsiveness.

### Tasks

1. **Profile and optimize**
   - GPU timing for each pass (especially PCSS blocker search)
   - CPU timing for mesh generation and light propagation
   - Identify bottlenecks per operation type

2. **PCSS optimization**
   - Early-out paths when no blockers found
   - Adaptive sample counts based on penumbra size
   - Consider temporal stability (cache blocker search results)

3. **Modification performance budget**
   - Target: No frame drop for single block edit
   - Target: < 100ms total for chunk rebuild (async)
   - Target: Smooth light updates without popping

4. **Memory management**
   - GPU buffer pooling for mesh double-buffering
   - Old mesh cleanup after swap
   - Light grid memory optimization

5. **LOD considerations**
   - Reduce PCSS quality for distant cascades
   - Reduce AO sample count at distance
   - Consider Transvoxel for smooth terrain LOD

6. **Quality presets**
   - Low: No SSAO, fixed PCF (no PCSS), no volumetrics
   - Medium: SSAO low, PCSS medium, volumetrics half-res
   - High: GTAO full, PCSS high, volumetrics full

7. **Settings UI integration**
   - Expose quality options in settings menu
   - `light_size` as advanced/artistic option
   - Runtime switching without restart

8. **Debug visualization modes**
   - Cascade visualization (already exists)
   - AO buffer view
   - Light level heat map
   - Penumbra size visualization (color-code by softness)
   - Chunk state visualization (dirty/generating/ready)

### Deliverables
- Performance profiling data and benchmarks
- PCSS optimization passes
- Memory pooling system
- Quality preset system
- Settings UI integration
- Debug visualization shaders

---

## Appendix A: Technical Reference

### Geometry Type Handling Summary

| Aspect | Smooth Terrain | Blocky Structures |
|--------|----------------|-------------------|
| Mesh generation | Marching Cubes | Greedy meshing |
| Normals | Density field gradient | Face normals |
| Per-vertex AO | 1. 0 (deferred to SSAO) | 4-corner neighbor lookup |
| Geometry type flag | 0 | 1 |
| Light grid sampling | Trilinear interpolation | Direct lookup |
| Modification rebuild | Always async | Sync for single block, async for chunk |

### Shadow Pipeline Summary (with PCSS)

```
Sun Direction + Light Size
         │
         ▼
┌─────────────────┐
│ Calculate CSM   │ ◄── 4 cascade matrices based on camera position
│ Matrices        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Shadow Passes   │ ◄── Render depth to each cascade layer
│ (4 cascades)    │     (automatically includes modified geometry)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Fragment Shader │
│                 │
│ ┌─────────────┐ │
│ │ 1.  Blocker  │ │ ◄── Search for occluder depths
│ │    Search   │ │
│ └──────┬──────┘ │
│        ▼        │
│ ┌─────────────┐ │
│ │ 2. Penumbra │ │ ◄── Estimate shadow softness from distance
│ │    Estimate │ │
│ └──────┬──────┘ │
│        ▼        │
│ ┌─────────────┐ │
│ │ 3. Variable │ │ ◄── PCF with dynamic kernel size
│ │    PCF      │ │
│ └─────────────┘ │
└─────────────────┘
```

### PCSS Parameters

| Parameter | Description | Typical Range |
|-----------|-------------|---------------|
| `light_size` | Angular diameter of sun | 0.01 – 0.1 |
| `search_radius` | Blocker search area | 5 – 20 texels |
| `min_penumbra` | Minimum filter radius | 1 texel |
| `max_penumbra` | Maximum filter radius | 20 – 40 texels |
| `blocker_samples` | Samples for blocker search | 8 – 16 |
| `pcf_samples` | Samples for filtering | 16 – 64 |

### Lighting Accumulation Order

1.  Ambient × AO (SSAO blended with per-vertex)
2. Sun diffuse × PCSS shadow
3. Sun specular × PCSS shadow
4. Moon diffuse (no shadow or simplified)
5. Moon specular (no shadow or simplified)
6. Point/spot light loop (with optional per-light soft shadows)
7. Emissive/light propagation contribution
8. Volumetric additive blend
9.  Fog application

### Modification Event Flow

```
Block Changed
     │
     ├──► Dirty chunk mesh flags
     │         │
     │         ├──► Smooth?  Queue async rebuild
     │         └──► Blocky single block? Sync or async rebuild
     │
     ├──► Dirty neighbor chunks (if edge block)
     │
     ├──► Queue light propagation update
     │         │
     │         ├──► Light added?  Incremental flood
     │         └──► Light removed/blocked? Two-phase removal + re-flood
     │
     └──► Shadow maps update automatically next frame
```

### Chunk State Machine

```
     ┌─────────┐
     │  Idle   │◄─────────────────────────────┐
     └────┬────┘                              │
          │ Block modified                    │
          ▼                                   │
     ┌─────────┐                              │
     │  Dirty  │                              │
     └────┬────┘                              │
          │ Job scheduled                     │
          ▼                                   │
     ┌─────────┐                              │
     │ Queued  │──── Cancelled ───────────────┤
     └────┬────┘     (player moved away)      │
          │ Worker picks up                   │
          ▼                                   │
     ┌────────────┐                           │
     │ Generating │──── Cancelled ────────────┤
     └─────┬──────┘                           │
           │ Complete                         │
           ▼                                  │
     ┌─────────┐                              │
     │  Ready  │──── Swap mesh, return to ────┘
     └─────────┘     Idle
```

### Light Propagation: Removal Algorithm

```
Light Source Removed (or block placed in light path)
     │
     ▼
Phase 1: Identify affected blocks
     │
     ├──► BFS from removal point
     ├──► Collect blocks whose light came from this source
     └──► Stop at blocks with different/stronger light source
     │
     ▼
Phase 2: Clear light values
     │
     └──► Set light_level = 0 for all collected blocks
     │
     ▼
Phase 3: Re-flood from neighbors
     │
     ├──► Find all light sources adjacent to cleared region
     └──► BFS flood-fill from each source
           (standard addition algorithm)
```

### File Locations (Current)

| Component | Path |
|-----------|------|
| Fragment shader | `shaders/fragment.wgsl` |
| Skybox shader | `shaders/skybox.wgsl` |
| Voxel chunk | `moho_core/src/voxel/chunk.rs` |
| Chunk state | `moho_core/src/voxel/state.rs` |
| Block modifier | `moho_core/src/voxel/modification.rs` |
| Mesh job queue | `moho_core/src/voxel/jobs.rs` |
| Mesh generation | `moho_core/src/voxel/mesh.rs` |
| World events | `moho_core/src/events/types/world.rs` |
| Buffer manager | `moho_renderer/src/buffer_manager.rs` |
| Renderer | `moho_renderer/src/lib.rs` |
| Shadow system | `moho_renderer/src/shadow.rs` |
| GPU types | `moho_renderer/src/gpu_types.rs` |

---

## Appendix B: Performance Budget

### Per-Frame Budget (60 FPS = 16. 6ms)

| System | Budget | Notes |
|--------|--------|-------|
| Shadow passes (4 cascades) | 2–3ms | Already implemented |
| Main render pass | 4–6ms | Geometry dependent |
| PCSS overhead vs fixed PCF | +1–2ms | Blocker search cost |
| SSAO + blur | 1–2ms | Quarter-res helps |
| Volumetrics | 1–2ms | Half-res, 32 steps |
| Light grid upload | < 0.5ms | Dirty regions only |
| Mesh swaps | < 0.5ms | Pre-uploaded buffers |
| **Total rendering** | ~11–16ms | Tight but achievable |

### PCSS Sample Budget by Quality

| Quality | Blocker Samples | PCF Samples | Cascades with Full PCSS |
|---------|-----------------|-------------|-------------------------|
| Low | 0 (fixed PCF) | 9 | None |
| Medium | 8 | 16 | 0–1 |
| High | 16 | 32 | 0–2 |
| Ultra | 24 | 48 | 0–3 |

### Per-Modification Budget

| Operation | Sync Budget | Async Budget |
|-----------|-------------|--------------|
| Single blocky block | < 5ms | N/A |
| Blocky chunk rebuild | N/A | < 50ms |
| Single smooth block | N/A | < 100ms |
| Smooth chunk rebuild | N/A | < 200ms |
| Light update (local) | < 3ms | N/A |
| Light update (large) | N/A | Spread over frames |

---

## Appendix C: PCSS Tuning Guide

### Light Size Effects

| `light_size` Value | Visual Effect | Use Case |
|--------------------|---------------|----------|
| 0.01 | Nearly hard shadows | Harsh midday sun |
| 0.03 | Subtle softening at distance | Clear day default |
| 0.05 | Noticeable soft shadows | Slightly hazy |
| 0. 08 | Very soft distant shadows | Overcast starting |
| 0.10+ | Extremely soft | Heavy overcast |

### Weather Integration

| Weather State | Suggested `light_size` | Sun Intensity |
|---------------|------------------------|---------------|
| Clear | 0.02–0.03 | 1.0 |
| Partly Cloudy | 0.04–0.05 | 0. 8 |
| Overcast | 0.08–0.10 | 0.4 |
| Rain | 0.10–0.15 | 0.3 |
| Storm | 0.15+ | 0.2 |

### Common Issues and Solutions

| Issue | Cause | Solution |
|-------|-------|----------|
| Banding in soft shadows | Too few PCF samples | Increase samples or add noise rotation |
| Performance drop in shadowed areas | Full PCSS on every fragment | Early-out when no blockers, reduce far cascade quality |
| Shadows too soft everywhere | `light_size` too high | Reduce value, check penumbra clamp |
| Shadows too hard | `light_size` too low or `max_penumbra` clamped | Increase values |
| Flickering shadow edges | Temporal instability | Add temporal filtering or jittered sampling |

---

## Appendix D: Future Considerations

### Not in Current Scope (But Worth Noting)

1. **Global Illumination**
   - Light probe grids for indirect lighting
   - Would enhance cave/interior lighting
   - Significant complexity increase

2. **Reflections**
   - Screen-space reflections (SSR)
   - Reflection probes for water/metal
   - Adds to smooth surface quality

3. **Mesh LOD for Smooth Terrain**
   - Transvoxel algorithm for seamless LOD
   - Critical for large view distances
   - Interacts with modification system

4. **Multiplayer Synchronization**
   - Block change replication
   - Light state sync vs.  recompute
   - Prediction for responsive feel

5. **Persistent Light Storage**
   - Save/load light grid with world
   - Faster world load (skip full propagation)

6. **Area Light Shadows**
   - True area light shadow calculation
   - More accurate than PCSS approximation
   - Significantly more expensive

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2025-11-27 | Initial plan with hybrid geometry support |
| 2.0 | 2025-11-27 | Added Phase 0 for modification infrastructure, incremental light propagation, async mesh generation, performance budgets |
| 3.0 | 2025-11-27 | Replaced hard/soft shadow toggle with PCSS for distance-based shadow softness, added PCSS tuning guide, weather integration with light_size |
| 3.1 | 2025-11-27 | Phase 0 complete: ChunkState, BlockModifier, MeshJobQueue, double-buffering, world events implemented and tested |
| 3.2 | 2025-11-27 | Phase 1 complete: BlockCategory, Marching Cubes, BlockyMeshGenerator with per-vertex AO, HybridMeshGenerator, async job queue integration, 85 tests passing |
| 3.3 | 2025-11-27 | Phase 1 visual testing complete: Fixed Marching Cubes triangle indexing bug, smooth terrain rendering correctly with proper surface continuity, hybrid system functional |