// Ground Truth Ambient Occlusion (GTAO) compute shader
//
// This shader implements GTAO for computing ambient occlusion in screen space.
// It samples the depth buffer to compute horizon angles and estimate occlusion.
//
// References:
// - "Practical Real-Time Strategies for Accurate Indirect Occlusion"
//   by Jorge Jimenez et al. (SIGGRAPH 2016)
// - "Ground Truth Ambient Occlusion" by XeGTAO

// SSAO settings uniform
struct SsaoSettings {
    sample_count: u32,
    radius: f32,
    intensity: f32,
    bias: f32,
}

// Camera matrices for reconstructing view-space positions
struct CameraUniforms {
    inv_proj: mat4x4<f32>,   // Inverse projection for reconstructing view-space position from depth
}

@group(0) @binding(0) var depth_texture: texture_depth_2d;
@group(0) @binding(1) var depth_sampler: sampler;
@group(0) @binding(2) var ao_output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> settings: SsaoSettings;
@group(0) @binding(4) var<uniform> camera: CameraUniforms;

// Reconstruct view-space position from depth
fn reconstruct_view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    // Convert UV and depth to clip space
    let clip_space = vec4<f32>(
        uv.x * 2.0 - 1.0,
        (1.0 - uv.y) * 2.0 - 1.0,  // Flip Y for correct coordinate system
        depth,
        1.0
    );
    
    // Transform to view space
    let view_space = camera.inv_proj * clip_space;
    return view_space.xyz / view_space.w;
}

// Simple pseudo-random noise function
fn noise(coord: vec2<f32>) -> f32 {
    return fract(sin(dot(coord, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

// Poisson disk sampling pattern for GTAO
const POISSON_DISK: array<vec2<f32>, 16> = array<vec2<f32>, 16>(
    vec2<f32>( 0.0,  0.0),
    vec2<f32>( 0.53812504,  0.18565957),
    vec2<f32>(-0.25,  0.43301270),
    vec2<f32>(-0.96486687, -0.25881905),
    vec2<f32>( 0.70710678,  0.70710678),
    vec2<f32>(-0.70710678,  0.70710678),
    vec2<f32>(-0.70710678, -0.70710678),
    vec2<f32>( 0.70710678, -0.70710678),
    vec2<f32>( 0.92387953,  0.38268343),
    vec2<f32>(-0.38268343,  0.92387953),
    vec2<f32>(-0.92387953, -0.38268343),
    vec2<f32>( 0.38268343, -0.92387953),
    vec2<f32>( 0.0,  1.0),
    vec2<f32>( 1.0,  0.0),
    vec2<f32>( 0.0, -1.0),
    vec2<f32>(-1.0,  0.0)
);

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let texture_dims = textureDimensions(depth_texture);
    let pixel_coord = vec2<i32>(global_id.xy);
    
    // Bounds check
    if (pixel_coord.x >= i32(texture_dims.x) || pixel_coord.y >= i32(texture_dims.y)) {
        return;
    }
    
    let uv = vec2<f32>(global_id.xy) / vec2<f32>(texture_dims);
    
    // Sample center depth using textureLoad (compute shader compatible)
    let center_depth = textureLoad(depth_texture, pixel_coord, 0);
    
    // Skip skybox/far plane
    if (center_depth >= 0.9999) {
        textureStore(ao_output, pixel_coord, vec4<f32>(1.0));
        return;
    }
    
    // Reconstruct center position in view space
    let center_pos = reconstruct_view_position(uv, center_depth);
    
    // Simple normal estimation from depth derivatives
    let texel_size = vec2<f32>(1.0) / vec2<f32>(texture_dims);
    let depth_right = textureLoad(depth_texture, pixel_coord + vec2<i32>(1, 0), 0);
    let depth_up = textureLoad(depth_texture, pixel_coord + vec2<i32>(0, 1), 0);
    
    let pos_right = reconstruct_view_position(uv + vec2<f32>(texel_size.x, 0.0), depth_right);
    let pos_up = reconstruct_view_position(uv + vec2<f32>(0.0, texel_size.y), depth_up);
    
    let normal = normalize(cross(pos_right - center_pos, pos_up - center_pos));
    
    // Compute random rotation angle for sampling
    let random_angle = noise(vec2<f32>(pixel_coord)) * 6.28318530718; // 2*PI
    let sin_angle = sin(random_angle);
    let cos_angle = cos(random_angle);
    
    // Accumulate occlusion
    var occlusion = 0.0;
    let sample_count_f = f32(settings.sample_count);
    let radius_pixels = settings.radius;
    
    for (var i = 0u; i < settings.sample_count; i++) {
        // Get sample direction and rotate it
        let sample_dir = POISSON_DISK[i % 16u];
        let rotated_dir = vec2<f32>(
            sample_dir.x * cos_angle - sample_dir.y * sin_angle,
            sample_dir.x * sin_angle + sample_dir.y * cos_angle
        );
        
        // Scale by radius (in pixels)
        let sample_offset = rotated_dir * radius_pixels * texel_size;
        let sample_uv = uv + sample_offset;
        
        // Bounds check for sample
        if (sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0) {
            continue;
        }
        
        // Convert UV to pixel coordinates for textureLoad
        let sample_pixel = vec2<i32>(sample_uv * vec2<f32>(texture_dims));
        let sample_depth = textureLoad(depth_texture, sample_pixel, 0);
        let sample_pos = reconstruct_view_position(sample_uv, sample_depth);
        
        // Compute occlusion contribution
        let diff = sample_pos - center_pos;
        let distance_sq = dot(diff, diff);
        let normal_dot = max(0.0, dot(normal, normalize(diff)));
        
        // Range check and accumulate
        let range_check = smoothstep(0.0, 1.0, settings.radius / sqrt(distance_sq));
        occlusion += (normal_dot - settings.bias) * range_check;
    }
    
    // Average and apply intensity
    occlusion = occlusion / sample_count_f;
    let ao = 1.0 - saturate(occlusion * settings.intensity);
    
    // Write to output
    textureStore(ao_output, pixel_coord, vec4<f32>(ao));
}
