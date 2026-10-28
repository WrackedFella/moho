// Shadow mapping shader - renders scene from a cascade's light view
// Outputs depth values to one layer of the shadow map array

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
    // x = effective shadow distance, y = PCSS light size, z = PCSS quality, w = view-height cut (world y above it is discarded)
    metadata: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> shadow_data: Shadow;

// Immediate data for the layer being rendered (wgpu 29 renamed push constants to "immediates")
struct PushConstants {
    layer: u32,
}

var<immediate> pc: PushConstants;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct InstanceInput {
    @location(2) model_col0: vec4<f32>,
    @location(3) model_col1: vec4<f32>,
    @location(4) model_col2: vec4<f32>,
    @location(5) model_col3: vec4<f32>,
    @location(6) material: u32,
    @location(7) object_type: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // World y relative to the view-height cut; positive means above it.
    @location(0) height_over_cut: f32,
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Transform vertex position by model matrix
    let p = vec4<f32>(vertex.position, 1.0);
    let row0 = vec4<f32>(instance.model_col0.x, instance.model_col1.x, instance.model_col2.x, instance.model_col3.x);
    let row1 = vec4<f32>(instance.model_col0.y, instance.model_col1.y, instance.model_col2.y, instance.model_col3.y);
    let row2 = vec4<f32>(instance.model_col0.z, instance.model_col1.z, instance.model_col2.z, instance.model_col3.z);
    let row3 = vec4<f32>(instance.model_col0.w, instance.model_col1.w, instance.model_col2.w, instance.model_col3.w);
    let world_pos = vec4<f32>(dot(row0, p), dot(row1, p), dot(row2, p), dot(row3, p));
    
    // Transform to the layer's light space
    out.clip_position = shadow_data.matrices[pc.layer] * world_pos;

    out.height_over_cut = world_pos.y - shadow_data.metadata.w;

    return out;
}

// Depth-only pass: the fragment stage exists only to discard what the main
// pass discards, so shadows come from drawn geometry alone. It is a varying
// rather than a uniform read so the shadow bind group stays vertex-only.
@fragment
fn fs_main(in: VertexOutput) {
    if (in.height_over_cut > 0.0) {
        discard;
    }
}
