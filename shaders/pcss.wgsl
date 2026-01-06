// Percentage Closer Soft Shadows (PCSS) implementation
//
// This file contains functions for implementing PCSS, which creates
// physically-based soft shadows where softness varies with distance
// from the shadow caster.
//
// Algorithm:
// 1. Blocker Search: Find average depth of occluding geometry
// 2. Penumbra Estimation: Calculate shadow softness based on distances
// 3. Variable PCF: Filter with kernel size based on penumbra estimate

// Poisson disk sampling pattern for PCSS (16 samples)
// Distributed evenly in unit circle for good coverage
const POISSON_DISK_16: array<vec2<f32>, 16> = array<vec2<f32>, 16>(
    vec2<f32>(-0.94201624, -0.39906216),
    vec2<f32>( 0.94558609, -0.76890725),
    vec2<f32>(-0.094184101, -0.92938870),
    vec2<f32>( 0.34495938,  0.29387760),
    vec2<f32>(-0.91588581,  0.45771432),
    vec2<f32>(-0.81544232, -0.87912464),
    vec2<f32>(-0.38277543,  0.27676845),
    vec2<f32>( 0.97484398,  0.75648379),
    vec2<f32>( 0.44323325, -0.97511554),
    vec2<f32>( 0.53742981, -0.47373420),
    vec2<f32>(-0.26496911, -0.41893023),
    vec2<f32>( 0.79197514,  0.19090188),
    vec2<f32>(-0.24188840,  0.99706507),
    vec2<f32>(-0.81409955,  0.91437590),
    vec2<f32>( 0.19984126,  0.78641367),
    vec2<f32>( 0.14383161, -0.14100790)
);

// Additional 32-sample Poisson disk for higher quality
const POISSON_DISK_32: array<vec2<f32>, 32> = array<vec2<f32>, 32>(
    vec2<f32>(-0.94201624, -0.39906216),
    vec2<f32>( 0.94558609, -0.76890725),
    vec2<f32>(-0.094184101, -0.92938870),
    vec2<f32>( 0.34495938,  0.29387760),
    vec2<f32>(-0.91588581,  0.45771432),
    vec2<f32>(-0.81544232, -0.87912464),
    vec2<f32>(-0.38277543,  0.27676845),
    vec2<f32>( 0.97484398,  0.75648379),
    vec2<f32>( 0.44323325, -0.97511554),
    vec2<f32>( 0.53742981, -0.47373420),
    vec2<f32>(-0.26496911, -0.41893023),
    vec2<f32>( 0.79197514,  0.19090188),
    vec2<f32>(-0.24188840,  0.99706507),
    vec2<f32>(-0.81409955,  0.91437590),
    vec2<f32>( 0.19984126,  0.78641367),
    vec2<f32>( 0.14383161, -0.14100790),
    vec2<f32>(-0.53984505, -0.66302765),
    vec2<f32>( 0.71255219, -0.29883471),
    vec2<f32>(-0.00195503,  0.53868192),
    vec2<f32>( 0.33912184, -0.56749830),
    vec2<f32>( 0.25465642,  0.71934384),
    vec2<f32>(-0.66351717,  0.21931773),
    vec2<f32>(-0.31381029, -0.23970658),
    vec2<f32>( 0.66012466,  0.45485940),
    vec2<f32>(-0.49779937,  0.87818235),
    vec2<f32>( 0.87460357, -0.02350538),
    vec2<f32>(-0.15238107,  0.10829121),
    vec2<f32>( 0.38516614, -0.20240621),
    vec2<f32>(-0.06816042, -0.56843674),
    vec2<f32>( 0.02857628,  0.91085684),
    vec2<f32>(-0.94851988,  0.07145872),
    vec2<f32>( 0.50467544,  0.04501028)
);

// Simple hash function for per-pixel rotation
fn hash(p: vec2<f32>) -> f32 {
    let p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    let p_dot = dot(p3, vec3<f32>(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33));
    return fract((p3.x + p3.y) * p_dot);
}

// Rotate 2D vector by angle
fn rotate2d(v: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(
        v.x * c - v.y * s,
        v.x * s + v.y * c
    );
}

// PCSS Step 1: Blocker Search
// Searches for occluding geometry and returns average blocker depth
// Returns -1.0 if no blockers found (fragment is fully lit)
fn pcss_blocker_search(
    shadow_coords: vec3<f32>,
    light_idx: u32,
    search_radius: f32,
    num_samples: u32,
    rotation: f32
) -> f32 {
    let texel_size = 1.0 / 4096.0;
    var blocker_sum = 0.0;
    var blocker_count = 0.0;
    
    // Sample shadow map in search region
    for (var i = 0u; i < num_samples; i++) {
        // Get sample offset and rotate it
        let offset = rotate2d(POISSON_DISK_16[i % 16u], rotation) * search_radius * texel_size;
        let sample_coords = shadow_coords.xy + offset;
        
        // Bounds check
        if (sample_coords.x < 0.0 || sample_coords.x > 1.0 ||
            sample_coords.y < 0.0 || sample_coords.y > 1.0) {
            continue;
        }
        
        // Sample shadow map depth
        let shadow_depth = textureSampleLevel(
            shadow_map,
            shadow_sampler_nearest,  // Use nearest for depth comparison
            sample_coords,
            i32(light_idx),
            0.0
        ).r;
        
        // If this sample is blocking (closer than receiver)
        if (shadow_depth < shadow_coords.z) {
            blocker_sum += shadow_depth;
            blocker_count += 1.0;
        }
    }
    
    // No blockers found - fully lit
    if (blocker_count < 0.5) {
        return -1.0;
    }
    
    // Return average blocker depth
    return blocker_sum / blocker_count;
}

// PCSS Step 2: Penumbra Estimation
// Calculates the softness of the shadow based on distances
fn pcss_penumbra_size(
    receiver_depth: f32,
    blocker_depth: f32,
    light_size: f32,
    min_penumbra: f32,
    max_penumbra: f32
) -> f32 {
    // Penumbra = light_size * (receiver_depth - blocker_depth) / blocker_depth
    let penumbra = light_size * (receiver_depth - blocker_depth) / max(blocker_depth, 0.001);
    return clamp(penumbra, min_penumbra, max_penumbra);
}

// PCSS Step 3: Variable-radius PCF
// Performs percentage-closer filtering with dynamic kernel size
fn pcss_filter(
    shadow_coords: vec3<f32>,
    light_idx: u32,
    filter_radius: f32,
    num_samples: u32,
    rotation: f32,
    bias: f32
) -> f32 {
    let texel_size = 1.0 / 4096.0;
    var shadow_sum = 0.0;
    
    // Sample shadow map with PCF
    for (var i = 0u; i < num_samples; i++) {
        // Get sample offset and rotate it
        let sample_idx = i % 32u;  // Support up to 32 samples
        var offset: vec2<f32>;
        if (sample_idx < 16u) {
            offset = POISSON_DISK_16[sample_idx];
        } else {
            offset = POISSON_DISK_32[sample_idx];
        }
        offset = rotate2d(offset, rotation) * filter_radius * texel_size;
        
        let sample_coords = shadow_coords.xy + offset;
        
        // Bounds check
        if (sample_coords.x < 0.0 || sample_coords.x > 1.0 ||
            sample_coords.y < 0.0 || sample_coords.y > 1.0) {
            shadow_sum += 1.0;  // Outside bounds = no shadow
            continue;
        }
        
        // Sample with depth comparison
        shadow_sum += textureSampleCompareLevel(
            shadow_map,
            shadow_sampler,
            sample_coords,
            i32(light_idx),
            shadow_coords.z - bias
        );
    }
    
    return shadow_sum / f32(num_samples);
}

// Main PCSS function
// Returns shadow factor (0.0 = full shadow, 1.0 = no shadow)
fn calculate_pcss_shadow(
    shadow_coords: vec3<f32>,
    light_idx: u32,
    bias: f32,
    light_size: f32,
    search_radius: f32,
    min_penumbra: f32,
    max_penumbra: f32,
    blocker_samples: u32,
    pcf_samples: u32,
    screen_pos: vec2<f32>
) -> f32 {
    // Generate per-pixel rotation to reduce banding
    let rotation = hash(screen_pos) * 6.28318530718; // 0 to 2*PI
    
    // Step 1: Blocker search
    let avg_blocker_depth = pcss_blocker_search(
        shadow_coords,
        light_idx,
        search_radius,
        blocker_samples,
        rotation
    );
    
    // No blockers found - fully lit
    if (avg_blocker_depth < 0.0) {
        return 1.0;
    }
    
    // Step 2: Estimate penumbra size
    let penumbra = pcss_penumbra_size(
        shadow_coords.z,
        avg_blocker_depth,
        light_size,
        min_penumbra,
        max_penumbra
    );
    
    // Step 3: Variable-radius PCF
    return pcss_filter(
        shadow_coords,
        light_idx,
        penumbra,
        pcf_samples,
        rotation,
        bias
    );
}
