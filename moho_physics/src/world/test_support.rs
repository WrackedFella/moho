use crate::movement::MoveSettings;
use glam::Vec3;
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
        max_velocity: 90.0,
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

/// A plane spanning x in [-10, 10] that runs from height `y_from` at `z_from` to height
/// `y_to` at `z_to`, facing up. Equal heights make level ground.
pub(crate) fn slope_mesh(z_from: f32, y_from: f32, z_to: f32, y_to: f32) -> WorldMesh {
    quad_mesh(
        &[
            [-10.0, y_from, z_from],
            [10.0, y_from, z_from],
            [10.0, y_to, z_to],
            [-10.0, y_to, z_to],
        ],
        &[0, 2, 1, 0, 3, 2],
    )
}

/// Level ground at y = 0 made of two meshes, one for z in [-10, -`seam`/2] and one for z in
/// [`seam`/2, 10], leaving a `seam` m wide gap along z = 0.
pub(crate) fn seam_floor_meshes(seam: f32) -> [WorldMesh; 2] {
    [
        slope_mesh(-10.0, 0.0, -seam / 2.0, 0.0),
        slope_mesh(seam / 2.0, 0.0, 10.0, 0.0),
    ]
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

/// Length in metres of each wall of [`corner_mesh`].
pub(crate) const CORNER_WALL_LENGTH: f32 = 10.0;

/// An inside corner of two walls, each [`CORNER_WALL_LENGTH`] long and spanning y in [0, 10],
/// meeting at `apex` (the y of `apex` is ignored).
///
/// The walls open toward -Z, `half_angle` radians either side of that axis, so a character
/// running along +Z meets both. The first mesh is the wall on the +X side with its front
/// facing (-cos, 0, -sin) of `half_angle`; the second is the mirror on the -X side with its
/// front facing (cos, 0, -sin).
pub(crate) fn corner_mesh(apex: Vec3, half_angle: f32) -> [WorldMesh; 2] {
    let (sin, cos) = half_angle.sin_cos();
    [
        corner_wall(apex, Vec3::new(sin, 0.0, -cos), Vec3::new(-cos, 0.0, -sin)),
        corner_wall(apex, Vec3::new(-sin, 0.0, -cos), Vec3::new(cos, 0.0, -sin)),
    ]
}

/// A wall running from `apex` along `direction`, wound so its front faces `facing`.
fn corner_wall(apex: Vec3, direction: Vec3, facing: Vec3) -> WorldMesh {
    let far = apex + direction * CORNER_WALL_LENGTH;
    let verts = [
        [apex.x, 0.0, apex.z],
        [far.x, 0.0, far.z],
        [far.x, 10.0, far.z],
        [apex.x, 10.0, apex.z],
    ];
    let front = Vec3::new(direction.z, 0.0, -direction.x);
    let idxs: &[u32] = if front.dot(facing) > 0.0 {
        &[0, 2, 1, 0, 3, 2]
    } else {
        &[0, 1, 2, 0, 2, 3]
    };
    quad_mesh(&verts, idxs)
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

/// `steps` solid boxes spanning x in [-10, 10]; step `i` has its riser at
/// `z_start + i * tread`, top at `(i + 1) * rise`, and runs `landing` m past the last riser.
pub(crate) fn stairs_mesh(
    steps: u32,
    rise: f32,
    tread: f32,
    z_start: f32,
    landing: f32,
) -> WorldMesh {
    let z_end = z_start + (steps - 1) as f32 * tread + landing;
    let mut verts: Vec<[f32; 3]> = Vec::new();
    let mut idxs: Vec<u32> = Vec::new();
    for i in 0..steps {
        let z_front = z_start + i as f32 * tread;
        let height = (i + 1) as f32 * rise;
        let base = verts.len() as u32;
        verts.extend([
            [-10.0, 0.0, z_front],
            [10.0, 0.0, z_front],
            [10.0, height, z_front],
            [-10.0, height, z_front],
            [-10.0, 0.0, z_end],
            [10.0, 0.0, z_end],
            [10.0, height, z_end],
            [-10.0, height, z_end],
        ]);
        let faces: [u32; 36] = [
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6,
            5, 0, 1, 5, 0, 5, 4,
        ];
        idxs.extend(faces.iter().map(|f| base + f));
    }
    quad_mesh(&verts, &idxs)
}

/// The tag tests give a mesh whose tag they do not inspect.
pub(crate) const TEST_TAG: super::HitTag = super::HitTag(1);

/// A wall in the plane `x`, spanning y and z in [-10, 10], wound so its front faces -X.
pub(crate) fn x_wall_mesh(x: f32) -> WorldMesh {
    quad_mesh(&x_wall_verts(x), &[0, 2, 1, 0, 3, 2])
}

/// The same wall as [`x_wall_mesh`] with the opposite winding, so its front faces +X.
pub(crate) fn x_wall_mesh_reversed(x: f32) -> WorldMesh {
    quad_mesh(&x_wall_verts(x), &[0, 1, 2, 0, 2, 3])
}

fn x_wall_verts(x: f32) -> [[f32; 3]; 4] {
    [
        [x, -10.0, -10.0],
        [x, 10.0, -10.0],
        [x, 10.0, 10.0],
        [x, -10.0, 10.0],
    ]
}
