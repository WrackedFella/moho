use crate::movement::MoveSettings;
use moho_render_api::WorldMesh;

/// Tuning where ground accel x dt exceeds the friction drop, so "at speed" means 4 m/s.
pub(crate) fn test_settings() -> MoveSettings {
    MoveSettings {
        max_speed: 4.0,
        ground_accel: 40.0,
        friction: 4.0,
        stop_speed: 1.0,
        air_accel: 10.0,
        air_speed_cap: 0.8,
        gravity: 18.0,
        jump_speed: 8.0,
    }
}

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

/// A wall in the plane `z`, spanning x in [-10, 10] and y in [0, 10], facing -Z.
pub(crate) fn wall_mesh(z: f32) -> WorldMesh {
    quad_mesh(
        &[
            [-10.0, 0.0, z],
            [10.0, 0.0, z],
            [10.0, 10.0, z],
            [-10.0, 10.0, z],
        ],
        &[0, 2, 1, 0, 3, 2],
    )
}

/// A closed box spanning x in [-10, 10], y in [0, `height`] and z in [`z_front`, `z_front + depth`].
pub(crate) fn step_box_mesh(z_front: f32, depth: f32, height: f32) -> WorldMesh {
    let z_back = z_front + depth;
    quad_mesh(
        &[
            [-10.0, 0.0, z_front],
            [10.0, 0.0, z_front],
            [10.0, height, z_front],
            [-10.0, height, z_front],
            [-10.0, 0.0, z_back],
            [10.0, 0.0, z_back],
            [10.0, height, z_back],
            [-10.0, height, z_back],
        ],
        &[
            0, 2, 1, 0, 3, 2, // front riser
            4, 5, 6, 4, 6, 7, // back
            3, 7, 6, 3, 6, 2, // top
            0, 4, 7, 0, 7, 3, // left
            1, 2, 6, 1, 6, 5, // right
            0, 1, 5, 0, 5, 4, // bottom
        ],
    )
}
