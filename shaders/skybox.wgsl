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
    ambient: vec4<f32>,        // xyz = color, w = intensity
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
    
    // Get camera position
    let cam_pos = vec3<f32>(camera.cam_pos.x, camera.cam_pos.y, camera.cam_pos.z);
    
    // Position skybox around camera (large sphere centered on camera)
    let world_pos = in.position * 1000.0 + cam_pos;
    
    // Transform to clip space
    let wp = vec4<f32>(world_pos, 1.0);
    let vp0 = camera.vp0;
    let vp1 = camera.vp1;
    let vp2 = camera.vp2;
    let vp3 = camera.vp3;
    let r0 = vec4<f32>(vp0.x, vp1.x, vp2.x, vp3.x);
    let r1 = vec4<f32>(vp0.y, vp1.y, vp2.y, vp3.y);
    let r2 = vec4<f32>(vp0.z, vp1.z, vp2.z, vp3.z);
    let r3 = vec4<f32>(vp0.w, vp1.w, vp2.w, vp3.w);
    
    let clip = vec4<f32>(
        dot(r0, wp),
        dot(r1, wp),
        dot(r2, wp),
        dot(r3, wp)
    );
    
    // Set z = w to ensure skybox is at maximum depth
    out.clip_position = vec4<f32>(clip.xy, clip.w, clip.w);
    out.world_pos = normalize(in.position);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_pos);
    
    // Sky gradient from zenith to horizon
    let zenith_color = vec3<f32>(0.2, 0.4, 0.8);    // Blue sky
    let horizon_color = vec3<f32>(0.7, 0.8, 0.9);    // Light blue/white horizon
    let ground_color = vec3<f32>(0.3, 0.3, 0.35);    // Dark gray ground
    
    // Interpolate based on vertical direction
    let t = dir.y; // -1 (down) to 1 (up)
    
    var sky_color: vec3<f32>;
    if (t > 0.0) {
        // Above horizon: blend from horizon to zenith
        let blend = smoothstep(0.0, 0.6, t);
        sky_color = mix(horizon_color, zenith_color, blend);
    } else {
        // Below horizon: fade to ground color
        let blend = smoothstep(0.0, -0.3, t);
        sky_color = mix(horizon_color, ground_color, blend);
    }
    
    // Add sun disk
    let sun_dir = normalize(lighting.sun_direction.xyz);
    let sun_col = lighting.sun_color.xyz;
    let sun_intensity = lighting.sun_direction.w;
    
    let sun_dot = dot(dir, sun_dir);
    
    // Sun disk (sharp edge)
    let sun_size = 0.998; // cos(angle) - smaller = bigger sun
    let sun_glow_size = 0.95; // Larger glow around sun
    
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
    
    return vec4<f32>(sky_color, 1.0);
}
