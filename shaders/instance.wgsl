struct Camera {
    vp0: vec4<f32>,
    vp1: vec4<f32>,
    vp2: vec4<f32>,
    vp3: vec4<f32>,
    cam_pos: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct InstanceIn {
    @location(2) model_col0: vec4<f32>,
    @location(3) model_col1: vec4<f32>,
    @location(4) model_col2: vec4<f32>,
    @location(5) model_col3: vec4<f32>,
    @location(6) material: u32,
    @location(7) object_type: u32,
}

struct Material {
    albedo: vec3<f32>,
    fuzz: f32,
    ref_idx: f32,
    _pad: f32,
}

@group(0) @binding(1)
var<storage, read> materials: array<Material>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) material: u32,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
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
    // transform vertex normal (w=0) by model matrix rows
    let nvec = vec4<f32>(v.normal, 0.0);
    let n_ws = vec3<f32>(dot(row0, nvec), dot(row1, nvec), dot(row2, nvec));
    out.normal = normalize(n_ws);
    out.world_pos = vec3<f32>(world_pos.x, world_pos.y, world_pos.z);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Lighting params
    let light_dir = normalize(vec3<f32>(1.0, 1.0, 0.5));
    // reduce ambient to increase contrast and perceived shading
    let ambient = 0.03;
    let N = normalize(in.normal);
    let L = normalize(light_dir);
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

    var color: vec3<f32> = vec3<f32>(0.0);
    let mat = materials[in.material];
    if (in.material == 0u) {
        // Lambertian: diffuse + small specular
        color = mat.albedo * (ambient + (1.0 - ambient) * ao * diff) + vec3<f32>(spec * 0.05);
    } else if (in.material == 1u) {
        // Metal: specular-dominant, modulated by fuzz
        let metal_factor = 1.0 - clamp(mat.fuzz, 0.0, 1.0);
        color = mat.albedo * (ambient * 0.15 + 0.85 * ao * diff * metal_factor) + vec3<f32>(spec * (0.9 * metal_factor));
    } else if (in.material == 2u) {
        // Dielectric: tinted base + reduced diffuse response
        let t = clamp((mat.ref_idx - 1.0) * 0.25, 0.0, 1.0);
        let base = mix(mat.albedo, vec3<f32>(0.8, 0.9, 1.0), t);
        color = base * (ambient + (1.0 - ambient) * ao * diff * 0.5) + vec3<f32>(spec * 0.25);
    } else {
        color = mat.albedo;
    }
    color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(color, 1.0);
}
