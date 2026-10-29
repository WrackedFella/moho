// Shadow-map lookup: projection into a cascade layer and the fixed-grid PCF filter.
// Every pixel uses the same tap pattern, so the filter adds no noise and an edge does not crawl.

// Widest filter radius in texels; `metadata.z` is clamped to it.
const SHADOW_MAX_FILTER_RADIUS: f32 = 3.0;
// Floor of |n.(a x b)| as a fraction of |a x b|: bounds the receiver-plane gradient of a
// surface within about 84 degrees of edge-on to the light.
const PLANE_BIAS_MIN_COS: f32 = 0.1;

// Light-depth change per texel step in u and v across the receiver's plane (normal `n`), so a
// tap away from the centre compares at the receiver's own depth there instead of acne.
// Built from the layer matrix, not screen derivatives: it runs inside the divergent cascade pick.
fn shadow_plane_gradient(layer: u32, n: vec3<f32>) -> vec2<f32> {
    let m = shadow_data.matrices[layer];
    // World-space gradients of u, v (flipped: texture y points down) and depth.
    let a = vec3<f32>(m[0][0], m[1][0], m[2][0]) * 0.5;
    let b = vec3<f32>(m[0][1], m[1][1], m[2][1]) * -0.5;
    let c = vec3<f32>(m[0][2], m[1][2], m[2][2]);

    let ab = cross(a, b);
    let raw = dot(n, ab);
    let floor_mag = PLANE_BIAS_MIN_COS * length(ab);
    let denom = select(-1.0, 1.0, raw >= 0.0) * max(abs(raw), floor_mag);

    let map_size = f32(textureDimensions(shadow_map).x);
    return vec2<f32>(dot(n, cross(c, b)), dot(n, cross(a, c))) / (denom * map_size);
}

// Tent-weighted (2r+1)^2 grid of hardware 2x2 compares at integer texel offsets; r = 0 is one tap.
fn shadow_pcf(layer: u32, uv: vec2<f32>, z_ref: f32, plane_grad: vec2<f32>, radius: i32) -> f32 {
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_map));
    var sum = 0.0;
    var weight_sum = 0.0;
    for (var y = -radius; y <= radius; y++) {
        for (var x = -radius; x <= radius; x++) {
            let offset = vec2<f32>(f32(x), f32(y));
            let weight = f32(radius + 1 - abs(x)) * f32(radius + 1 - abs(y));
            let depth = clamp(z_ref + dot(plane_grad, offset), 0.0, 1.0);
            sum += weight * textureSampleCompareLevel(
                shadow_map,
                shadow_sampler,
                uv + offset * texel,
                i32(layer),
                depth
            );
            weight_sum += weight;
        }
    }
    return sum / weight_sum;
}

// Shadow factor (1 = lit) of `world_pos` in one cascade layer; `normal` is the geometric normal.
fn sample_light_shadow(layer: u32, world_pos: vec3<f32>, normal: vec3<f32>, bias: f32) -> f32 {
    let light_space_pos = shadow_data.matrices[layer] * vec4<f32>(world_pos, 1.0);
    var proj_coords = light_space_pos.xyz / light_space_pos.w;

    // XY are in clip space (-1..1) with y up; Z is already 0..1 (the cascade projections are zero-to-one).
    proj_coords.x = proj_coords.x * 0.5 + 0.5;
    proj_coords.y = 1.0 - (proj_coords.y * 0.5 + 0.5);

    if (proj_coords.x < 0.0 || proj_coords.x > 1.0 ||
        proj_coords.y < 0.0 || proj_coords.y > 1.0 ||
        proj_coords.z < 0.0 || proj_coords.z > 1.0) {
        return 1.0;
    }

    let radius = i32(clamp(shadow_data.metadata.z, 0.0, SHADOW_MAX_FILTER_RADIUS));
    let plane_grad = shadow_plane_gradient(layer, normal);
    return shadow_pcf(layer, proj_coords.xy, proj_coords.z - bias, plane_grad, radius);
}
