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

// CSM Phase 4: Select appropriate cascade based on view-space depth
// Returns cascade index (0-3) based on split distances
fn select_cascade(view_depth: f32) -> u32 {
    // Cascade split distances: [20, 50, 100, 200] units
    if (view_depth < 20.0) {
        return 0u; // Closest cascade (highest detail)
    } else if (view_depth < 50.0) {
        return 1u;
    } else if (view_depth < 100.0) {
        return 2u;
    } else {
        return 3u; // Farthest cascade (lowest detail, largest area)
    }
}

// Calculate shadow factor using PCF (Percentage Closer Filtering) with CSM
// Phase 4: Selects appropriate cascade based on fragment depth
fn calculate_shadow(light_space_pos: vec4<f32>, normal: vec3<f32>, light_dir: vec3<f32>, view_depth: f32) -> f32 {
    // Select cascade based on view-space depth
    let cascade_index = select_cascade(view_depth);
    
    // Perspective divide
    var proj_coords = light_space_pos.xyz / light_space_pos.w;
    
    // Transform from [-1, 1] to [0, 1] for texture coordinates
    proj_coords = proj_coords * 0.5 + 0.5;
    
    // Flip Y coordinate (texture coordinates are top-left origin)
    proj_coords.y = 1.0 - proj_coords.y;
    
    // Outside shadow map bounds? Full light
    if (proj_coords.x < 0.0 || proj_coords.x > 1.0 ||
        proj_coords.y < 0.0 || proj_coords.y > 1.0 ||
        proj_coords.z < 0.0 || proj_coords.z > 1.0) {
        return 1.0;
    }
    
    // Slope-scale depth bias for shadow acne prevention
    let ndotl = max(dot(normal, light_dir), 0.0);
    let base_bias = 0.0012;
    let slope_bias = 0.0025 * sqrt(1.0 - ndotl * ndotl) / max(ndotl, 0.1);
    let bias = base_bias + slope_bias;
    let biased_depth = proj_coords.z - bias;
    
    // PCF for soft shadows - 3x3 kernel
    let texel_size = 1.0 / 4096.0; // Shadow map size (4096x4096 per cascade)
    var shadow = 0.0;
    let pcf_radius = 1.0;
    
    // 3x3 kernel - sample from selected cascade layer
    for (var x = -1.0; x <= 1.0; x += 1.0) {
        for (var y = -1.0; y <= 1.0; y += 1.0) {
            let offset = vec2<f32>(x, y) * texel_size * pcf_radius;
            let sample_coords = proj_coords.xy + offset;
            // Sample from cascade array texture at selected layer
            shadow += textureSampleCompareLevel(shadow_map, shadow_sampler, sample_coords, i32(cascade_index), biased_depth);
        }
    }
    shadow /= 9.0;
    
    return shadow; // 0.0 = full shadow, 1.0 = full light
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Extract lighting parameters from uniform
    let sun_dir = normalize(lighting.sun_direction.xyz);
    let sun_intensity = lighting.sun_direction.w;
    let sun_col = lighting.sun_color.xyz;
    let ambient_col = lighting.ambient.xyz;
    let ambient_intensity = lighting.ambient.w;
    
    let N = normalize(in.normal);
    let L = sun_dir;
    let diff = max(dot(N, L), 0.0);
    
    // Calculate view-space depth for cascade selection (Phase 4: CSM)
    let cam_pos_vec = vec3<f32>(camera.cam_pos.x, camera.cam_pos.y, camera.cam_pos.z);
    let view_depth = length(in.world_pos - cam_pos_vec);
    
    // Calculate shadow factor with CSM cascade selection
    let shadow = calculate_shadow(in.light_space_pos, N, L, view_depth);
    
    // CSM Phase 4: Debug visualization - color-code cascades
    // Set to false to disable, true to see cascade bands
    let csm_debug = false; // Disabled - using proper CSM shadows
    if (csm_debug) {
        let cascade_idx = select_cascade(view_depth);
        var cascade_color = vec3<f32>(1.0, 1.0, 1.0); // White fallback
        if (cascade_idx == 0u) {
            cascade_color = vec3<f32>(1.0, 0.0, 0.0); // Red: 0-20 units
        } else if (cascade_idx == 1u) {
            cascade_color = vec3<f32>(0.0, 1.0, 0.0); // Green: 20-50 units
        } else if (cascade_idx == 2u) {
            cascade_color = vec3<f32>(0.0, 0.0, 1.0); // Blue: 50-100 units
        } else {
            cascade_color = vec3<f32>(1.0, 1.0, 0.0); // Yellow: 100-200 units
        }
        return vec4<f32>(cascade_color, 1.0);
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
        // Apply shadow to direct lighting only
        let spec_diel = vec3<f32>(F) * pow(max(dot(N, H), 0.0), 128.0) * sun_col * sun_intensity * shadow;

        // Approximate transmitted light (tint) scaled by (1 - F). This is a
        // cheap stand-in for refraction/transmission and helps the object
        // look glassy when combined with specular.
        let trans = final_albedo * (1.0 - F) * 0.6 * sun_col * sun_intensity * shadow;

        // Reduce ambient in shadowed areas AND on back-facing surfaces
        let shadow_darkening = mix(0.2, 1.0, shadow);
        let backface_darkening = mix(0.15, 1.0, diff);
        let total_darkening = shadow_darkening * backface_darkening;
        color = ambient_col * ambient_intensity * final_albedo * 0.1 * total_darkening + ao * (spec_diel + trans);
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
        // Apply shadow to diffuse lighting
        let diff_color = final_albedo * diff * diff_strength * sun_col * sun_intensity * shadow;
        
        // Metal specular: strong but affected by fuzz (roughness)
        // Lambertian specular: very weak
        let base_spec_strength = mix(0.04, 0.85, is_metal);
        // Fuzz reduces specular strength for metals (rougher = less reflective)
        let fuzz_factor = mix(1.0, 0.4, clamp(fuzz, 0.0, 1.0));
        let spec_strength = base_spec_strength * fuzz_factor;
        
        // Sharper specular for metals (higher power)
        let spec_power = mix(64.0, 96.0, is_metal);
        let spec_highlight = pow(max(dot(N, H), 0.0), spec_power);
        
        // For metals the specular should be tinted by albedo and sun color
        // Apply shadow to specular as well
        let spec_color = final_albedo * spec_strength * spec_highlight * sun_col * sun_intensity * shadow;

        // Reduce ambient in shadowed areas AND on back-facing surfaces
        // Surfaces in complete shadow should be much darker
        // Also darken surfaces facing away from light (diff close to 0)
        let shadow_darkening = mix(0.2, 1.0, shadow); // Darken by 80% in full shadow
        let backface_darkening = mix(0.15, 1.0, diff); // Darken by 85% when facing away
        let total_darkening = shadow_darkening * backface_darkening;
        color = ambient_col * ambient_intensity * final_albedo * total_darkening + ao * (diff_color + spec_color);
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
