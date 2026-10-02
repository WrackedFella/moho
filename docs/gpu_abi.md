# GPU ABI (WGSL <-> Rust)

This document lists the GPU-visible struct layouts and how they map between WGSL
and Rust. Keeping these in sync is critical: WGSL storage and uniform buffer
alignment rules are strict (vec4 alignment).

## `Material` / `MaterialGpu`
WGSL:

```wgsl
struct Material {
    albedo: vec4<f32>, // .xyz = albedo, .w unused
    params: vec4<f32>, // params.x = fuzz, params.y = ref_idx
};
```

Rust:

```rust
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy)]
pub struct MaterialGpu {
    pub albedo: [f32; 4],
    pub params: [f32; 4],
}
```

Notes: each material occupies 32 bytes (two vec4s). `bytemuck` guarantees allow
safe `cast_slice` usage when uploading slices of `MaterialGpu` to GPU buffers.

## `Camera` / `CameraGpu`
WGSL expects a 5-vec4 uniform (4x4 view-proj columns + cam_pos):

```wgsl
struct Camera {
    vp0: vec4<f32>;
    vp1: vec4<f32>;
    vp2: vec4<f32>;
    vp3: vec4<f32>;
    cam_pos: vec4<f32>;
}
```

Rust:

```rust
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy)]
pub struct CameraGpu {
    pub vp0: [f32; 4],
    pub vp1: [f32; 4],
    pub vp2: [f32; 4],
    pub vp3: [f32; 4],
    pub cam_pos: [f32; 4],
}
```

Size: 80 bytes (5 vec4s).

## `Lighting` / `LightingGpu`
WGSL expects lighting data for sun, moon, ambient, and time:

```wgsl
struct Lighting {
    sun_direction: vec4<f32>,  // xyz = direction (normalized), w = intensity
    sun_color: vec4<f32>,      // xyz = color, w = unused
    moon_direction: vec4<f32>, // xyz = direction (normalized), w = intensity
    moon_color: vec4<f32>,     // xyz = color, w = unused
    ambient: vec4<f32>,        // xyz = color, w = intensity
    time_of_day: vec4<f32>,    // x = 0-24 hours, yzw = unused
}
```

Rust:

```rust
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy)]
pub struct LightingGpu {
    pub sun_direction: [f32; 4],
    pub sun_color: [f32; 4],
    pub moon_direction: [f32; 4],
    pub moon_color: [f32; 4],
    pub ambient: [f32; 4],
    pub time_of_day: [f32; 4],
}
```

Size: 96 bytes (6 vec4s).

Notes: 
- Sun and moon intensities are stored in the `.w` component of direction vectors
- Sun/moon provide directional lighting (sun casts shadows, moon does not)
- time_of_day.x ranges from 0.0-24.0 for shader-based sky color calculations
- Ambient lighting adjusts dynamically based on time of day

## `CascadedShadowMatrix` / `CascadedShadowMatrixGpu`
WGSL expects CSM data with 4 cascade transforms and split distances:

```wgsl
struct CascadedShadowMatrix {
    cascade0_m0: vec4<f32>,
    cascade0_m1: vec4<f32>,
    cascade0_m2: vec4<f32>,
    cascade0_m3: vec4<f32>,
    // ... cascade1, cascade2, cascade3 (same pattern)
    split_distances: vec4<f32>,  // xyz = cascade 0-2 far planes, w = cascade 3 far
}
```

Rust:

```rust
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy)]
pub struct CascadedShadowMatrixGpu {
    pub cascade0_m0: [f32; 4],
    pub cascade0_m1: [f32; 4],
    pub cascade0_m2: [f32; 4],
    pub cascade0_m3: [f32; 4],
    pub cascade1_m0: [f32; 4],
    pub cascade1_m1: [f32; 4],
    pub cascade1_m2: [f32; 4],
    pub cascade1_m3: [f32; 4],
    pub cascade2_m0: [f32; 4],
    pub cascade2_m1: [f32; 4],
    pub cascade2_m2: [f32; 4],
    pub cascade2_m3: [f32; 4],
    pub cascade3_m0: [f32; 4],
    pub cascade3_m1: [f32; 4],
    pub cascade3_m2: [f32; 4],
    pub cascade3_m3: [f32; 4],
    pub split_distances: [f32; 4],
}
```

Size: 272 bytes (4 matrices × 64 bytes + 16 bytes for splits).

Notes:
- 4 cascades cover view ranges: [0-50], [50-150], [150-400], [400-800] units
- Each cascade is a 4×4 light-space transform matrix
- Shadow maps are 4096×4096 per cascade (Depth32Float texture array)

## Tips
- Always change Rust types first and run `cargo test` to catch size/layout issues.
- Use `bytemuck::cast_slice` for safe transmutation of GPU data to `&[u8]`.
- Keep `shaders/*.wgsl` and `moho_renderer::gpu_types` in sync when adding fields.
- Buffer sizes must match struct sizes: use `std::mem::size_of::<T>()` to verify.

