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
    
    // use camera-provided world position for view direction
    let cam_pos = vec3<f32>(camera.cam_pos.x, camera.cam_pos.y, camera.cam_pos.z);
    let V = normalize(cam_pos - in.world_pos);
    let H = normalize(L + V);
    let spec = pow(max(dot(N, H), 0.0), 64.0);

    // Simple ambient occlusion-like term based on N·L to darken occluded areas
    // and a cheap contact shadow near the ground (y ~= 0) to restore the
    // perception of objects sitting on the ground plane.
    let ao_from_light = clamp(0.3 + 0.7 * diff, 0.0, 1.0);
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
        let spec_diel = vec3<f32>(F) * pow(max(dot(N, H), 0.0), 128.0) * sun_col * sun_intensity;

        // Approximate transmitted light (tint) scaled by (1 - F). This is a
        // cheap stand-in for refraction/transmission and helps the object
        // look glassy when combined with specular.
        let trans = final_albedo * (1.0 - F) * 0.6 * sun_col * sun_intensity;

        // Ambient contribution
        color = ambient_col * ambient_intensity * final_albedo * 0.1 + ao * (spec_diel + trans);
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
        let diff_color = final_albedo * diff * diff_strength * sun_col * sun_intensity;
        
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
        let spec_color = final_albedo * spec_strength * spec_highlight * sun_col * sun_intensity;

        color = ambient_col * ambient_intensity * final_albedo + ao * (diff_color + spec_color);
    }
    // For dielectrics we computed `alpha` above; otherwise alpha is opaque.
    var out_alpha: f32 = 1.0;
    if (ref_idx > 0.0) {
        out_alpha = clamp((1.0 - ( (1.0 - ref_idx) / (1.0 + ref_idx) ) * ((1.0 - ref_idx) / (1.0 + ref_idx))) * 0.6 + 0.05, 0.02, 1.0);
    }
    return vec4<f32>(color, out_alpha);
}
