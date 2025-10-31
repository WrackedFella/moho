// Shadow mapping shader - renders scene from light's perspective
// Outputs depth values to shadow map texture

struct ShadowMatrix {
    sm0: vec4<f32>,
    sm1: vec4<f32>,
    sm2: vec4<f32>,
    sm3: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> shadow_matrix: ShadowMatrix;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct InstanceInput {
    @location(2) model_col0: vec4<f32>,
    @location(3) model_col1: vec4<f32>,
    @location(4) model_col2: vec4<f32>,
    @location(5) model_col3: vec4<f32>,
    @location(6) material: u32,
    @location(7) object_type: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Transform vertex position by model matrix
    let p = vec4<f32>(vertex.position, 1.0);
    let row0 = vec4<f32>(instance.model_col0.x, instance.model_col1.x, instance.model_col2.x, instance.model_col3.x);
    let row1 = vec4<f32>(instance.model_col0.y, instance.model_col1.y, instance.model_col2.y, instance.model_col3.y);
    let row2 = vec4<f32>(instance.model_col0.z, instance.model_col1.z, instance.model_col2.z, instance.model_col3.z);
    let row3 = vec4<f32>(instance.model_col0.w, instance.model_col1.w, instance.model_col2.w, instance.model_col3.w);
    let world_pos = vec4<f32>(dot(row0, p), dot(row1, p), dot(row2, p), dot(row3, p));
    
    // Transform to light space using shadow matrix
    let sm0 = shadow_matrix.sm0;
    let sm1 = shadow_matrix.sm1;
    let sm2 = shadow_matrix.sm2;
    let sm3 = shadow_matrix.sm3;
    let r0 = vec4<f32>(sm0.x, sm1.x, sm2.x, sm3.x);
    let r1 = vec4<f32>(sm0.y, sm1.y, sm2.y, sm3.y);
    let r2 = vec4<f32>(sm0.z, sm1.z, sm2.z, sm3.z);
    let r3 = vec4<f32>(sm0.w, sm1.w, sm2.w, sm3.w);
    
    out.clip_position = vec4<f32>(
        dot(r0, world_pos),
        dot(r1, world_pos),
        dot(r2, world_pos),
        dot(r3, world_pos)
    );
    
    return out;
}

// No fragment shader needed - depth-only pass
