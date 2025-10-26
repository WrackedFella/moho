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

## Tips
- Always change Rust types first and run `cargo test` to catch size/layout issues.
- Use `bytemuck::cast_slice` for safe transmutation of `&[MaterialGpu]` to `&[u8]`.
- Keep `shaders/*.wgsl` and `moho_renderer::gpu_types` in sync when adding fields.

