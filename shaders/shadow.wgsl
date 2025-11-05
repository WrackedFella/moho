// Shadow mapping shader - renders scene from light's perspective
// Outputs depth values to shadow map texture
// CSM Phase 3: Updated to support cascaded shadow maps

// Cascaded shadow matrix buffer (272 bytes total)
struct CascadedShadowMatrix {
    // Cascade 0 matrix (we'll only use this one for Phase 3)
    cascade0_m0: vec4<f32>,
    cascade0_m1: vec4<f32>,
    cascade0_m2: vec4<f32>,
    cascade0_m3: vec4<f32>,
    // Cascade 1 matrix
    cascade1_m0: vec4<f32>,
    cascade1_m1: vec4<f32>,
    cascade1_m2: vec4<f32>,
    cascade1_m3: vec4<f32>,
    // Cascade 2 matrix
    cascade2_m0: vec4<f32>,
    cascade2_m1: vec4<f32>,
    cascade2_m2: vec4<f32>,
    cascade2_m3: vec4<f32>,
    // Cascade 3 matrix
    cascade3_m0: vec4<f32>,
    cascade3_m1: vec4<f32>,
    cascade3_m2: vec4<f32>,
    cascade3_m3: vec4<f32>,
    // Split distances
    split_distances: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> csm_matrix: CascadedShadowMatrix;

// Push constant for cascade index (Phase 4)
struct PushConstants {
    cascade_index: u32,
}

var<push_constant> pc: PushConstants;

// Helper function to get matrix columns for a specific cascade
fn get_cascade_matrix(index: u32) -> array<vec4<f32>, 4> {
    var result: array<vec4<f32>, 4>;
    if (index == 0u) {
        result[0] = csm_matrix.cascade0_m0;
        result[1] = csm_matrix.cascade0_m1;
        result[2] = csm_matrix.cascade0_m2;
        result[3] = csm_matrix.cascade0_m3;
    } else if (index == 1u) {
        result[0] = csm_matrix.cascade1_m0;
        result[1] = csm_matrix.cascade1_m1;
        result[2] = csm_matrix.cascade1_m2;
        result[3] = csm_matrix.cascade1_m3;
    } else if (index == 2u) {
        result[0] = csm_matrix.cascade2_m0;
        result[1] = csm_matrix.cascade2_m1;
        result[2] = csm_matrix.cascade2_m2;
        result[3] = csm_matrix.cascade2_m3;
    } else {
        result[0] = csm_matrix.cascade3_m0;
        result[1] = csm_matrix.cascade3_m1;
        result[2] = csm_matrix.cascade3_m2;
        result[3] = csm_matrix.cascade3_m3;
    }
    return result;
}

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
    
    // Transform to light space using selected cascade matrix (Phase 4: all cascades)
    let cascade_mat = get_cascade_matrix(pc.cascade_index);
    let sm0 = cascade_mat[0];
    let sm1 = cascade_mat[1];
    let sm2 = cascade_mat[2];
    let sm3 = cascade_mat[3];
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
