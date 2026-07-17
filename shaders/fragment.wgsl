// Debug: Returns color-coded visualization of shadow coordinates
fn debug_shadow_coords(light_space_pos: vec4<f32>) -> vec3<f32> {
    var proj_coords = light_space_pos.xyz / light_space_pos.w;
    proj_coords = proj_coords * 0.5 + 0.5;
    proj_coords.y = 1.0 - proj_coords.y;
    
    // Color code: Red = outside X, Green = outside Y, Blue = outside Z, White = inside bounds
    var debug_color = vec3<f32>(1.0, 1.0, 1.0); // Start with white (inside)
    
    if (proj_coords.x < 0.0 || proj_coords.x > 1.0) {
        debug_color = vec3<f32>(1.0, 0.0, 0.0); // Red = outside X bounds
    } else if (proj_coords.y < 0.0 || proj_coords.y > 1.0) {
        debug_color = vec3<f32>(0.0, 1.0, 0.0); // Green = outside Y bounds
    } else if (proj_coords.z < 0.0 || proj_coords.z > 1.0) {
        debug_color = vec3<f32>(0.0, 0.0, 1.0); // Blue = outside Z bounds (depth)
    } else {
        // Inside bounds - show UV coordinates as color
        debug_color = vec3<f32>(proj_coords.x, proj_coords.y, 0.5);
    }
    
    return debug_color;
}

// Check if a light is active based on its intensity
fn is_light_active(light_idx: u32) -> bool {
    let intensities = shadow_matrices.light_intensities;
    if (light_idx == 0u) {
        return intensities.x > 0.01;
    } else if (light_idx == 1u) {
        return intensities.y > 0.01;
    } else if (light_idx == 2u) {
        return intensities.z > 0.01;
    } else {
        return intensities.w > 0.01;
    }
}

// Get light intensity by index
fn get_light_intensity(light_idx: u32) -> f32 {
    let intensities = shadow_matrices.light_intensities;
    if (light_idx == 0u) {
        return intensities.x;
    } else if (light_idx == 1u) {
        return intensities.y;
    } else if (light_idx == 2u) {
        return intensities.z;
    } else {
        return intensities.w;
    }
}

// Sample SSAO texture at screen-space position
// Returns ambient occlusion factor [0, 1] where 0 = fully occluded, 1 = no occlusion
fn sample_ssao(clip_pos: vec4<f32>) -> f32 {
    // Convert clip space to NDC (normalized device coordinates)
    let ndc = clip_pos.xy / clip_pos.w;
    
    // Convert NDC [-1, 1] to UV [0, 1]
    let uv = ndc * 0.5 + 0.5;
    
    // Flip Y coordinate (NDC has Y pointing up, texture has Y pointing down)
    let screen_uv = vec2<f32>(uv.x, 1.0 - uv.y);
    
    // Sample SSAO texture (R8Unorm stores AO directly in red channel)
    return textureSample(ssao_texture, ssao_sampler, screen_uv).r;
}

// Transform world position to light space using the specified light's matrix
// Light indices: 0 = Sun, 1 = Moon, 2 = Dynamic1, 3 = Dynamic2
fn world_to_light_space(world_pos: vec3<f32>, light_idx: u32) -> vec4<f32> {
    let world_pos_h = vec4<f32>(world_pos, 1.0);
    
    // Each light has its own shadow matrix stored in columns
    // We reconstruct rows for proper multiplication
    if (light_idx == 0u) {
        // Sun shadow matrix (light 0)
        let sm0 = shadow_matrices.light0_m0;
        let sm1 = shadow_matrices.light0_m1;
        let sm2 = shadow_matrices.light0_m2;
        let sm3 = shadow_matrices.light0_m3;
        let sm_r0 = vec4<f32>(sm0.x, sm1.x, sm2.x, sm3.x);
        let sm_r1 = vec4<f32>(sm0.y, sm1.y, sm2.y, sm3.y);
        let sm_r2 = vec4<f32>(sm0.z, sm1.z, sm2.z, sm3.z);
        let sm_r3 = vec4<f32>(sm0.w, sm1.w, sm2.w, sm3.w);
        return vec4<f32>(
            dot(sm_r0, world_pos_h),
            dot(sm_r1, world_pos_h),
            dot(sm_r2, world_pos_h),
            dot(sm_r3, world_pos_h)
        );
    } else if (light_idx == 1u) {
        // Moon shadow matrix (light 1)
        let sm0 = shadow_matrices.light1_m0;
        let sm1 = shadow_matrices.light1_m1;
        let sm2 = shadow_matrices.light1_m2;
        let sm3 = shadow_matrices.light1_m3;
        let sm_r0 = vec4<f32>(sm0.x, sm1.x, sm2.x, sm3.x);
        let sm_r1 = vec4<f32>(sm0.y, sm1.y, sm2.y, sm3.y);
        let sm_r2 = vec4<f32>(sm0.z, sm1.z, sm2.z, sm3.z);
        let sm_r3 = vec4<f32>(sm0.w, sm1.w, sm2.w, sm3.w);
        return vec4<f32>(
            dot(sm_r0, world_pos_h),
            dot(sm_r1, world_pos_h),
            dot(sm_r2, world_pos_h),
            dot(sm_r3, world_pos_h)
        );
    } else if (light_idx == 2u) {
        // Dynamic light 1 shadow matrix
        let sm0 = shadow_matrices.light2_m0;
        let sm1 = shadow_matrices.light2_m1;
        let sm2 = shadow_matrices.light2_m2;
        let sm3 = shadow_matrices.light2_m3;
        let sm_r0 = vec4<f32>(sm0.x, sm1.x, sm2.x, sm3.x);
        let sm_r1 = vec4<f32>(sm0.y, sm1.y, sm2.y, sm3.y);
        let sm_r2 = vec4<f32>(sm0.z, sm1.z, sm2.z, sm3.z);
        let sm_r3 = vec4<f32>(sm0.w, sm1.w, sm2.w, sm3.w);
        return vec4<f32>(
            dot(sm_r0, world_pos_h),
            dot(sm_r1, world_pos_h),
            dot(sm_r2, world_pos_h),
            dot(sm_r3, world_pos_h)
        );
    } else {
        // Dynamic light 2 shadow matrix (light 3)
        let sm0 = shadow_matrices.light3_m0;
        let sm1 = shadow_matrices.light3_m1;
        let sm2 = shadow_matrices.light3_m2;
        let sm3 = shadow_matrices.light3_m3;
        let sm_r0 = vec4<f32>(sm0.x, sm1.x, sm2.x, sm3.x);
        let sm_r1 = vec4<f32>(sm0.y, sm1.y, sm2.y, sm3.y);
        let sm_r2 = vec4<f32>(sm0.z, sm1.z, sm2.z, sm3.z);
        let sm_r3 = vec4<f32>(sm0.w, sm1.w, sm2.w, sm3.w);
        return vec4<f32>(
            dot(sm_r0, world_pos_h),
            dot(sm_r1, world_pos_h),
            dot(sm_r2, world_pos_h),
            dot(sm_r3, world_pos_h)
        );
    }
}

// ===== PCSS (Percentage Closer Soft Shadows) Implementation =====
// Poisson disk sampling patterns for PCSS
const POISSON_16: array<vec2<f32>, 16> = array<vec2<f32>, 16>(
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

// Hash function for per-pixel rotation
fn pcss_hash(p: vec2<f32>) -> f32 {
    let p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    let p_dot = dot(p3, vec3<f32>(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33));
    return fract((p3.x + p3.y) * p_dot);
}

// Rotate 2D vector
fn rotate2d(v: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(v.x * c - v.y * s, v.x * s + v.y * c);
}

// PCSS Step 1: Blocker Search
// PCSS Step 1: Search for average blocker depth
fn pcss_blocker_search(
    shadow_coords: vec3<f32>,
    light_idx: u32,
    search_radius: f32,
    num_samples: u32,
    rotation: f32,
    bias: f32
) -> f32 {
    let texel_size = 1.0 / 4096.0;
    var blocker_sum = 0.0;
    var blocker_count = 0.0;
    
    // Use biased depth for blocker comparison to prevent self-shadowing
    let biased_depth = shadow_coords.z - bias;
    
    for (var i = 0u; i < num_samples; i++) {
        let offset = rotate2d(POISSON_16[i % 16u], rotation) * search_radius * texel_size;
        let sample_coords = shadow_coords.xy + offset;
        
        if (sample_coords.x < 0.0 || sample_coords.x > 1.0 ||
            sample_coords.y < 0.0 || sample_coords.y > 1.0) {
            continue;
        }
        
        // Convert normalized coords to texel coords for textureLoad
        let texel_coords = vec2<i32>(sample_coords * 4096.0);
        
        // Load depth value directly (textureLoad returns f32 for depth textures)
        let shadow_depth = textureLoad(
            shadow_map,
            texel_coords,
            i32(light_idx),
            0
        );
        
        // Compare with biased depth to prevent self-shadowing artifacts
        if (shadow_depth < biased_depth) {
            blocker_sum += shadow_depth;
            blocker_count += 1.0;
        }
    }
    
    if (blocker_count < 0.5) {
        return -1.0;  // No blockers
    }
    
    return blocker_sum / blocker_count;
}

// PCSS Step 2: Penumbra Estimation
fn pcss_penumbra_size(
    receiver_depth: f32,
    blocker_depth: f32,
    light_size: f32,
    min_penumbra: f32,
    max_penumbra: f32
) -> f32 {
    let penumbra = light_size * (receiver_depth - blocker_depth) / max(blocker_depth, 0.001);
    return clamp(penumbra, min_penumbra, max_penumbra);
}

// PCSS Step 3: Variable-radius PCF
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
    var valid_samples = 0u;
    
    for (var i = 0u; i < num_samples; i++) {
        let offset = rotate2d(POISSON_16[i % 16u], rotation) * filter_radius * texel_size;
        let sample_coords = shadow_coords.xy + offset;
        
        // Skip out-of-bounds samples (don't contribute to the average)
        if (sample_coords.x < 0.0 || sample_coords.x > 1.0 ||
            sample_coords.y < 0.0 || sample_coords.y > 1.0) {
            continue;
        }
        
        shadow_sum += textureSampleCompareLevel(
            shadow_map,
            shadow_sampler,
            sample_coords,
            i32(light_idx),
            shadow_coords.z - bias
        );
        valid_samples += 1u;
    }
    
    // Return average of valid samples, or 0.0 (full shadow) if no valid samples
    return select(0.0, shadow_sum / f32(valid_samples), valid_samples > 0u);
}
// ===== End PCSS Implementation =====

// Sample shadow for a specific light using PCF or PCSS
// Takes world position and transforms it with the correct light's matrix
// Helper to compute PCSS shadow with specific sample counts
fn compute_pcss_with_samples(
    proj_coords: vec3<f32>,
    light_idx: u32,
    bias: f32,
    screen_pos: vec2<f32>,
    blocker_samples: u32,
    pcf_samples: u32,
    light_size: f32
) -> f32 {
    let search_radius = 15.0;
    let min_penumbra = 1.0;
    let max_penumbra = 32.0;
    let rotation = pcss_hash(screen_pos) * 6.28318530718;
    
    // Step 1: Blocker search
    let avg_blocker_depth = pcss_blocker_search(
        proj_coords,
        light_idx,
        search_radius,
        blocker_samples,
        rotation,
        bias
    );
    
    // No blockers found - fully lit
    if (avg_blocker_depth < 0.0) {
        return 1.0;
    }
    
    // Step 2: Estimate penumbra size
    let penumbra = pcss_penumbra_size(
        proj_coords.z,
        avg_blocker_depth,
        light_size,
        min_penumbra,
        max_penumbra
    );
    
    // Step 3: Variable-radius PCF
    return pcss_filter(
        proj_coords,
        light_idx,
        penumbra,
        pcf_samples,
        rotation,
        bias
    );
}

fn sample_light_shadow(light_idx: u32, world_pos: vec3<f32>, bias: f32, screen_pos: vec2<f32>, view_depth: f32) -> f32 {
    // Transform world position to this light's space
    let light_space_pos = world_to_light_space(world_pos, light_idx);
    
    // Perspective divide
    var proj_coords = light_space_pos.xyz / light_space_pos.w;
    
    // Transform from [-1, 1] to [0, 1] for texture coordinates
    // XY are in Clip Space (-1..1), so we map to 0..1
    // Z is ALREADY in 0..1 (because we fixed the projection matrix to be Zero-to-One)
    proj_coords.x = proj_coords.x * 0.5 + 0.5;
    proj_coords.y = proj_coords.y * 0.5 + 0.5;
    // proj_coords.z is kept as-is
    
    // Flip Y coordinate (texture coordinates are top-left origin)
    proj_coords.y = 1.0 - proj_coords.y;
    
    // Outside shadow map bounds? Return no shadow (fully lit)
    if (proj_coords.x < 0.0 || proj_coords.x > 1.0 ||
        proj_coords.y < 0.0 || proj_coords.y > 1.0 ||
        proj_coords.z < 0.0 || proj_coords.z > 1.0) {
        return 1.0;
    }
    
    // Get PCSS settings from metadata
    let pcss_quality = u32(shadow_matrices.metadata.z);
    let light_size = shadow_matrices.metadata.y;
    let shadow_distance = shadow_matrices.metadata.x;
    
    // Use PCSS if quality > 0, otherwise use fixed PCF
    if (pcss_quality > 0u) {
        // Base PCSS quality levels (for near distance)
        var blocker_samples = 8u;
        var pcf_samples = 16u;
        
        if (pcss_quality == 2u) { // Medium
            blocker_samples = 12u;
            pcf_samples = 24u;
        } else if (pcss_quality >= 3u) { // High
            blocker_samples = 16u;
            pcf_samples = 32u;
        }
        
        // Per-light quality adjustment: Moon gets reduced quality (less noticeable)
        if (light_idx == 1u) { // Moon
            blocker_samples = max(blocker_samples / 2u, 4u);
            pcf_samples = max(pcf_samples / 2u, 8u);
        }
        
        // Depth-based quality scaling with blending
        // Use view_depth (distance from camera) instead of light space depth
        let depth_fraction = clamp(view_depth / shadow_distance, 0.0, 1.0);
        
        // Define transition zones (blend over 10% ranges to avoid popping)
        let mid_start = 0.45;
        let mid_end = 0.55;
        let far_start = 0.70;
        let far_end = 0.80;
        
        if (depth_fraction > mid_start) {
            if (depth_fraction > far_start) {
                // Far distance zone (70-100%): Blend to minimal quality
                if (depth_fraction < far_end) {
                    // Transition zone: Blend between reduced and minimal quality
                    let blend = (depth_fraction - far_start) / (far_end - far_start);
                    
                    let reduced_blocker = max(u32(f32(blocker_samples) * 0.7), 4u);
                    let reduced_pcf = max(u32(f32(pcf_samples) * 0.7), 8u);
                    let minimal_blocker = max(u32(f32(blocker_samples) * 0.4), 4u);
                    let minimal_pcf = max(u32(f32(pcf_samples) * 0.4), 8u);
                    
                    // Sample at both quality levels
                    let shadow_high = compute_pcss_with_samples(
                        proj_coords, light_idx, bias, screen_pos,
                        reduced_blocker, reduced_pcf, light_size
                    );
                    let shadow_low = compute_pcss_with_samples(
                        proj_coords, light_idx, bias, screen_pos,
                        minimal_blocker, minimal_pcf, light_size
                    );
                    
                    return mix(shadow_high, shadow_low, blend);
                } else {
                    // Beyond transition: Use minimal quality
                    blocker_samples = max(u32(f32(blocker_samples) * 0.4), 4u);
                    pcf_samples = max(u32(f32(pcf_samples) * 0.4), 8u);
                }
            } else if (depth_fraction < mid_end) {
                // Mid-distance transition zone (45-55%): Blend to reduced quality
                let blend = (depth_fraction - mid_start) / (mid_end - mid_start);
                
                let full_blocker = blocker_samples;
                let full_pcf = pcf_samples;
                let reduced_blocker = max(u32(f32(blocker_samples) * 0.7), 4u);
                let reduced_pcf = max(u32(f32(pcf_samples) * 0.7), 8u);
                
                // Sample at both quality levels
                let shadow_high = compute_pcss_with_samples(
                    proj_coords, light_idx, bias, screen_pos,
                    full_blocker, full_pcf, light_size
                );
                let shadow_low = compute_pcss_with_samples(
                    proj_coords, light_idx, bias, screen_pos,
                    reduced_blocker, reduced_pcf, light_size
                );
                
                return mix(shadow_high, shadow_low, blend);
            } else {
                // Between transitions: Use reduced quality
                blocker_samples = max(u32(f32(blocker_samples) * 0.7), 4u);
                pcf_samples = max(u32(f32(pcf_samples) * 0.7), 8u);
            }
        }
        
        // Near distance or post-blend: Use computed sample counts
        return compute_pcss_with_samples(
            proj_coords, light_idx, bias, screen_pos,
            blocker_samples, pcf_samples, light_size
        );
    } else {
        // Fixed 3x3 PCF for compatibility (PCSS disabled)
        let texel_size = 1.0 / 4096.0;
        var shadow_sum = 0.0;
        
        for (var x = -1.0; x <= 1.0; x += 1.0) {
            for (var y = -1.0; y <= 1.0; y += 1.0) {
                let offset = vec2<f32>(x, y) * texel_size;
                let sample_coords = proj_coords.xy + offset;
                shadow_sum += textureSampleCompareLevel(
                    shadow_map, 
                    shadow_sampler, 
                    sample_coords, 
                    i32(light_idx),
                    proj_coords.z - bias
                );
            }
        }
        return shadow_sum / 9.0;
    }
}

// Calculate shadow factor for a specific light with distance fade
// Light indices: 0 = Sun, 1 = Moon, 2 = Dynamic1, 3 = Dynamic2
fn calculate_light_shadow(light_idx: u32, world_pos: vec3<f32>, normal: vec3<f32>, light_dir: vec3<f32>, view_depth: f32, screen_pos: vec2<f32>) -> f32 {
    // Check if this light is active
    if (!is_light_active(light_idx)) {
        return 1.0; // No shadow from inactive lights
    }
    
    // Shadow distance (from metadata.x in uniform)
    let shadow_distance = shadow_matrices.metadata.x;
    if (shadow_distance <= 0.0) {
        return 1.0; // Shadow disabled
    }
    
    // Objects beyond shadow distance - fade to no shadow
    if (view_depth > shadow_distance) {
        // Fade from slight shadow at shadow_distance to no shadow at 2x distance
        let beyond_distance = view_depth - shadow_distance;
        let fade_factor = clamp(beyond_distance / shadow_distance, 0.0, 1.0);
        return mix(0.7, 1.0, fade_factor);
    }
    
    // Normal offset bias: Move sample point along normal to reduce acne on curved surfaces
    // DISABLED (Exp 17): Using Hardware Depth Bias instead.
    let ndotl = max(dot(normal, light_dir), 0.0);
    let normal_offset = 0.0; 
    let offset_pos = world_pos;
    
    // Slope-scale depth bias for shadow acne prevention
    // DISABLED (Exp 17): Using Hardware Depth Bias instead.
    // We keep a tiny epsilon just for float precision.
    let bias = 0.000005; 
    
    // Sample shadow for this light (with PCSS if enabled)
    let shadow = sample_light_shadow(light_idx, offset_pos, bias, screen_pos, view_depth);
    
    // Fade shadow at distance to avoid hard cut
    // Fade from 80% to 100% of shadow_distance
    let fade_start = shadow_distance * 0.8;
    if (view_depth > fade_start) {
        let fade_factor = clamp((view_depth - fade_start) / (shadow_distance - fade_start), 0.0, 1.0);
        return mix(shadow, 1.0, fade_factor);
    }
    
    return shadow;
}// Calculate contribution from a single point light
// Returns (diffuse_contrib, specular_contrib) as separate vec3s for flexibility
fn calculate_point_light(
    light_pos: vec3<f32>,
    light_color: vec3<f32>,
    light_intensity: f32,
    light_range: f32,
    world_pos: vec3<f32>,
    normal: vec3<f32>,
    view_dir: vec3<f32>,
    albedo: vec3<f32>,
    is_metal: bool,
    spec_strength: f32,
    spec_power: f32,
) -> vec3<f32> {
    // Vector from surface to light
    let light_dir = light_pos - world_pos;
    let distance = length(light_dir);
    
    // Early out if beyond range
    if (distance > light_range) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }
    
    let L = normalize(light_dir);
    
    // Distance attenuation using inverse square law with smooth falloff
    // Attenuation = intensity / (1 + distance² / range²)
    // This ensures light reaches 0 at exactly `range` distance
    let dist_ratio = distance / light_range;
    let attenuation = light_intensity / (1.0 + dist_ratio * dist_ratio * 4.0);
    
    if (attenuation < 0.001) {
        return vec3<f32>(0.0, 0.0, 0.0); // Too dim to matter
    }
    
    // Lambertian diffuse
    let ndotl = max(dot(normal, L), 0.0);
    
    // Reduce diffuse for metals (they reflect more than absorb)
    let diff_strength = select(1.0, 0.3, is_metal);
    let diffuse = albedo * ndotl * diff_strength * light_color * attenuation;
    
    // Blinn-Phong specular
    let H = normalize(L + view_dir);
    let spec_highlight = pow(max(dot(normal, H), 0.0), spec_power);
    let specular = albedo * spec_strength * spec_highlight * light_color * attenuation;
    
    return diffuse + specular;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Extract lighting parameters from uniform
    let sun_dir = normalize(lighting.sun_direction.xyz);
    let sun_intensity = lighting.sun_direction.w;
    let sun_col = lighting.sun_color.xyz;
    let moon_dir = normalize(lighting.moon_direction.xyz);
    let moon_intensity = lighting.moon_direction.w;
    let moon_col = lighting.moon_color.xyz;
    let ambient_col = lighting.ambient.xyz;
    let ambient_intensity = lighting.ambient.w;
    
    let N = normalize(in.normal);
    let L = sun_dir;
    let diff = max(dot(N, L), 0.0);
    
    // Moon lighting calculation
    let L_moon = moon_dir;
    let diff_moon = max(dot(N, L_moon), 0.0);
    
    // Calculate view-space depth for shadow distance calculations
    let cam_pos_vec = vec3<f32>(camera.cam_pos.x, camera.cam_pos.y, camera.cam_pos.z);
    let view_depth = length(in.world_pos - cam_pos_vec);
    
    // Screen-space position for PCSS per-pixel rotation
    let screen_pos = in.clip.xy;
    
    // Calculate shadow factors for sun (light 0) and moon (light 1) with PCSS
    let sun_shadow = calculate_light_shadow(0u, in.world_pos, N, L, view_depth, screen_pos);
    let moon_shadow = calculate_light_shadow(1u, in.world_pos, N, L_moon, view_depth, screen_pos);
    
    // Multi-light shadow debug visualization
    // Set to true to see which lights are casting shadows
    let shadow_debug = false;
    if (shadow_debug) {
        // Visualize sun shadow (red) and moon shadow (green)
        let sun_contrib = (1.0 - sun_shadow) * sun_intensity;
        let moon_contrib = (1.0 - moon_shadow) * moon_intensity;
        return vec4<f32>(sun_contrib, moon_contrib, 0.0, 1.0);
    }
    
    // use camera-provided world position for view direction
    let cam_pos = vec3<f32>(camera.cam_pos.x, camera.cam_pos.y, camera.cam_pos.z);
    let V = normalize(cam_pos - in.world_pos);
    let H = normalize(L + V);
    let spec = pow(max(dot(N, H), 0.0), 64.0);

    // Hybrid ambient occlusion strategy (Phase 3):
    // - For blocky geometry (geometry_type == 1): Use per-vertex AO only (4-corner neighbor occupancy)
    // - For smooth terrain (geometry_type == 0): Blend per-vertex AO with SSAO
    //   - Per-vertex AO provides macro-scale occlusion from terrain shape
    //   - SSAO provides micro-scale contact shadows and crevice darkening
    let vertex_ao = in.ao;
    let screen_ssao = sample_ssao(in.clip);
    
    // Blend based on geometry type:
    // Apply SSAO to both smooth and blocky geometry to ensure contact shadows in crevices
    let ao = vertex_ao * screen_ssao;

    let mat = materials[in.material];
    let albedo = mat.albedo.xyz;
    // For terrain (object_type 0) params.xyz encodes the side-face colour, not
    // material properties — zero them out so we don't accidentally enter the
    // dielectric or metal branches below.
    let is_terrain = in.object_type == 0u;
    let fuzz    = select(mat.params.x, 0.0, is_terrain);
    let ref_idx = select(mat.params.y, 0.0, is_terrain);

    // Emissive short-circuit: skip all lighting, output flat colour * intensity.
    // Any material with params.w > 0 is treated as self-illuminating.
    if mat.params.w > 0.0 {
        return vec4<f32>(mat.albedo.xyz * mat.params.w, 1.0);
    }

    // Terrain (object_type 0): normal-based face colour sourced from the material.
    //   mat.albedo.xyz = top-face colour (grass), mat.params.xyz = side-face colour (dirt).
    // All other geometry: use material albedo directly.
    let is_top_face = abs(N.y) > 0.9;
    let voxel_albedo = select(
        mat.params.xyz,  // side colour for terrain
        mat.albedo.xyz,  // top colour for terrain
        is_top_face
    );
    let final_albedo = select(albedo, voxel_albedo, is_terrain);

    // If ref_idx > 0 we treat the material as a dielectric (glass-like).
    // We don't implement true refraction here (no background sampling), but
    // use a Fresnel-based specular term and reduce diffuse contribution so
    // the object appears reflective/transparent rather than fully diffuse.
    var color: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);
    if (ref_idx > 0.0) {
        // Schlick's approximation for Fresnel
        let r0 = (1.0 - ref_idx) / (1.0 + ref_idx);
        let R0 = r0 * r0;
        let cos_theta = max(dot(N, V), 0.0);
        let F = R0 + (1.0 - R0) * pow(1.0 - cos_theta, 5.0);

        // Strong, sharp specular for dielectrics; use a high exponent
        // Apply sun shadow to sun lighting
        let spec_diel = vec3<f32>(F) * pow(max(dot(N, H), 0.0), 128.0) * sun_col * sun_intensity * sun_shadow;
        
        // Moon specular for dielectrics (with moon shadow)
        let H_moon = normalize(L_moon + V);
        let spec_diel_moon = vec3<f32>(F) * pow(max(dot(N, H_moon), 0.0), 128.0) * moon_col * moon_intensity * 0.5 * moon_shadow;

        // Approximate transmitted light (tint) scaled by (1 - F). This is a
        // cheap stand-in for refraction/transmission and helps the object
        // look glassy when combined with specular.
        let trans = final_albedo * (1.0 - F) * 0.6 * sun_col * sun_intensity * sun_shadow;
        let trans_moon = final_albedo * (1.0 - F) * 0.6 * moon_col * moon_intensity * 0.5 * moon_shadow;

        // Reduce ambient in shadowed areas AND on back-facing surfaces
        // Use minimum of both shadows for ambient darkening
        let combined_shadow = min(sun_shadow, moon_shadow);
        let shadow_darkening = mix(0.2, 1.0, combined_shadow);
        let backface_darkening = mix(0.15, 1.0, diff);
        let total_darkening = shadow_darkening * backface_darkening;
        color = ambient_col * ambient_intensity * final_albedo * 0.1 * total_darkening + ao * (spec_diel + trans + spec_diel_moon + trans_moon);
        // approximate alpha: more reflective (higher F) -> less transmitted
        // we bias alpha so very slight translucency remains even for weakly
        // refractive materials.
        let alpha = clamp((1.0 - F) * 0.6 + 0.05, 0.02, 1.0);
    } else {
        // Non-dielectric path (Lambertian / Metal)
        // For metals, fuzz represents surface roughness (0 = smooth, 1 = rough)
        // Detect if this is a metal: fuzz > 0.01 indicates metallic material
        let is_metal = step(0.01, fuzz);
        
        // For metals: reduce diffuse, increase specular
        // For lambertian: normal diffuse, minimal specular
        let diff_strength = mix(1.0, 0.3, is_metal);
        // Apply sun shadow to sun diffuse lighting
        let diff_color = final_albedo * diff * diff_strength * sun_col * sun_intensity * sun_shadow;
        
        // Moon diffuse lighting with moon shadow
        let diff_color_moon = final_albedo * diff_moon * diff_strength * moon_col * moon_intensity * moon_shadow;
        
        // Metal specular: strong but affected by fuzz (roughness)
        // Lambertian specular: very weak
        let base_spec_strength = mix(0.04, 0.85, is_metal);
        // Fuzz reduces specular strength for metals (rougher = less reflective)
        let fuzz_factor = mix(1.0, 0.4, clamp(fuzz, 0.0, 1.0));
        let spec_strength = base_spec_strength * fuzz_factor;
        
        // Sharper specular for metals (higher power)
        let spec_power = mix(64.0, 96.0, is_metal);
        let spec_highlight = pow(max(dot(N, H), 0.0), spec_power);
        
        // Moon specular (softer than sun)
        let H_moon = normalize(L_moon + V);
        let spec_highlight_moon = pow(max(dot(N, H_moon), 0.0), spec_power * 0.8);
        
        // For metals the specular should be tinted by albedo and light color
        // Apply respective shadow to each light source
        let spec_color = final_albedo * spec_strength * spec_highlight * sun_col * sun_intensity * sun_shadow;
        let spec_color_moon = final_albedo * spec_strength * spec_highlight_moon * moon_col * moon_intensity * 0.5 * moon_shadow;

        // Reduce ambient in shadowed areas AND on back-facing surfaces
        // Surfaces in complete shadow should be much darker
        // Also darken surfaces facing away from light (diff close to 0)
        // Use minimum of both shadows for ambient darkening
        let combined_shadow = min(sun_shadow, moon_shadow);
        let shadow_darkening = mix(0.2, 1.0, combined_shadow); // Darken by 80% in full shadow
        let backface_darkening = mix(0.15, 1.0, diff); // Darken by 85% when facing away
        let total_darkening = shadow_darkening * backface_darkening;
        color = ambient_col * ambient_intensity * final_albedo * total_darkening + ao * (diff_color + spec_color + diff_color_moon + spec_color_moon);
    }
    
    // Accumulate dynamic point light contributions
    // Iterate over all active point lights and add their contributions
    let num_lights = dynamic_lights.light_count.x;
    var point_light_contrib = vec3<f32>(0.0, 0.0, 0.0);
    
    if (num_lights > 0u) {
        // Determine material properties for point lights (reuse from above)
        let is_metal = step(0.01, fuzz);
        let base_spec_strength = mix(0.04, 0.85, is_metal);
        let fuzz_factor = mix(1.0, 0.4, clamp(fuzz, 0.0, 1.0));
        let spec_strength = base_spec_strength * fuzz_factor;
        let spec_power = mix(64.0, 96.0, is_metal);
        
        // Accumulate point light contributions
        for (var i = 0u; i < num_lights; i = i + 1u) {
            let light = dynamic_lights.lights[i];
            let light_pos = light.position_range.xyz;
            let light_range = light.position_range.w;
            let light_color = light.color_intensity.xyz;
            let light_intensity = light.color_intensity.w;
            
            // Calculate light contribution (diffuse + specular)
            let contrib = calculate_point_light(
                light_pos,
                light_color,
                light_intensity,
                light_range,
                in.world_pos,
                N,
                V,
                final_albedo,
                is_metal > 0.5,
                spec_strength,
                spec_power
            );
            
            point_light_contrib += contrib;
        }
        
        // Add point light contribution with AO applied
        color += ao * point_light_contrib;
    }
    
    // DEBUG: Visualize shadow coordinates (comment out for normal rendering)
    // Uncomment the line below to see shadow map coverage:
    // Red = outside X bounds, Green = outside Y bounds, Blue = outside Z bounds (depth)
    // Color gradient = inside bounds (shows UV coords)
    // return vec4<f32>(debug_shadow_coords(in.light_space_pos), 1.0);
    
    // Apply per-channel block light (colored, from torches/emissive blocks) blended
    // with sky exposure (white ambient from open sky). Take the max of the two so a
    // brightly lit block underground isn't dimmed by low sky exposure, and open-sky
    // surfaces with no nearby light source still get full ambient brightness.
    // Ensure minimum brightness so geometry isn't completely black if both are 0.
    // TODO: Implement proper light occlusion - blocks should prevent light from
    // reaching surfaces in crevices/caves (requires transparency checks in propagation)
    let sky_ambient = vec3<f32>(in.sky_exposed);
    let total_light = max(in.block_light_rgb, sky_ambient);
    color = color * max(total_light, vec3<f32>(0.1));
    
    // For dielectrics we computed `alpha` above; otherwise alpha is opaque.
    var out_alpha: f32 = 1.0;
    if (ref_idx > 0.0) {
        out_alpha = clamp((1.0 - ( (1.0 - ref_idx) / (1.0 + ref_idx) ) * ((1.0 - ref_idx) / (1.0 + ref_idx))) * 0.6 + 0.05, 0.02, 1.0);
    }

    // DEBUG VISUALIZATION
    let debug_mode = u32(lighting.params.y);
    if (debug_mode == 1u) {
        // Mode 1: World Normals (RGB = Normal * 0.5 + 0.5)
        return vec4<f32>(N * 0.5 + 0.5, 1.0);
    } else if (debug_mode == 2u) {
        // Mode 2: Bias Heatmap (Red = High Bias)
        // Re-calculate bias to visualize it
        let ndotl = max(dot(N, sun_dir), 0.0);
        // MATCH THE FIX:
        let raw_slope_bias = 0.000005 * sqrt(1.0 - ndotl * ndotl) / max(ndotl, 0.05);
        let slope_bias = min(raw_slope_bias, 0.00002);
        
        // Visualize bias magnitude
        // 0.00002 * 40000.0 = 0.8 (Bright Red)
        return vec4<f32>(slope_bias * 40000.0, 0.0, 0.0, 1.0);
    } else if (debug_mode == 3u) {
        // Mode 3: Shadow Factor (White = Lit, Black = Shadow)
        return vec4<f32>(vec3<f32>(sun_shadow), 1.0);
    } else if (debug_mode == 4u) {
        // Mode 4: Raw Light Level
        return vec4<f32>(vec3<f32>(in.light_level), 1.0);
    } else if (debug_mode == 5u) {
        // Mode 5: Point Lights Contribution
        return vec4<f32>(point_light_contrib, 1.0);
    }

    return vec4<f32>(color, out_alpha);
}
