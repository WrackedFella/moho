// Bilateral blur for SSAO
//
// This shader applies a bilateral blur to the raw SSAO output to reduce noise
// while preserving edges. It uses depth-aware weighting to avoid bleeding across
// depth discontinuities.

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var depth_texture: texture_depth_2d;
@group(0) @binding(3) var depth_sampler: sampler;
@group(0) @binding(4) var output_texture: texture_storage_2d<rgba8unorm, write>;

// 4x4 Gaussian kernel weights
const KERNEL_SIZE: i32 = 2;
const GAUSSIAN_WEIGHTS: array<f32, 5> = array<f32, 5>(
    0.06136,  // offset -2
    0.24477,  // offset -1
    0.38774,  // offset 0
    0.24477,  // offset 1
    0.06136   // offset 2
);

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let texture_dims = textureDimensions(input_texture);
    let pixel_coord = vec2<i32>(global_id.xy);
    
    // Bounds check
    if (pixel_coord.x >= i32(texture_dims.x) || pixel_coord.y >= i32(texture_dims.y)) {
        return;
    }
    
    let uv = vec2<f32>(global_id.xy) / vec2<f32>(texture_dims);
    let texel_size = vec2<f32>(1.0) / vec2<f32>(texture_dims);
    
    // Sample center depth for bilateral weight (use textureLoad for depth texture)
    let center_depth = textureLoad(depth_texture, pixel_coord, 0);
    
    var sum = 0.0;
    var weight_sum = 0.0;
    
    // Separable blur (horizontal pass)
    // NOTE: In a full implementation, you'd do this in two passes (H then V)
    // For simplicity, this does a 2D blur in one pass
    for (var y = -KERNEL_SIZE; y <= KERNEL_SIZE; y++) {
        for (var x = -KERNEL_SIZE; x <= KERNEL_SIZE; x++) {
            let offset = vec2<i32>(x, y);
            let sample_pixel = pixel_coord + offset;
            let sample_uv = vec2<f32>(sample_pixel) / vec2<f32>(texture_dims);
            
            // Bounds check
            if (sample_pixel.x < 0 || sample_pixel.x >= i32(texture_dims.x) || 
                sample_pixel.y < 0 || sample_pixel.y >= i32(texture_dims.y)) {
                continue;
            }
            
            // Sample AO value
            let ao = textureSampleLevel(input_texture, input_sampler, sample_uv, 0.0).r;
            
            // Sample depth for bilateral weight (use textureLoad for depth texture)
            let sample_depth = textureLoad(depth_texture, sample_pixel, 0);
            
            // Bilateral weight based on depth difference
            let depth_diff = abs(center_depth - sample_depth);
            let depth_weight = exp(-depth_diff * 100.0); // Scale factor controls edge preservation
            
            // Spatial weight from Gaussian
            let spatial_weight = GAUSSIAN_WEIGHTS[abs(x) + KERNEL_SIZE] * 
                                 GAUSSIAN_WEIGHTS[abs(y) + KERNEL_SIZE];
            
            let weight = spatial_weight * depth_weight;
            sum += ao * weight;
            weight_sum += weight;
        }
    }
    
    let blurred_ao = sum / max(weight_sum, 0.0001);
    
    // Write to output
    textureStore(output_texture, pixel_coord, vec4<f32>(blurred_ao));
}
