// Skybox shader for rendering atmospheric gradient and sun disk
// Uses a simple gradient from zenith to horizon with sun disk overlay

struct Camera {
    vp0: vec4<f32>,
    vp1: vec4<f32>,
    vp2: vec4<f32>,
    vp3: vec4<f32>,
    cam_pos: vec4<f32>,
}

struct Lighting {
    sun_direction: vec4<f32>,  // xyz = direction (normalized), w = intensity
    sun_color: vec4<f32>,      // xyz = color, w = unused
    moon_direction: vec4<f32>, // xyz = direction (normalized), w = intensity
    moon_color: vec4<f32>,     // xyz = color, w = unused
    ambient: vec4<f32>,        // w = sky-ambient strength, xyz = unused
    time_of_day: vec4<f32>,    // x = 0-24 hours, yzw = unused
    sky_zenith: vec4<f32>,     // xyz = colour, w = unused
    sky_horizon: vec4<f32>,    // xyz = colour, w = unused
    sky_ground: vec4<f32>,     // xyz = colour, w = unused
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(2)
var<uniform> lighting: Lighting;

struct VertexInput {
    @location(0) position: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Render fullscreen quad directly in clip space at far plane depth
    out.clip_position = vec4<f32>(in.position.xy, 0.999999, 1.0);
    
    // Compute inverse view-projection to transform clip space to world direction
    // Build the view-projection matrix
    let vp = mat4x4<f32>(
        camera.vp0,
        camera.vp1,
        camera.vp2,
        camera.vp3
    );
    
    // We need the inverse to go from clip space to world space
    // For a perspective projection, we can compute the inverse
    let inv_vp = inverse_mat4(vp);
    
    // Transform the clip position (at far plane) to world space
    let world_pos_far = inv_vp * vec4<f32>(in.position.xy, 1.0, 1.0);
    let world_pos_near = inv_vp * vec4<f32>(in.position.xy, 0.0, 1.0);
    
    // Compute view direction from near to far plane
    out.world_pos = normalize((world_pos_far.xyz / world_pos_far.w) - (world_pos_near.xyz / world_pos_near.w));
    
    return out;
}

// 4x4 matrix inverse (needed for view-projection inverse)
fn inverse_mat4(m: mat4x4<f32>) -> mat4x4<f32> {
    let a00 = m[0][0]; let a01 = m[0][1]; let a02 = m[0][2]; let a03 = m[0][3];
    let a10 = m[1][0]; let a11 = m[1][1]; let a12 = m[1][2]; let a13 = m[1][3];
    let a20 = m[2][0]; let a21 = m[2][1]; let a22 = m[2][2]; let a23 = m[2][3];
    let a30 = m[3][0]; let a31 = m[3][1]; let a32 = m[3][2]; let a33 = m[3][3];

    let b00 = a00 * a11 - a01 * a10;
    let b01 = a00 * a12 - a02 * a10;
    let b02 = a00 * a13 - a03 * a10;
    let b03 = a01 * a12 - a02 * a11;
    let b04 = a01 * a13 - a03 * a11;
    let b05 = a02 * a13 - a03 * a12;
    let b06 = a20 * a31 - a21 * a30;
    let b07 = a20 * a32 - a22 * a30;
    let b08 = a20 * a33 - a23 * a30;
    let b09 = a21 * a32 - a22 * a31;
    let b10 = a21 * a33 - a23 * a31;
    let b11 = a22 * a33 - a23 * a32;

    let det = b00 * b11 - b01 * b10 + b02 * b09 + b03 * b08 - b04 * b07 + b05 * b06;
    let inv_det = 1.0 / det;

    return mat4x4<f32>(
        vec4<f32>(
            (a11 * b11 - a12 * b10 + a13 * b09) * inv_det,
            (a02 * b10 - a01 * b11 - a03 * b09) * inv_det,
            (a31 * b05 - a32 * b04 + a33 * b03) * inv_det,
            (a22 * b04 - a21 * b05 - a23 * b03) * inv_det
        ),
        vec4<f32>(
            (a12 * b08 - a10 * b11 - a13 * b07) * inv_det,
            (a00 * b11 - a02 * b08 + a03 * b07) * inv_det,
            (a32 * b02 - a30 * b05 - a33 * b01) * inv_det,
            (a20 * b05 - a22 * b02 + a23 * b01) * inv_det
        ),
        vec4<f32>(
            (a10 * b10 - a11 * b08 + a13 * b06) * inv_det,
            (a01 * b08 - a00 * b10 - a03 * b06) * inv_det,
            (a30 * b04 - a31 * b02 + a33 * b00) * inv_det,
            (a21 * b02 - a20 * b04 - a23 * b00) * inv_det
        ),
        vec4<f32>(
            (a11 * b07 - a10 * b09 - a12 * b06) * inv_det,
            (a00 * b09 - a01 * b07 + a02 * b06) * inv_det,
            (a31 * b01 - a30 * b03 - a32 * b00) * inv_det,
            (a20 * b03 - a21 * b01 + a22 * b00) * inv_det
        )
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_pos);

    // Sky colours come from the lighting uniform, shared with the ambient light
    var sky_color = sky_gradient(
        dir,
        lighting.sky_zenith.xyz,
        lighting.sky_horizon.xyz,
        lighting.sky_ground.xyz
    );

    // Add sun disk
    let sun_dir = normalize(lighting.sun_direction.xyz);
    let sun_col = lighting.sun_color.xyz;
    let sun_intensity = lighting.sun_direction.w;
    
    let sun_dot = dot(dir, sun_dir);
    
    // Sun disk (sharp edge)
    let sun_size = 0.9998; // cos(~1.1°) - realistic sun size (real sun is ~0.5°)
    let sun_glow_size = 0.998; // Tighter glow around smaller sun
    
    if (sun_dot > sun_size) {
        // Bright sun disk
        sky_color = sun_col * sun_intensity * 2.5;
    } else if (sun_dot > sun_glow_size) {
        // Sun glow/corona
        let glow_factor = smoothstep(sun_glow_size, sun_size, sun_dot);
        let glow = sun_col * sun_intensity * glow_factor * 1.5;
        sky_color = sky_color + glow;
    }
    
    // Atmospheric scattering effect near sun (subtle)
    let scatter_factor = max(0.0, sun_dot);
    let scatter = pow(scatter_factor, 8.0) * 0.3 * sun_col;
    sky_color = sky_color + scatter;
    
    // Add moon disk
    let moon_dir = normalize(lighting.moon_direction.xyz);
    let moon_col = lighting.moon_color.xyz;
    let moon_intensity = lighting.moon_direction.w;
    
    let moon_dot = dot(dir, moon_dir);
    
    // Moon disk (same size as sun for simplicity)
    let moon_size = 0.9998; // cos(~1.1°) - same as sun
    let moon_glow_size = 0.998; // Subtle glow around moon
    
    if (moon_dot > moon_size) {
        // Bright moon disk (softer than sun)
        let moon_brightness = moon_col * moon_intensity * 1.5;
        sky_color = sky_color + moon_brightness;
    } else if (moon_dot > moon_glow_size) {
        // Moon glow/corona (more subtle than sun)
        let glow_factor = smoothstep(moon_glow_size, moon_size, moon_dot);
        let glow = moon_col * moon_intensity * glow_factor * 0.8;
        sky_color = sky_color + glow;
    }
    
    // Atmospheric scattering effect near moon (very subtle)
    let moon_scatter_factor = max(0.0, moon_dot);
    let moon_scatter = pow(moon_scatter_factor, 12.0) * 0.15 * moon_col;
    sky_color = sky_color + moon_scatter;
    
    return vec4<f32>(sky_color, 1.0);
}
