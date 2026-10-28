// Common definitions shared by vertex and fragment shaders
struct Camera {
    vp0: vec4<f32>,
    vp1: vec4<f32>,
    vp2: vec4<f32>,
    vp3: vec4<f32>,
    cam_pos: vec4<f32>,  // xyz = camera position, w = view-height cut (world y above it is discarded)
}

struct Lighting {
    sun_direction: vec4<f32>,  // xyz = direction (normalized), w = intensity
    sun_color: vec4<f32>,      // xyz = color, w = unused
    moon_direction: vec4<f32>, // xyz = direction (normalized), w = intensity
    moon_color: vec4<f32>,     // xyz = color, w = unused
    ambient: vec4<f32>,        // w = sky-ambient strength, xyz = unused
    params: vec4<f32>,         // x = time of day, y = debug mode, zw = unused
    sky_zenith: vec4<f32>,     // xyz = colour, w = unused
    sky_horizon: vec4<f32>,    // xyz = colour, w = unused
    sky_ground: vec4<f32>,     // xyz = colour, w = unused
}

// Cascaded shadow block (496 bytes): sun layers 0-3, moon layers 4-5. Mirrors `ShadowGpu`.
struct Shadow {
    // Light view-projection per layer
    matrices: array<mat4x4<f32>, 6>,
    // Far end of each cascade by view depth: sun in [0], moon in [1].xy
    split_far: array<vec4<f32>, 2>,
    // World size of one texel per cascade, packed like split_far
    texel_world: array<vec4<f32>, 2>,
    // xyz = camera forward, w = cascade blend fraction
    camera_forward: vec4<f32>,
    // x = sun, y = moon
    light_intensities: vec4<f32>,
    // x = effective shadow distance, y = PCSS light size, z = PCSS quality (0=off, 1=low, 2=med, 3=high), w = view-height cut
    metadata: vec4<f32>,
}

// Point light structure (32 bytes)
struct PointLight {
    position_range: vec4<f32>,    // xyz = position, w = range
    color_intensity: vec4<f32>,   // xyz = color, w = intensity
}

// Dynamic lights buffer (max 64 lights)
struct DynamicLights {
    light_count: vec4<u32>,       // x = count, yzw = padding (16 bytes for alignment)
    lights: array<PointLight, 64>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var<storage, read> materials: array<Material>;

@group(0) @binding(2)
var<uniform> lighting: Lighting;

@group(0) @binding(3)
var ssao_texture: texture_2d<f32>;

@group(0) @binding(4)
var ssao_sampler: sampler;

@group(0) @binding(5)
var<storage, read> dynamic_lights: DynamicLights;

// Shadow mapping resources (group 1)
@group(1) @binding(0)
var<uniform> shadow_data: Shadow;

@group(1) @binding(1)
var shadow_map: texture_depth_2d_array; // One layer per cascade

@group(1) @binding(2)
var shadow_sampler: sampler_comparison;

@group(1) @binding(3)
var shadow_sampler_nearest: sampler; // For PCSS blocker search (depth reads)

// Base-colour texture of the draw (group 2); untextured draws bind a 1x1 white texture.
@group(2) @binding(0)
var base_colour_texture: texture_2d<f32>;

@group(2) @binding(1)
var base_colour_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) ao: f32,
    @location(3) geometry_type: u32,
    @location(10) light_level: f32,
    @location(11) block_light_rgb: vec3<f32>,
    @location(12) sky_exposed: f32,
    @location(13) material: u32,
    @location(14) uv: vec2<f32>,
}

struct InstanceIn {
    @location(4) model_col0: vec4<f32>,
    @location(5) model_col1: vec4<f32>,
    @location(6) model_col2: vec4<f32>,
    @location(7) model_col3: vec4<f32>,
    @location(8) material: u32,
    @location(9) object_type: u32,
}

struct Material {
    albedo: vec4<f32>, // .xyz = albedo or top-face color, .w unused
    // .xyz = side-face color (VoxelTerrain only), else [fuzz, ref_idx, is_transparent]
    // .w   = emissive intensity (> 0 → bypass lighting)
    params: vec4<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) material: u32,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(4) ao: f32,
    @location(5) @interpolate(flat) geometry_type: u32,
    @location(6) light_level: f32,
    @location(7) @interpolate(flat) object_type: u32,
    @location(8) block_light_rgb: vec3<f32>,
    @location(9) sky_exposed: f32,
    @location(10) uv: vec2<f32>,
}
