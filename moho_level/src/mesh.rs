//! One glTF primitive to one world mesh.

use std::borrow::Cow;

use glam::{Mat3, Mat4, Vec3};
use moho_render_api::WorldMesh;

use crate::error::LevelErrorKind;

/// The triangle primitives of `mesh`; any other mode is skipped with a warning.
pub(crate) fn triangle_primitives<'a>(
    mesh: &gltf::Mesh<'a>,
) -> impl Iterator<Item = gltf::Primitive<'a>> {
    let (name, index) = (mesh.name().unwrap_or_default(), mesh.index());
    mesh.primitives().filter(move |primitive| {
        let triangles = primitive.mode() == gltf::mesh::Mode::Triangles;
        if !triangles {
            tracing::warn!(
                mesh = name,
                index,
                mode = ?primitive.mode(),
                "skipping non-triangle primitive"
            );
        }
        triangles
    })
}

/// Convert `primitive` with `world` baked into its vertices. `flip_winding` swaps each
/// triangle's last two indices; callers pass whether the transform they place the mesh
/// under mirrors it. `TEXCOORD_0`, when present, becomes the mesh's UVs.
pub(crate) fn convert(
    mesh: &gltf::Mesh<'_>,
    primitive: &gltf::Primitive<'_>,
    world: Mat4,
    flip_winding: bool,
    buffers: &[Cow<'_, [u8]>],
) -> Result<WorldMesh, LevelErrorKind> {
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
    if flip_winding {
        for tri in indices.as_chunks_mut::<3>().0 {
            tri.swap(1, 2);
        }
    }

    let uvs = primitive
        .get(&gltf::Semantic::TexCoords(0))
        .map(|_| {
            reader
                .read_tex_coords(0)
                .ok_or_else(|| bad("TEXCOORD_0 out of buffer range".into()))
                .map(|uvs| uvs.into_f32().collect::<Vec<_>>())
        })
        .transpose()?;

    let mesh = WorldMesh::new(
        positions,
        normals,
        vec![1.0; count],
        vec![[0.0; 3]; count],
        vec![1.0; count],
        vec![0; count],
        indices,
    )
    .map_err(|e| bad(e.to_string()))?;
    if let Some(uvs) = uvs {
        return mesh.with_uvs(uvs).map_err(|e| bad(e.to_string()));
    }
    Ok(mesh)
}
