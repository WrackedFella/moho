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
    
    // Transform to light space for shadow mapping (using light 0 = sun matrix)
    let sm0 = shadow_matrices.light0_m0;
    let sm1 = shadow_matrices.light0_m1;
    let sm2 = shadow_matrices.light0_m2;
    let sm3 = shadow_matrices.light0_m3;
    let sm_r0 = vec4<f32>(sm0.x, sm1.x, sm2.x, sm3.x);
    let sm_r1 = vec4<f32>(sm0.y, sm1.y, sm2.y, sm3.y);
    let sm_r2 = vec4<f32>(sm0.z, sm1.z, sm2.z, sm3.z);
    let sm_r3 = vec4<f32>(sm0.w, sm1.w, sm2.w, sm3.w);
    out.light_space_pos = vec4<f32>(
        dot(sm_r0, world_pos),
        dot(sm_r1, world_pos),
        dot(sm_r2, world_pos),
        dot(sm_r3, world_pos)
    );
    
    // Pass through AO and geometry type
    out.ao = v.ao;
    out.geometry_type = v.geometry_type;
    
    return out;
}
