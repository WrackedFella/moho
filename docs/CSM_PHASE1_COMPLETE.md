# CSM Phase 1 Complete: GPU Resources & Data Structures

**Status:** ✅ Complete  
**Date:** October 31, 2025  
**Branch:** rendering-improvements

## Objective
Establish GPU resources and data structures for Cascaded Shadow Maps (CSM) while keeping existing single shadow map working.

## Changes Implemented

### 1. Constants and Configuration (`moho_renderer/src/lib.rs`)
```rust
const NUM_SHADOW_CASCADES: u32 = 4;
const SHADOW_MAP_SIZE: u32 = 4096;
const CASCADE_SPLIT_DISTANCES: [f32; 4] = [20.0, 50.0, 100.0, 200.0];
const CSM_DEBUG_MODE: bool = false;
const CSM_VERBOSE_LOGGING: bool = false;
```
- 4 cascades (industry standard)
- Option B moderate split distances: 20, 50, 100, 200 units
- 4096x4096 resolution per cascade
- Debug flags for visualization and logging

### 2. GPU Data Structure (`moho_renderer/src/gpu_types.rs`)
```rust
pub struct CascadedShadowMatrixGpu {
    // 4 matrices (4x4 each) for cascade transforms
    pub cascade0_m0..m3: [f32; 4],
    pub cascade1_m0..m3: [f32; 4],
    pub cascade2_m0..m3: [f32; 4],
    pub cascade3_m0..m3: [f32; 4],
    // Split distances for cascade selection
    pub split_distances: [f32; 4],
}
```
- Size: 272 bytes (4×64 + 16)
- Pod + Zeroable for GPU buffer compatibility
- Default implementation with identity matrices

### 3. CSM Texture Array Resources
Created alongside existing single shadow map:
- **CSM Texture Array:** 4096×4096×4 layers (Depth32Float)
- **Cascade Views:** 4 individual D2 views (one per cascade for rendering)
- **Array View:** Single D2Array view (for shader sampling)

### 4. Renderer State
Added to `Renderer` struct:
```rust
csm_texture: wgpu::Texture,
csm_cascade_views: Vec<wgpu::TextureView>,
csm_array_view: wgpu::TextureView,
```

## Verification

### Build Status
✅ Compiles successfully with no errors  
⚠️ Expected warnings for unused fields (Phase 2+ will use them)

### Runtime Verification
Enabled `CSM_VERBOSE_LOGGING = true` temporarily and confirmed:
```
[INFO moho_renderer::gfx::wgpu_impl] Created CSM texture array: 4096x4096 with 4 layers
[INFO moho_renderer::gfx::wgpu_impl] Created 4 cascade views + 1 array view for CSM sampling
```

### Existing Functionality
✅ Application runs normally  
✅ Current single shadow map still working  
✅ No visual changes (expected - CSM not yet active)

## Next Steps: Phase 2

**Cascade Frustum Calculation (3-4 SP)**
- Implement `calculate_cascade_frustums()` for view frustum slicing
- Implement `calculate_cascade_matrices()` for tight orthographic projections
- Add detailed logging of cascade bounds
- Verification: Console shows 4 cascades with sensible bounds

## Notes
- Split distances (20, 50, 100, 200) chosen for moderate coverage
- Can be adjusted after voxel chunk size is finalized
- Matches terrain scale: 128×128 units with camera ~24 units high
- Legacy single shadow map preserved during transition
