// Shadow mapping shader - renders scene from light's perspective
// Outputs depth values to shadow map texture
// Multi-light: Updated to support 4 light sources (Sun, Moon, 2 Dynamic)

// Multi-light shadow matrix buffer (288 bytes total for 4 lights)
struct MultiLightShadowMatrix {
    // Light 0 (Sun) matrix
    light0_m0: vec4<f32>,
    light0_m1: vec4<f32>,
    light0_m2: vec4<f32>,
    light0_m3: vec4<f32>,
    // Light 1 (Moon) matrix
    light1_m0: vec4<f32>,
    light1_m1: vec4<f32>,
    light1_m2: vec4<f32>,
    light1_m3: vec4<f32>,
    // Light 2 (Dynamic 1) matrix
    light2_m0: vec4<f32>,
    light2_m1: vec4<f32>,
    light2_m2: vec4<f32>,
    light2_m3: vec4<f32>,
    // Light 3 (Dynamic 2) matrix
    light3_m0: vec4<f32>,
    light3_m1: vec4<f32>,
    light3_m2: vec4<f32>,
    light3_m3: vec4<f32>,
    // Light intensities: x=sun, y=moon, z=dyn1, w=dyn2
    light_intensities: vec4<f32>,
    // Metadata: x=shadow_distance, y=reserved, z=reserved, w=reserved
    metadata: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> shadow_matrices: MultiLightShadowMatrix;

// Immediate data for light index (replaces cascade_index; wgpu 29 renamed
// push constants to "immediates")
struct PushConstants {
    light_index: u32,
}

var<immediate> pc: PushConstants;

// Helper function to get matrix columns for a specific light
fn get_light_matrix(index: u32) -> array<vec4<f32>, 4> {
    var result: array<vec4<f32>, 4>;
    if (index == 0u) {
        // Sun matrix
        result[0] = shadow_matrices.light0_m0;
        result[1] = shadow_matrices.light0_m1;
        result[2] = shadow_matrices.light0_m2;
        result[3] = shadow_matrices.light0_m3;
    } else if (index == 1u) {
        // Moon matrix
        result[0] = shadow_matrices.light1_m0;
        result[1] = shadow_matrices.light1_m1;
        result[2] = shadow_matrices.light1_m2;
        result[3] = shadow_matrices.light1_m3;
    } else if (index == 2u) {
        // Dynamic light 1 matrix
        result[0] = shadow_matrices.light2_m0;
        result[1] = shadow_matrices.light2_m1;
        result[2] = shadow_matrices.light2_m2;
        result[3] = shadow_matrices.light2_m3;
    } else {
        // Dynamic light 2 matrix (index == 3u)
        result[0] = shadow_matrices.light3_m0;
        result[1] = shadow_matrices.light3_m1;
        result[2] = shadow_matrices.light3_m2;
        result[3] = shadow_matrices.light3_m3;
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
    
    // Transform to light space using selected light's matrix
    let light_mat = get_light_matrix(pc.light_index);
    let sm0 = light_mat[0];
    let sm1 = light_mat[1];
    let sm2 = light_mat[2];
    let sm3 = light_mat[3];
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
