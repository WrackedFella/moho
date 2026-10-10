//! Level loading: glTF files become world meshes the engine can draw and collide with.

mod buffers;
mod error;
mod mesh;

use std::borrow::Cow;
use std::path::Path;

use glam::Mat4;

pub use error::{LevelError, LevelErrorKind};
use moho_render_api::WorldMesh;

/// A loaded level: one entry per triangle primitive reached from the default scene.
#[derive(Debug)]
pub struct Level {
    pub meshes: Vec<LevelMesh>,
}

/// One world mesh with node transforms already applied.
#[derive(Debug)]
pub struct LevelMesh {
    pub mesh: WorldMesh,
    /// Name of the glTF material, if the primitive has a named one.
    pub material: Option<String>,
}

/// Load a `.gltf` or `.glb` level from `path`.
///
/// # Errors
/// Returns a [`LevelError`] naming `path` when the file cannot be read, is not
/// glTF, references an unreadable buffer, or holds a malformed mesh.
pub fn load(path: &Path) -> Result<Level, LevelError> {
    let fail = |kind| LevelError::new(path, kind);
    let gltf = {
        let bytes = std::fs::read(path).map_err(|e| fail(LevelErrorKind::Read(e)))?;
        gltf::Gltf::from_slice(&bytes).map_err(|e| fail(LevelErrorKind::NotGltf(Box::new(e))))?
    };
    let dir = path.parent().unwrap_or_else(|| Path::new(""));
    let buffers = buffers::resolve(&gltf.document, gltf.blob.as_deref(), dir).map_err(fail)?;
    let meshes = build(&gltf.document, &buffers).map_err(fail)?;
    Ok(Level { meshes })
}

/// Convert every triangle primitive reachable from the default scene.
fn build(
    document: &gltf::Document,
    buffers: &[Cow<'_, [u8]>],
) -> Result<Vec<LevelMesh>, LevelErrorKind> {
    let mut walk = Walk {
        buffers,
        seen: vec![false; document.nodes().len()],
        out: Vec::new(),
    };
    if let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        for node in scene.nodes() {
            walk.node(&node, Mat4::IDENTITY)?;
        }
    }
    Ok(walk.out)
}

/// Depth-first scene walk; `seen` refuses a node reached twice, since glTF node
/// graphs must be trees and a cycle would otherwise recurse forever.
struct Walk<'a> {
    buffers: &'a [Cow<'a, [u8]>],
    seen: Vec<bool>,
    out: Vec<LevelMesh>,
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
                    .push(mesh::convert(&mesh, &primitive, world, self.buffers)?);
            }
        }
        for child in node.children() {
            self.node(&child, world)?;
        }
        Ok(())
    }
}
