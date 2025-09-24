struct Camera {
    vp0: vec4<f32>,
    vp1: vec4<f32>,
    vp2: vec4<f32>,
    vp3: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexIn {
    @location(0) position: vec3<f32>,
}

struct InstanceIn {
    @location(1) model_col0: vec4<f32>,
    @location(2) model_col1: vec4<f32>,
    @location(3) model_col2: vec4<f32>,
    @location(4) model_col3: vec4<f32>,
    @location(5) material: u32,
    @location(6) object_type: u32,
    @location(7) albedo: vec3<f32>,
    @location(8) fuzz: f32,
    @location(9) ref_idx: f32,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) material: u32,
    @location(1) albedo: vec3<f32>,
    @location(2) fuzz: f32,
    @location(3) ref_idx: f32,
}

@vertex
fn vs_main(v: VertexIn, i: InstanceIn) -> VsOut {
    var out: VsOut;
    let p = vec4<f32>(v.position, 1.0);
    let row0 = vec4<f32>(i.model_col0.x, i.model_col1.x, i.model_col2.x, i.model_col3.x);
    let row1 = vec4<f32>(i.model_col0.y, i.model_col1.y, i.model_col2.y, i.model_col3.y);
    let row2 = vec4<f32>(i.model_col0.z, i.model_col1.z, i.model_col2.z, i.model_col3.z);
    let row3 = vec4<f32>(i.model_col0.w, i.model_col1.w, i.model_col2.w, i.model_col3.w);
    let world_pos = vec4<f32>(dot(row0, p), dot(row1, p), dot(row2, p), dot(row3, p));
    // camera vp are columns; reconstruct rows then multiply
    let vp0 = camera.vp0; 
    let vp1 = camera.vp1; 
    let vp2 = camera.vp2; 
    let vp3 = camera.vp3;
    let r0 = vec4<f32>(vp0.x, vp1.x, vp2.x, vp3.x);
    let r1 = vec4<f32>(vp0.y, vp1.y, vp2.y, vp3.y);
    let r2 = vec4<f32>(vp0.z, vp1.z, vp2.z, vp3.z);
    let r3 = vec4<f32>(vp0.w, vp1.w, vp2.w, vp3.w);
    out.clip = vec4<f32>(dot(r0, world_pos), dot(r1, world_pos), dot(r2, world_pos), dot(r3, world_pos));
    out.material = i.material;
    out.albedo = i.albedo;
    out.fuzz = i.fuzz;
    out.ref_idx = i.ref_idx;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Simple material-based shading:
    // material == 0 -> Lambertian: use albedo
    // material == 1 -> Metal: use albedo modulated by (1 - fuzz)
    // material == 2 -> Dielectric: tint by ref_idx (simple approximation)
    var color: vec3<f32> = vec3<f32>(0.0);
    if (in.material == 0u) {
        color = in.albedo;
    } else if (in.material == 1u) {
        let metal_factor = 1.0 - clamp(in.fuzz, 0.0, 1.0);
        color = in.albedo * metal_factor;
    } else if (in.material == 2u) {
        // tint glass slightly by ref_idx: this is not physically correct but
        // provides visual differentiation
        let t = clamp((in.ref_idx - 1.0) * 0.5, 0.0, 1.0);
        color = mix(in.albedo, vec3<f32>(0.8, 0.9, 1.0), t);
    } else {
        color = in.albedo;
    }
    return vec4<f32>(color, 1.0);
}
