//! Level loading: glTF files become world meshes the engine can draw and collide with,
//! plus named markers (empty nodes) for game code to interpret. The [`nav`] module builds a
//! navigation mesh from those meshes and finds paths across it.

mod buffers;
mod error;
mod image;
mod marker;
mod mesh;
mod model;
pub mod nav;

use std::borrow::Cow;
use std::path::Path;

use glam::Mat4;

pub use error::{LevelError, LevelErrorKind};
pub use image::load_image;
pub use marker::LevelMarker;
pub use model::{
    MODEL_MATERIAL_VARIANT, Model, ModelImage, ModelMaterial, ModelMesh, ModelNode, load_model,
};
use moho_render_api::WorldMesh;
pub use nav::{AgentProfile, NavBuildError, NavMesh};

/// A loaded level: one mesh per triangle primitive and one marker per named,
/// mesh-less node reached from the default scene.
#[derive(Debug)]
pub struct Level {
    pub meshes: Vec<LevelMesh>,
    /// In scene-walk order: depth-first, parents before children, siblings in file order.
    pub markers: Vec<LevelMarker>,
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
    let (meshes, markers) = with_buffers(path, |document, buffers, _| build(document, buffers))?;
    Ok(Level { meshes, markers })
}

/// Read and parse the glTF at `path`, resolve its buffers and run `f` on them and the
/// directory relative URIs resolve against; every failure, including `f`'s, is a
/// [`LevelError`] naming `path`.
fn with_buffers<T>(
    path: &Path,
    f: impl FnOnce(&gltf::Document, &[Cow<'_, [u8]>], &Path) -> Result<T, LevelErrorKind>,
) -> Result<T, LevelError> {
    let fail = |kind| LevelError::new(path, kind);
    let gltf = {
        let bytes = std::fs::read(path).map_err(|e| fail(LevelErrorKind::Read(e)))?;
        gltf::Gltf::from_slice(&bytes).map_err(|e| fail(LevelErrorKind::NotGltf(Box::new(e))))?
    };
    let dir = path.parent().unwrap_or_else(|| Path::new(""));
    let buffers = buffers::resolve(&gltf.document, gltf.blob.as_deref(), dir).map_err(fail)?;
    f(&gltf.document, &buffers, dir).map_err(fail)
}

fn build(
    document: &gltf::Document,
    buffers: &[Cow<'_, [u8]>],
) -> Result<(Vec<LevelMesh>, Vec<LevelMarker>), LevelErrorKind> {
    let mut walk = Walk {
        buffers,
        seen: vec![false; document.nodes().len()],
        meshes: Vec::new(),
        markers: Vec::new(),
    };
    for node in scene_roots(document) {
        walk.node(&node, Mat4::IDENTITY)?;
    }
    Ok((walk.meshes, walk.markers))
}

/// Root nodes of the default scene, or of the first scene when none is the default.
fn scene_roots(document: &gltf::Document) -> impl Iterator<Item = gltf::Node<'_>> {
    document
        .default_scene()
        .or_else(|| document.scenes().next())
        .into_iter()
        .flat_map(|scene| scene.nodes())
}

/// Mark `node` visited, refusing one reached twice: glTF node graphs must be trees, and
/// a cycle would otherwise recurse forever.
fn mark_seen(seen: &mut [bool], node: &gltf::Node<'_>) -> Result<(), LevelErrorKind> {
    if std::mem::replace(&mut seen[node.index()], true) {
        return Err(LevelErrorKind::NotGltf(
            format!("node {} has more than one parent", node.index()).into(),
        ));
    }
    Ok(())
}

/// Depth-first scene walk; `seen` refuses a node reached twice, since glTF node
/// graphs must be trees and a cycle would otherwise recurse forever.
struct Walk<'a> {
    buffers: &'a [Cow<'a, [u8]>],
    seen: Vec<bool>,
    meshes: Vec<LevelMesh>,
    markers: Vec<LevelMarker>,
}

impl Walk<'_> {
    fn node(&mut self, node: &gltf::Node<'_>, parent: Mat4) -> Result<(), LevelErrorKind> {
        mark_seen(&mut self.seen, node)?;
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        self.markers.extend(marker::from_node(node, world));
        if let Some(mesh) = node.mesh() {
            let flip_winding = world.determinant() < 0.0;
            for primitive in mesh::triangle_primitives(&mesh) {
                let converted =
                    mesh::convert(&mesh, &primitive, world, flip_winding, self.buffers)?;
                self.meshes.push(LevelMesh {
                    mesh: converted,
                    material: primitive.material().name().map(str::to_owned),
                });
            }
        }
        for child in node.children() {
            self.node(&child, world)?;
        }
        Ok(())
    }
}
