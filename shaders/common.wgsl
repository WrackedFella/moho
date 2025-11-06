// Common definitions shared by vertex and fragment shaders
struct Camera {
    vp0: vec4<f32>,
    vp1: vec4<f32>,
    vp2: vec4<f32>,
    vp3: vec4<f32>,
    cam_pos: vec4<f32>,
}

struct Lighting {
    sun_direction: vec4<f32>,  // xyz = direction (normalized), w = intensity
    sun_color: vec4<f32>,      // xyz = color, w = unused
    moon_direction: vec4<f32>, // xyz = direction (normalized), w = intensity
    moon_color: vec4<f32>,     // xyz = color, w = unused
    ambient: vec4<f32>,        // xyz = color, w = intensity
    time_of_day: vec4<f32>,    // x = 0-24 hours, yzw = unused
}

struct ShadowMatrix {
    sm0: vec4<f32>,
    sm1: vec4<f32>,
    sm2: vec4<f32>,
    sm3: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var<storage, read> materials: array<Material>;

@group(0) @binding(2)
var<uniform> lighting: Lighting;

// Shadow mapping resources (group 1)
@group(1) @binding(0)
var<uniform> shadow_matrix: ShadowMatrix;

@group(1) @binding(1)
var shadow_map: texture_depth_2d_array; // Phase 4: Array texture for all 4 cascades

@group(1) @binding(2)
var shadow_sampler: sampler_comparison;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct InstanceIn {
    @location(2) model_col0: vec4<f32>,
    @location(3) model_col1: vec4<f32>,
    @location(4) model_col2: vec4<f32>,
    @location(5) model_col3: vec4<f32>,
    @location(6) material: u32,
    @location(7) object_type: u32,
}

struct Material {
    albedo: vec4<f32>, // .xyz = albedo, .w unused
    params: vec4<f32>, // params.x = fuzz, params.y = ref_idx
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) material: u32,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) light_space_pos: vec4<f32>,
}
