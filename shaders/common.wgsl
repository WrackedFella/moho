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

// Multi-light shadow matrix buffer (288 bytes total for 4 lights)
struct MultiLightShadowMatrix {
    // Light 0 matrix (Sun)
    light0_m0: vec4<f32>,
    light0_m1: vec4<f32>,
    light0_m2: vec4<f32>,
    light0_m3: vec4<f32>,
    // Light 1 matrix (Moon)
    light1_m0: vec4<f32>,
    light1_m1: vec4<f32>,
    light1_m2: vec4<f32>,
    light1_m3: vec4<f32>,
    // Light 2 matrix (Dynamic 1)
    light2_m0: vec4<f32>,
    light2_m1: vec4<f32>,
    light2_m2: vec4<f32>,
    light2_m3: vec4<f32>,
    // Light 3 matrix (Dynamic 2)
    light3_m0: vec4<f32>,
    light3_m1: vec4<f32>,
    light3_m2: vec4<f32>,
    light3_m3: vec4<f32>,
    // Light intensities (x=Sun, y=Moon, z=Dynamic1, w=Dynamic2)
    light_intensities: vec4<f32>,
    // Metadata (x=active_count, yzw=unused)
    metadata: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var<storage, read> materials: array<Material>;

@group(0) @binding(2)
var<uniform> lighting: Lighting;

// Shadow mapping resources (group 1)
@group(1) @binding(0)
var<uniform> shadow_matrices: MultiLightShadowMatrix;

@group(1) @binding(1)
var shadow_map: texture_depth_2d_array; // Array texture for all 4 lights

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
