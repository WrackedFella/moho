//! Level loading: glTF files become world meshes the engine can draw and collide with,
//! plus named markers (empty nodes) for game code to interpret. The [`nav`] module builds a
//! navigation mesh from those meshes and finds paths across it.

pub mod anim;
mod buffers;
mod error;
mod hash;
mod image;
mod loaded;
mod marker;
mod mesh;
mod model;
pub mod nav;
mod rig;
mod socket;

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use glam::Mat4;

pub use error::{LevelError, LevelErrorKind};
pub use hash::LevelHash;
pub use image::load_image;
pub use loaded::LoadedLevel;
pub use marker::LevelMarker;
pub use model::{
    MODEL_MATERIAL_VARIANT, MeshSkin, Model, ModelImage, ModelMaterial, ModelMesh, ModelNode,
    ModelSkin, load_model,
};
use moho_render_api::WorldMesh;
pub use nav::{AgentProfile, NavBuildError, NavMesh};
pub use socket::{Assembly, SocketError};

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

/// The hash a replayed switch to the level at `path` is pinned to, without building any mesh.
///
/// # Errors
/// Returns a [`LevelError`] naming `path` when the file or one of its buffers cannot be read
/// or the file is not glTF.
pub fn level_hash(path: &Path) -> Result<LevelHash, LevelError> {
    with_sources(path, |sources, buffers| Ok(sources.hash(buffers)))
}

/// Load the level at `path` only if its content hashes to `expected`.
///
/// # Errors
/// [`LevelErrorKind::HashMismatch`] when it does not, before any mesh is built; otherwise
/// as [`load`].
pub fn load_pinned(path: &Path, expected: &LevelHash) -> Result<Level, LevelError> {
    load_hashed(path, Some(expected)).map(|(level, _)| level)
}

/// Load the level at `path` with the hash of the bytes loaded; a `pin` that differs is
/// refused before any mesh is built.
pub(crate) fn load_hashed(
    path: &Path,
    pin: Option<&LevelHash>,
) -> Result<(Level, LevelHash), LevelError> {
    with_sources(path, |sources, buffers| {
        let found = sources.hash(buffers);
        if let Some(&expected) = pin.filter(|&&expected| expected != found) {
            return Err(LevelErrorKind::HashMismatch { expected, found });
        }
        let (meshes, markers) = build(&sources.gltf.document, buffers)?;
        Ok((Level { meshes, markers }, found))
    })
}

/// The bytes a level load reads: the main file, parsed, and the directory its relative
/// URIs resolve against.
struct Sources {
    main: Vec<u8>,
    gltf: gltf::Gltf,
    dir: PathBuf,
}

impl Sources {
    fn read(path: &Path) -> Result<Self, LevelError> {
        let fail = |kind| LevelError::new(path, kind);
        let main = std::fs::read(path).map_err(|e| fail(LevelErrorKind::Read(e)))?;
        let gltf = gltf::Gltf::from_slice(&main)
            .map_err(|e| fail(LevelErrorKind::NotGltf(Box::new(e))))?;
        let dir = path.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        Ok(Self { main, gltf, dir })
    }

    fn buffers(&self) -> Result<Vec<Cow<'_, [u8]>>, LevelErrorKind> {
        buffers::resolve(&self.gltf.document, self.gltf.blob.as_deref(), &self.dir)
    }

    /// The main file, then each URI buffer in index order; the GLB blob is part of the
    /// main file.
    fn hash(&self, buffers: &[Cow<'_, [u8]>]) -> LevelHash {
        let uri_buffers = self
            .gltf
            .document
            .buffers()
            .filter(|buffer| matches!(buffer.source(), gltf::buffer::Source::Uri(_)))
            .filter_map(|buffer| buffers.get(buffer.index()))
            .map(|data| &**data);
        LevelHash::of_sources(std::iter::once(self.main.as_slice()).chain(uri_buffers))
    }
}

/// Read the sources at `path`, resolve their buffers and run `f`; every failure,
/// including `f`'s, is a [`LevelError`] naming `path`.
fn with_sources<T>(
    path: &Path,
    f: impl FnOnce(&Sources, &[Cow<'_, [u8]>]) -> Result<T, LevelErrorKind>,
) -> Result<T, LevelError> {
    let sources = Sources::read(path)?;
    let fail = |kind| LevelError::new(path, kind);
    let buffers = sources.buffers().map_err(fail)?;
    f(&sources, &buffers).map_err(fail)
}

/// Read and parse the glTF at `path`, resolve its buffers and run `f` on them and the
/// directory relative URIs resolve against; every failure, including `f`'s, is a
/// [`LevelError`] naming `path`.
fn with_buffers<T>(
    path: &Path,
    f: impl FnOnce(&gltf::Document, &[Cow<'_, [u8]>], &Path) -> Result<T, LevelErrorKind>,
) -> Result<T, LevelError> {
    with_sources(path, |sources, buffers| {
        f(&sources.gltf.document, buffers, &sources.dir)
    })
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
        return Err(multiple_parents(node));
    }
    Ok(())
}

fn multiple_parents(node: &gltf::Node<'_>) -> LevelErrorKind {
    LevelErrorKind::NotGltf(format!("node {} has more than one parent", node.index()).into())
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
