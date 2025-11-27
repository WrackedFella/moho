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

// Sample shadow for a specific light using PCF (Percentage Closer Filtering)
// Takes world position and transforms it with the correct light's matrix
fn sample_light_shadow(light_idx: u32, world_pos: vec3<f32>, bias: f32) -> f32 {
    // Transform world position to this light's space
    let light_space_pos = world_to_light_space(world_pos, light_idx);
    
    // Perspective divide
    var proj_coords = light_space_pos.xyz / light_space_pos.w;
    
    // Transform from [-1, 1] to [0, 1] for texture coordinates
    proj_coords = proj_coords * 0.5 + 0.5;
    
    // Flip Y coordinate (texture coordinates are top-left origin)
    proj_coords.y = 1.0 - proj_coords.y;
    
    // Outside shadow map bounds? Return no shadow (fully lit)
    if (proj_coords.x < 0.0 || proj_coords.x > 1.0 ||
        proj_coords.y < 0.0 || proj_coords.y > 1.0 ||
        proj_coords.z < 0.0 || proj_coords.z > 1.0) {
        return 1.0;
    }
    
    // PCF for soft shadows - 3x3 kernel
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
                i32(light_idx),  // Use light index as array layer
                proj_coords.z - bias
            );
        }
    }
    return shadow_sum / 9.0;
}

// Calculate shadow factor for a specific light with distance fade
// Light indices: 0 = Sun, 1 = Moon, 2 = Dynamic1, 3 = Dynamic2
fn calculate_light_shadow(light_idx: u32, world_pos: vec3<f32>, normal: vec3<f32>, light_dir: vec3<f32>, view_depth: f32) -> f32 {
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
    
    // Slope-scale depth bias for shadow acne prevention
    let ndotl = max(dot(normal, light_dir), 0.0);
    let base_bias = 0.0012;
    let slope_bias = 0.0025 * sqrt(1.0 - ndotl * ndotl) / max(ndotl, 0.1);
    let bias = base_bias + slope_bias;
    
    // Sample shadow for this light
    return sample_light_shadow(light_idx, world_pos, bias);
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
    
    // Calculate shadow factors for sun (light 0) and moon (light 1)
    let sun_shadow = calculate_light_shadow(0u, in.world_pos, N, L, view_depth);
    let moon_shadow = calculate_light_shadow(1u, in.world_pos, N, L_moon, view_depth);
    
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

    // Simple ambient occlusion-like term based on N·L to darken occluded areas
    // and a cheap contact shadow near the ground (y ~= 0) to restore the
    // perception of objects sitting on the ground plane.
    // Reduced minimum from 0.3 to 0.05 so back-faces are much darker
    let ao_from_light = clamp(0.05 + 0.95 * diff, 0.0, 1.0);
    let contact = exp(-10.0 * max(in.world_pos.y, 0.0)); // strong near y=0
    let ao = ao_from_light * mix(1.0, 0.6, contact);

    let mat = materials[in.material];
    let albedo = mat.albedo.xyz;
    let fuzz = mat.params.x;
    let ref_idx = mat.params.y;

    // Override albedo based on face normal for voxel terrain
    // Top faces (normal pointing up) = green, Side faces = light brown
    let is_top_face = abs(N.y) > 0.9; // Normal mostly vertical
    let voxel_albedo = select(
        vec3<f32>(0.6, 0.5, 0.4), // Light brown for sides
        vec3<f32>(0.3, 0.6, 0.3), // Green for top
        is_top_face
    );
    let final_albedo = voxel_albedo;

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
    
    // DEBUG: Visualize shadow coordinates (comment out for normal rendering)
    // Uncomment the line below to see shadow map coverage:
    // Red = outside X bounds, Green = outside Y bounds, Blue = outside Z bounds (depth)
    // Color gradient = inside bounds (shows UV coords)
    // return vec4<f32>(debug_shadow_coords(in.light_space_pos), 1.0);
    
    // For dielectrics we computed `alpha` above; otherwise alpha is opaque.
    var out_alpha: f32 = 1.0;
    if (ref_idx > 0.0) {
        out_alpha = clamp((1.0 - ( (1.0 - ref_idx) / (1.0 + ref_idx) ) * ((1.0 - ref_idx) / (1.0 + ref_idx))) * 0.6 + 0.05, 0.02, 1.0);
    }
    return vec4<f32>(color, out_alpha);
}
