//! Scene walk and primitive conversion.

use glam::{Mat3, Mat4, Vec3};
use moho_render_api::WorldMesh;

use crate::{LevelMesh, error::LevelErrorKind};

/// Convert every triangle primitive reachable from the default scene.
pub(crate) fn build(
    document: &gltf::Document,
    buffers: &[Vec<u8>],
) -> Result<Vec<LevelMesh>, LevelErrorKind> {
    let mut out = Vec::new();
    let mut walk = Walk {
        buffers,
        seen: vec![false; document.nodes().len()],
        out: &mut out,
    };
    if let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        for node in scene.nodes() {
            walk.node(&node, Mat4::IDENTITY)?;
        }
    }
    Ok(out)
}

/// Depth-first scene walk; `seen` refuses a node reached twice, since glTF node
/// graphs must be trees and a cycle would otherwise recurse forever.
struct Walk<'a> {
    buffers: &'a [Vec<u8>],
    seen: Vec<bool>,
    out: &'a mut Vec<LevelMesh>,
}

impl Walk<'_> {
    fn node(&mut self, node: &gltf::Node<'_>, parent: Mat4) -> Result<(), LevelErrorKind> {
        if std::mem::replace(&mut self.seen[node.index()], true) {
            return Err(LevelErrorKind::NotGltf(
                format!("node {} has more than one parent", node.index()).into(),
            ));
        }
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    tracing::warn!(
                        mesh = mesh.name().unwrap_or_default(),
                        index = mesh.index(),
                        mode = ?primitive.mode(),
                        "skipping non-triangle primitive"
                    );
                    continue;
                }
                self.out
                    .push(convert(&mesh, &primitive, world, self.buffers)?);
            }
        }
        for child in node.children() {
            self.node(&child, world)?;
        }
        Ok(())
    }
}

fn convert(
    mesh: &gltf::Mesh<'_>,
    primitive: &gltf::Primitive<'_>,
    world: Mat4,
    buffers: &[Vec<u8>],
) -> Result<LevelMesh, LevelErrorKind> {
    let bad = |reason: String| LevelErrorKind::BadMesh {
        mesh: mesh.index(),
        reason,
    };
    let reader = primitive.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
    let raw_positions = reader
        .read_positions()
        .ok_or_else(|| bad("primitive has no positions".into()))?;
    let raw_normals = reader
        .read_normals()
        .ok_or_else(|| bad("primitive has no normals".into()))?;

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

    let mut indices: Vec<u32> = reader.read_indices().map_or_else(
        || (0..u32::try_from(count).unwrap_or(u32::MAX)).collect(),
        |i| i.into_u32().collect(),
    );
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
