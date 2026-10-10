//! One glTF primitive to one world mesh.

use std::borrow::Cow;

use glam::{Mat3, Mat4, Vec3};
use moho_render_api::WorldMesh;

use crate::{LevelMesh, error::LevelErrorKind};

pub(crate) fn convert(
    mesh: &gltf::Mesh<'_>,
    primitive: &gltf::Primitive<'_>,
    world: Mat4,
    buffers: &[Cow<'_, [u8]>],
) -> Result<LevelMesh, LevelErrorKind> {
    let bad = |reason: String| LevelErrorKind::BadMesh {
        mesh: mesh.index(),
        reason,
    };
    let reader = primitive.reader(|b| buffers.get(b.index()).map(|data| &**data));
    let raw_positions = reader
        .read_positions()
        .ok_or_else(|| bad("positions missing or out of buffer range".into()))?;
    let raw_normals = reader
        .read_normals()
        .ok_or_else(|| bad("normals missing or out of buffer range".into()))?;

    let normal_matrix = Mat3::from_mat4(world).inverse().transpose();
    let positions: Vec<[f32; 3]> = raw_positions
        .map(|p| world.transform_point3(Vec3::from(p)).to_array())
        .collect();
    let normals: Vec<[f32; 3]> = raw_normals
        .map(|n| {
            (normal_matrix * Vec3::from(n))
                .normalize_or_zero()
                .to_array()
        })
        .collect();
    let count = positions.len();

    let mut indices: Vec<u32> = if primitive.indices().is_some() {
        reader
            .read_indices()
            .ok_or_else(|| bad("indices out of buffer range".into()))?
            .into_u32()
            .collect()
    } else {
        (0..u32::try_from(count).unwrap_or(u32::MAX)).collect()
    };
    if world.determinant() < 0.0 {
        for tri in indices.as_chunks_mut::<3>().0 {
            tri.swap(1, 2);
        }
    }

    let world_mesh = WorldMesh::new(
        positions,
        normals,
        vec![1.0; count],
        vec![[0.0; 3]; count],
        vec![1.0; count],
        vec![0; count],
        indices,
    )
    .map_err(|e| bad(e.to_string()))?;

    Ok(LevelMesh {
        mesh: world_mesh,
        material: primitive.material().name().map(str::to_owned),
    })
}
