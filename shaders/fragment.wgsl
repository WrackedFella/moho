// Keeps a surface with no sky exposure and no block light from going black.
const MIN_SKY_AMBIENT: f32 = 0.1;
// Normal offset of the shadow lookup, in texels of the cascade looked up. 2.0 covers the
// 1-texel filter radius plus the bilinear compare footprint. A test in
// moho_renderer's shadow.rs pins the value.
const NORMAL_OFFSET_TEXELS: f32 = 2.0;
// Cascades per light; the sun's layers come first, then the moon's.
const SUN_CASCADES: u32 = 4u;
const MOON_CASCADES: u32 = 2u;
// Scales block light's additive term; can push colour past 1 next to strong emitters.
const BLOCK_LIGHT_GAIN: f32 = 1.0;

fn is_light_active(light_idx: u32) -> bool {
    return shadow_data.light_intensities[light_idx] > 0.01;
}

// Cascades of a light (0 = sun, 1 = moon).
fn cascade_count(light_idx: u32) -> u32 {
    return select(MOON_CASCADES, SUN_CASCADES, light_idx == 0u);
}

// Shadow-map layer of a light's first cascade.
fn first_layer(light_idx: u32) -> u32 {
    return select(SUN_CASCADES, 0u, light_idx == 0u);
}

// The cascade covering a view depth, and how much of the next one to mix in.
struct CascadePick {
    // Index within the light's cascades; the light's cascade count when the depth is past the last.
    cascade: u32,
    // Weight of cascade + 1 across the blend band at the end of the slice; 0 outside it.
    blend: f32,
}

fn pick_cascade(light_idx: u32, z: f32) -> CascadePick {
    let count = cascade_count(light_idx);
    let splits = shadow_data.split_far[light_idx];

    // Splits rise, so the first cascade with z < split is the number of splits already passed.
    var cascade = 0u;
    for (var i = 0u; i < count; i++) {
        cascade += select(0u, 1u, z >= splits[i]);
    }
    if (cascade >= count) {
        return CascadePick(count, 0.0);
    }

    var blend = 0.0;
    if (cascade + 1u < count) {
        let end = splits[cascade];
        let start = select(0.0, splits[max(cascade, 1u) - 1u], cascade > 0u);
        let band_start = end - shadow_data.camera_forward.w * (end - start);
        blend = clamp((z - band_start) / max(end - band_start, 0.0001), 0.0, 1.0);
    }
    return CascadePick(cascade, blend);
}

// Debug view 8: red, green, blue, yellow for the sun's cascades, mixed across the blend band.
fn cascade_debug_colour(pick: CascadePick) -> vec3<f32> {
    var colours = array<vec3<f32>, 4>(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(0.0, 0.0, 1.0),
        vec3<f32>(1.0, 1.0, 0.0),
    );
    if (pick.cascade >= SUN_CASCADES) {
        return vec3<f32>(0.1);
    }
    let next = min(pick.cascade + 1u, SUN_CASCADES - 1u);
    return mix(colours[pick.cascade], colours[next], pick.blend);
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
    layer: u32,
    search_radius: f32,
    num_samples: u32,
    rotation: f32,
    bias: f32
) -> f32 {
    let map_size = vec2<f32>(textureDimensions(shadow_map));
    let texel_size = 1.0 / map_size.x;
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
        let texel_coords = vec2<i32>(sample_coords * map_size);
        
        // Load depth value directly (textureLoad returns f32 for depth textures)
        let shadow_depth = textureLoad(
            shadow_map,
            texel_coords,
            i32(layer),
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
    layer: u32,
    filter_radius: f32,
    num_samples: u32,
    rotation: f32,
    bias: f32
) -> f32 {
    let texel_size = 1.0 / f32(textureDimensions(shadow_map).x);
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
            i32(layer),
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
    layer: u32,
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
        layer,
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
        layer,
        penumbra,
        pcf_samples,
        rotation,
        bias
    );
}

fn sample_light_shadow(layer: u32, world_pos: vec3<f32>, bias: f32, screen_pos: vec2<f32>, view_depth: f32) -> f32 {
    let light_space_pos = shadow_data.matrices[layer] * vec4<f32>(world_pos, 1.0);

    // Perspective divide
    var proj_coords = light_space_pos.xyz / light_space_pos.w;

    // XY are in clip space (-1..1); Z is already 0..1 (the cascade projections are zero-to-one).
    proj_coords.x = proj_coords.x * 0.5 + 0.5;
    proj_coords.y = proj_coords.y * 0.5 + 0.5;

    // Flip Y coordinate (texture coordinates are top-left origin)
    proj_coords.y = 1.0 - proj_coords.y;

    // Outside shadow map bounds? Return no shadow (fully lit)
    if (proj_coords.x < 0.0 || proj_coords.x > 1.0 ||
        proj_coords.y < 0.0 || proj_coords.y > 1.0 ||
        proj_coords.z < 0.0 || proj_coords.z > 1.0) {
        return 1.0;
    }
    
    // Get PCSS settings from metadata
    let pcss_quality = u32(shadow_data.metadata.z);
    let light_size = shadow_data.metadata.y;
    let shadow_distance = shadow_data.metadata.x;
    
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
        if (layer >= SUN_CASCADES) {
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
                        proj_coords, layer, bias, screen_pos,
                        reduced_blocker, reduced_pcf, light_size
                    );
                    let shadow_low = compute_pcss_with_samples(
                        proj_coords, layer, bias, screen_pos,
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
                    proj_coords, layer, bias, screen_pos,
                    full_blocker, full_pcf, light_size
                );
                let shadow_low = compute_pcss_with_samples(
                    proj_coords, layer, bias, screen_pos,
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
            proj_coords, layer, bias, screen_pos,
            blocker_samples, pcf_samples, light_size
        );
    } else {
        // Fixed 3x3 PCF for compatibility (PCSS disabled)
        let texel_size = 1.0 / f32(textureDimensions(shadow_map).x);
        var shadow_sum = 0.0;
        
        for (var x = -1.0; x <= 1.0; x += 1.0) {
            for (var y = -1.0; y <= 1.0; y += 1.0) {
                let offset = vec2<f32>(x, y) * texel_size;
                let sample_coords = proj_coords.xy + offset;
                shadow_sum += textureSampleCompareLevel(
                    shadow_map, 
                    shadow_sampler, 
                    sample_coords, 
                    i32(layer),
                    proj_coords.z - bias
                );
            }
        }
        return shadow_sum / 9.0;
    }
}

// Shadow factor of one cascade, looked up at the position pushed along the geometric normal by
// a few of that cascade's texels so a grazing surface does not shadow itself.
fn sample_cascade_shadow(light_idx: u32, cascade: u32, world_pos: vec3<f32>, normal: vec3<f32>, light_dir: vec3<f32>, screen_pos: vec2<f32>, view_depth: f32) -> f32 {
    let texel = shadow_data.texel_world[light_idx][cascade];
    let ndotl = clamp(dot(normal, light_dir), -1.0, 1.0);
    let offset_pos = world_pos + normal * (texel * NORMAL_OFFSET_TEXELS * sqrt(1.0 - ndotl * ndotl));

    // Hardware depth bias and the normal offset carry acne prevention; this is float epsilon.
    let bias = 0.000005;

    return sample_light_shadow(first_layer(light_idx) + cascade, offset_pos, bias, screen_pos, view_depth);
}

// Calculate shadow factor for a specific light with distance fade
// Light indices: 0 = Sun, 1 = Moon. `view_depth` is the distance along the camera's forward axis.
fn calculate_light_shadow(light_idx: u32, world_pos: vec3<f32>, normal: vec3<f32>, light_dir: vec3<f32>, view_depth: f32, screen_pos: vec2<f32>) -> f32 {
    // Check if this light is active
    if (!is_light_active(light_idx)) {
        return 1.0; // No shadow from inactive lights
    }

    let shadow_distance = shadow_data.metadata.x;
    if (shadow_distance <= 0.0) {
        return 1.0; // Shadow disabled
    }

    // Past the last cascade nothing is shadowed.
    let pick = pick_cascade(light_idx, view_depth);
    if (pick.cascade >= cascade_count(light_idx)) {
        return 1.0;
    }

    var shadow = sample_cascade_shadow(light_idx, pick.cascade, world_pos, normal, light_dir, screen_pos, view_depth);
    if (pick.blend > 0.0) {
        let next = sample_cascade_shadow(light_idx, pick.cascade + 1u, world_pos, normal, light_dir, screen_pos, view_depth);
        shadow = mix(shadow, next, pick.blend);
    }

    // Fade shadow at distance to avoid hard cut
    // Fade from 80% to 100% of shadow_distance
    let fade_start = shadow_distance * 0.8;
    if (view_depth > fade_start) {
        let fade_factor = clamp((view_depth - fade_start) / (shadow_distance - fade_start), 0.0, 1.0);
        return mix(shadow, 1.0, fade_factor);
    }

    return shadow;
}

// Calculate contribution from a single point light
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

// Valve ambient cube (Mitchell et al. 2006): six colours weighted by the squared normal.
// The palette is the same at every azimuth, so +-X and +-Z share the horizon colour.
fn sky_ambient_cube(n: vec3<f32>) -> vec3<f32> {
    let n2 = n * n;
    let vertical = select(lighting.sky_ground.xyz, lighting.sky_zenith.xyz, n.y >= 0.0);
    return (n2.x + n2.z) * lighting.sky_horizon.xyz + n2.y * vertical;
}

// Jimenez et al. 2016 multibounce: bright albedo loses less light in crevices.
fn multibounce_ao(ao: f32, albedo: vec3<f32>) -> vec3<f32> {
    let a = 2.0404 * albedo - 0.3324;
    let b = -4.7951 * albedo + 0.6417;
    let c = 2.7552 * albedo + 0.6903;
    return max(vec3(ao), ((ao * a + b) * ao + c) * ao);
}

// Sky mirrored by a surface with Fresnel reflectance `f0` and roughness `r` (0 = mirror).
// Rough surfaces bend the reflection toward the normal and blur it toward the ambient cube.
// Sky exposure scales it outright, so a covered surface reflects exactly nothing.
fn sky_reflection(
    f0: vec3<f32>,
    r: f32,
    n: vec3<f32>,
    v: vec3<f32>,
    ao: f32,
    sky_exposed: f32,
) -> vec3<f32> {
    let refl = reflect(-v, n);
    let dominant = normalize(mix(refl, n, r * r));
    let env = mix(
        sky_gradient(
            dominant,
            lighting.sky_zenith.xyz,
            lighting.sky_horizon.xyz,
            lighting.sky_ground.xyz
        ),
        sky_ambient_cube(n),
        r
    );
    let fresnel = f0 + (max(vec3(1.0 - r), f0) - f0) * pow(1.0 - saturate(dot(n, v)), 5.0);
    return fresnel * env * mix(1.0, 0.4, r) * sky_exposed * ao;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Sampled ahead of every branch: an implicit-derivative sample must stay in uniform
    // control flow. Alpha is ignored; placed meshes draw opaque.
    let base_texel = textureSample(base_colour_texture, base_colour_sampler, in.uv);

    if (in.world_pos.y > camera.cam_pos.w) {
        discard;
    }

    // Extract lighting parameters from uniform
    let sun_dir = normalize(lighting.sun_direction.xyz);
    let sun_intensity = lighting.sun_direction.w;
    let sun_col = lighting.sun_color.xyz;
    let moon_dir = normalize(lighting.moon_direction.xyz);
    let moon_intensity = lighting.moon_direction.w;
    let moon_col = lighting.moon_color.xyz;
    
    let N = normalize(in.normal);
    let L = sun_dir;
    let diff = max(dot(N, L), 0.0);
    
    // Moon lighting calculation
    let L_moon = moon_dir;
    let diff_moon = max(dot(N, L_moon), 0.0);
    
    // View depth along the camera's forward axis: what the cascade splits are measured in
    let cam_pos_vec = vec3<f32>(camera.cam_pos.x, camera.cam_pos.y, camera.cam_pos.z);
    let view_depth = dot(in.world_pos - cam_pos_vec, shadow_data.camera_forward.xyz);
    
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
    let gloss = select(0.0, clamp(mat.albedo.w, 0.0, 1.0), is_terrain);

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
    let final_albedo = select(albedo, voxel_albedo, is_terrain) * base_texel.rgb;

    // If ref_idx > 0 we treat the material as a dielectric (glass-like).
    // We don't implement true refraction here (no background sampling), but
    // use a Fresnel-based specular term and reduce diffuse contribution so
    // the object appears reflective/transparent rather than fully diffuse.
    var celestial: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);
    var dielectric_fresnel = 0.0;
    // Sky reflection inputs; reflectance stays zero unless a branch below sets them.
    var reflects = false;
    var reflect_f0 = vec3<f32>(0.0);
    var reflect_roughness = 0.0;
    if (ref_idx > 0.0) {
        // Schlick's approximation for Fresnel
        let r0 = (1.0 - ref_idx) / (1.0 + ref_idx);
        let R0 = r0 * r0;
        let cos_theta = max(dot(N, V), 0.0);
        let F = R0 + (1.0 - R0) * pow(1.0 - cos_theta, 5.0);
        dielectric_fresnel = F;
        reflects = true;
        reflect_f0 = vec3<f32>(R0);

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

        celestial = ao * (spec_diel + trans + spec_diel_moon + trans_moon);
        // approximate alpha: more reflective (higher F) -> less transmitted
        // we bias alpha so very slight translucency remains even for weakly
        // refractive materials.
        let alpha = clamp((1.0 - F) * 0.6 + 0.05, 0.02, 1.0);
    } else {
        // Non-dielectric path (Lambertian / Metal)
        // For metals, fuzz represents surface roughness (0 = smooth, 1 = rough)
        // Detect if this is a metal: fuzz > 0.01 indicates metallic material
        let is_metal = step(0.01, fuzz);
        if (is_metal > 0.5) {
            reflects = true;
            reflect_f0 = final_albedo;
            reflect_roughness = clamp(fuzz, 0.0, 1.0);
        } else if (gloss > 0.0) {
            reflects = true;
            reflect_f0 = vec3<f32>(0.04);
            reflect_roughness = 1.0 - gloss;
        }

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

        celestial = ao * (diff_color + spec_color + diff_color_moon + spec_color_moon);
    }
    
    // Ambient from the sky palette by facing. Metals diffuse less, and dielectrics keep a
    // weak ambient that the reflected share (1 - F) cannot exceed.
    let metal_ambient_scale = mix(1.0, 0.3, step(0.01, fuzz));
    let ambient_scale = select(metal_ambient_scale, 0.1 * (1.0 - dielectric_fresnel), ref_idx > 0.0);
    let sky_ambient = final_albedo * sky_ambient_cube(N) * lighting.ambient.w * ambient_scale
        * in.sky_exposed * multibounce_ao(ao, final_albedo);
    // Outside the sky-exposure floor below, so zero exposure reflects exactly nothing.
    var sky_reflected = vec3<f32>(0.0);
    if (reflects) {
        sky_reflected = sky_reflection(reflect_f0, reflect_roughness, N, V, ao, in.sky_exposed);
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
    }
    
    // Sky exposure gates only sun and moon. Sky ambient scales by exposure itself, point
    // lights are not sky light, and block light adds on top so it can light surfaces the
    // sky leaves dark.
    // TODO: Implement proper light occlusion - blocks should prevent light from
    // reaching surfaces in crevices/caves (requires transparency checks in propagation)
    let sky_gate = max(vec3<f32>(in.sky_exposed), vec3<f32>(MIN_SKY_AMBIENT));
    let color = celestial * sky_gate
        + ao * point_light_contrib
        + sky_ambient
        + sky_reflected
        + final_albedo * ao * in.block_light_rgb * BLOCK_LIGHT_GAIN;
    
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
    } else if (debug_mode == 6u) {
        // Mode 6: Sky Ambient alone
        return vec4<f32>(sky_ambient, 1.0);
    } else if (debug_mode == 7u) {
        return vec4<f32>(sky_reflected, 1.0);
    } else if (debug_mode == 8u) {
        // Mode 8: Sun cascades (red, green, blue, yellow), mixed across the blend band
        return vec4<f32>(cascade_debug_colour(pick_cascade(0u, view_depth)), 1.0);
    }

    return vec4<f32>(color, out_alpha);
}
