// Common definitions shared by vertex and fragment shaders
struct Camera {
    vp0: vec4<f32>,
    vp1: vec4<f32>,
    vp2: vec4<f32>,
    vp3: vec4<f32>,
    cam_pos: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

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

@group(0) @binding(1)
var<storage, read> materials: array<Material>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) material: u32,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
}
