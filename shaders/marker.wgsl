// Translucent box markers: a unit cube instanced per marker, scaled to its
// min..max box and drawn over the world without writing depth.

struct Camera {
    vp0: vec4<f32>,
    vp1: vec4<f32>,
    vp2: vec4<f32>,
    vp3: vec4<f32>,
    cam_pos: vec4<f32>,  // w = view-height cut
}

// Lets a marker box whose top face sits on the cut keep its lid.
override marker_inflate: f32;

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) unit_pos: vec3<f32>,
    @location(1) box_min: vec4<f32>,
    @location(2) box_max: vec4<f32>,
    @location(3) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_y: f32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = mix(in.box_min.xyz, in.box_max.xyz, in.unit_pos);
    let view_proj = mat4x4<f32>(camera.vp0, camera.vp1, camera.vp2, camera.vp3);
    out.clip_position = view_proj * vec4<f32>(world_pos, 1.0);
    out.color = in.color;
    out.world_y = world_pos.y;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.world_y > camera.cam_pos.w + marker_inflate) {
        discard;
    }
    return in.color;
}
