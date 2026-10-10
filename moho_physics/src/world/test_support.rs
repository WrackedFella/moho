use moho_render_api::WorldMesh;

pub(crate) fn quad_mesh(verts: &[[f32; 3]], idxs: &[u32]) -> WorldMesh {
    let n = verts.len();
    WorldMesh::new(
        verts.to_vec(),
        vec![[0.0, 1.0, 0.0]; n],
        vec![1.0; n],
        vec![[0.0; 3]; n],
        vec![1.0; n],
        vec![0; n],
        idxs.to_vec(),
    )
    .expect("valid quad mesh")
}

pub(crate) fn floor_mesh(y: f32) -> WorldMesh {
    quad_mesh(
        &[
            [-10.0, y, -10.0],
            [10.0, y, -10.0],
            [10.0, y, 10.0],
            [-10.0, y, 10.0],
        ],
        &[0, 2, 1, 0, 3, 2],
    )
}
