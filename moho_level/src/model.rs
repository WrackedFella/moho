//! Model import: a glTF file becomes plain data a game places node by node.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use glam::{Affine3A, Mat4};
use moho_render_api::{MaterialGpu, MaterialKey, RenderMaterial, Rgba8Image, WorldMesh};

use crate::anim::{Clip, Pose};
use crate::image::ImageSources;
use crate::{LevelError, LevelErrorKind, mesh, rig};

/// `MaterialKey::variant` of every [`ModelMaterial`]; outside the variants searchlight
/// (0..4) and flagstone (0, 1) use, so a game that also uses `0x80` would merge keys.
pub const MODEL_MATERIAL_VARIANT: u8 = 0x80;

/// A loaded model: meshes in node space, a named node tree and materials.
#[derive(Debug)]
pub struct Model {
    nodes: Vec<ModelNode>,
    meshes: Vec<ModelMesh>,
    materials: Vec<ModelMaterial>,
    images: Vec<ModelImage>,
    skins: Vec<ModelSkin>,
    clips: Vec<Clip>,
}

/// A glTF skin: the nodes that deform a mesh and their inverse bind matrices.
#[derive(Clone, Debug)]
pub struct ModelSkin {
    /// Indices into [`Model::nodes`].
    pub joints: Vec<usize>,
    /// One per joint; identity when the file has no inverse-bind matrices.
    pub inverse_binds: Vec<Affine3A>,
}

/// Per-vertex joint influences of a skinned mesh: four per vertex.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshSkin {
    /// Index into [`Model::skins`].
    pub skin: usize,
    /// Per vertex, indices into the skin's joints.
    pub joints: Vec<[u16; 4]>,
    /// Per vertex, renormalised to sum to 1.
    pub weights: Vec<[f32; 4]>,
}

/// One glTF node, in walk order.
#[derive(Clone, Debug)]
pub struct ModelNode {
    pub name: Option<String>,
    /// Index into [`Model::nodes`]; always smaller than this node's own index.
    pub parent: Option<usize>,
    pub local: Affine3A,
    /// The node's offset from the model origin: the product of locals from the root.
    pub rest: Affine3A,
}

/// One triangle primitive, with vertices in its node's space.
#[derive(Clone, Debug)]
pub struct ModelMesh {
    pub mesh: WorldMesh,
    /// Index into [`Model::nodes`].
    pub node: usize,
    /// Index into [`Model::materials`]; `None` draws as [`ModelMaterial::GLTF_DEFAULT`].
    pub material: Option<usize>,
    /// Joint influences when the primitive is skinned.
    pub skin: Option<MeshSkin>,
}

/// A glTF material's base colour.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelMaterial {
    pub name: Option<String>,
    /// Linear RGBA from `pbrMetallicRoughness.baseColorFactor`.
    pub base_colour: [f32; 4],
    /// Index into [`Model::images`] of the base-colour texture, if the material has one.
    pub base_colour_image: Option<usize>,
}

/// One decoded base-colour image of a model.
#[derive(Clone, Debug)]
pub struct ModelImage {
    pub name: Option<String>,
    /// Canonical path of a file image; `None` for one embedded in a buffer view.
    pub source: Option<PathBuf>,
    pub image: Rgba8Image,
}

impl ModelMaterial {
    /// The material a primitive without one uses: white.
    pub const GLTF_DEFAULT: Self = Self {
        name: None,
        base_colour: [1.0; 4],
        base_colour_image: None,
    };
}

impl RenderMaterial for ModelMaterial {
    fn to_gpu(&self) -> MaterialGpu {
        let [r, g, b, _] = self.base_colour;
        MaterialGpu {
            albedo: [r, g, b, 0.0],
            params: [r, g, b, 0.0],
        }
    }

    fn dedup_key(&self) -> MaterialKey {
        let [r, g, b, _] = self.base_colour;
        MaterialKey {
            variant: MODEL_MATERIAL_VARIANT,
            albedo_bits: [r.to_bits(), g.to_bits(), b.to_bits()],
            fuzz_bits: 0,
            ref_idx_bits: 0,
            extra_bits: [0; 3],
        }
    }
}

/// Load a `.gltf` or `.glb` model from `path`.
///
/// # Errors
/// Returns a [`LevelError`] naming `path` when the file cannot be read, is not glTF,
/// references an unreadable buffer, reaches a node twice, or holds a malformed mesh
/// (including a textured primitive without `TEXCOORD_0`, or bad joint influences), skin or
/// animation; a base-colour image that is missing is `MissingImage`, one that is not a
/// relative file or a decodable PNG or JPEG is `BadImage`. Images no material uses as its
/// base colour are never read.
pub fn load_model(path: &Path) -> Result<Model, LevelError> {
    crate::with_buffers(path, build)
}

/// Maps glTF images to [`Model::images`], decoding each base-colour image once.
struct Images<'a> {
    sources: ImageSources<'a>,
    /// For each glTF image, its index in `decoded` once decoded.
    by_gltf_index: Vec<Option<usize>>,
    decoded: Vec<ModelImage>,
}

impl Images<'_> {
    /// The index in [`Model::images`] of `image`, decoding it on first use.
    fn index_of(&mut self, image: &gltf::Image<'_>) -> Result<usize, LevelErrorKind> {
        if let Some(index) = self.by_gltf_index[image.index()] {
            return Ok(index);
        }
        let (source, decoded) = self.sources.load(image)?;
        let index = self.decoded.len();
        self.decoded.push(ModelImage {
            name: image.name().map(str::to_owned),
            source,
            image: decoded,
        });
        self.by_gltf_index[image.index()] = Some(index);
        Ok(index)
    }
}

fn build(
    document: &gltf::Document,
    buffers: &[Cow<'_, [u8]>],
    dir: &Path,
) -> Result<Model, LevelErrorKind> {
    let mut images = Images {
        sources: ImageSources { buffers, dir },
        by_gltf_index: vec![None; document.images().len()],
        decoded: Vec::new(),
    };
    let materials = document
        .materials()
        .map(|m| model_material(&m, &mut images))
        .collect::<Result<_, LevelErrorKind>>()?;
    let mut walk = Walk {
        buffers,
        index_of: vec![None; document.nodes().len()],
        model: Model {
            nodes: Vec::new(),
            meshes: Vec::new(),
            materials,
            images: images.decoded,
            skins: Vec::new(),
            clips: Vec::new(),
        },
    };
    for node in crate::scene_roots(document) {
        walk.node(&node, None)?;
    }
    walk.model.skins = rig::skins(document, buffers, &walk.index_of)?;
    walk.model.clips = rig::clips(document, buffers, &walk.index_of)?;
    Ok(walk.model)
}

/// `material`'s base colour, with its base-colour image decoded into `images`.
fn model_material(
    material: &gltf::Material<'_>,
    images: &mut Images<'_>,
) -> Result<ModelMaterial, LevelErrorKind> {
    let pbr = material.pbr_metallic_roughness();
    let base_colour_image = pbr
        .base_color_texture()
        .map(|info| {
            if info.tex_coord() != 0 {
                tracing::warn!(
                    material = material.name().unwrap_or_default(),
                    tex_coord = info.tex_coord(),
                    "base-colour texture uses another texcoord set; reading set 0"
                );
            }
            images.index_of(&info.texture().source())
        })
        .transpose()?;
    Ok(ModelMaterial {
        name: material.name().map(str::to_owned),
        base_colour: pbr.base_color_factor(),
        base_colour_image,
    })
}

struct Walk<'a> {
    buffers: &'a [Cow<'a, [u8]>],
    /// Per glTF node index, its index in [`Model::nodes`]; `None` until the walk reaches
    /// it, which also refuses a node reached twice.
    index_of: Vec<Option<usize>>,
    model: Model,
}

/// A glTF column-major matrix as an affine transform.
pub(crate) fn affine_from_columns(columns: &[[f32; 4]; 4]) -> Affine3A {
    Affine3A::from_mat4(Mat4::from_cols_array_2d(columns))
}

impl Walk<'_> {
    fn node(&mut self, node: &gltf::Node<'_>, parent: Option<usize>) -> Result<(), LevelErrorKind> {
        if self.index_of[node.index()].is_some() {
            return Err(crate::multiple_parents(node));
        }
        let local = affine_from_columns(&node.transform().matrix());
        let rest = parent.map_or(local, |p| self.model.nodes[p].rest * local);
        let index = self.model.nodes.len();
        self.index_of[node.index()] = Some(index);
        self.model.nodes.push(ModelNode {
            name: node.name().map(str::to_owned),
            parent,
            local,
            rest,
        });
        if let Some(mesh) = node.mesh() {
            // The mesh is placed under `rest`, so a mirrored node must flip its own winding.
            let flip_winding = rest.matrix3.determinant() < 0.0;
            for primitive in mesh::triangle_primitives(&mesh) {
                let converted = mesh::convert(
                    &mesh,
                    &primitive,
                    Mat4::IDENTITY,
                    flip_winding,
                    self.buffers,
                )?;
                if self.is_textured(primitive.material().index()) && converted.uvs().is_none() {
                    return Err(LevelErrorKind::BadMesh {
                        mesh: mesh.index(),
                        reason: "material has a base-colour texture but the primitive has no \
                                 TEXCOORD_0"
                            .into(),
                    });
                }
                let skin = node
                    .skin()
                    .map(|skin| rig::mesh_skin(&mesh, &primitive, &skin, self.buffers))
                    .transpose()?;
                self.model.meshes.push(ModelMesh {
                    mesh: converted,
                    node: index,
                    material: primitive.material().index(),
                    skin,
                });
            }
        }
        for child in node.children() {
            self.node(&child, Some(index))?;
        }
        Ok(())
    }

    /// Whether the material at `material` has a base-colour image; `None` (the glTF
    /// default) has none.
    fn is_textured(&self, material: Option<usize>) -> bool {
        material.is_some_and(|i| self.model.materials[i].base_colour_image.is_some())
    }
}

impl Model {
    pub fn nodes(&self) -> &[ModelNode] {
        &self.nodes
    }

    pub fn meshes(&self) -> &[ModelMesh] {
        &self.meshes
    }

    pub fn materials(&self) -> &[ModelMaterial] {
        &self.materials
    }

    /// The decoded base-colour images, each once however many materials use it.
    pub fn images(&self) -> &[ModelImage] {
        &self.images
    }

    /// The first node called `name` in walk order.
    pub fn node(&self, name: &str) -> Option<&ModelNode> {
        self.nodes.iter().find(|n| n.name.as_deref() == Some(name))
    }

    /// The material `mesh` draws with.
    pub fn material_of(&self, mesh: &ModelMesh) -> &ModelMaterial {
        mesh.material
            .and_then(|i| self.materials.get(i))
            .unwrap_or(&ModelMaterial::GLTF_DEFAULT)
    }

    /// For each mesh in order, its index and `root * nodes[mesh.node].rest`.
    pub fn placements(&self, root: Affine3A) -> impl Iterator<Item = (usize, Affine3A)> + '_ {
        self.placed(root, |node| self.nodes[node].rest)
    }

    /// For each mesh in order, its index and `root * transform(mesh.node)`.
    fn placed<'a>(
        &'a self,
        root: Affine3A,
        transform: impl Fn(usize) -> Affine3A + 'a,
    ) -> impl Iterator<Item = (usize, Affine3A)> + 'a {
        self.meshes
            .iter()
            .enumerate()
            .map(move |(i, m)| (i, root * transform(m.node)))
    }

    pub fn skins(&self) -> &[ModelSkin] {
        &self.skins
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    /// The index in [`Model::clips`] of the first clip called `name`.
    pub fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name() == Some(name))
    }

    /// Every node at its `rest` transform.
    pub fn rest_pose(&self) -> Pose {
        Pose::new(self.nodes.iter().map(|n| n.rest).collect())
    }

    /// The pose of clip `clip` at `time`: nodes no channel targets keep `local` exactly,
    /// targeted nodes replace the animated properties of their decomposed `local`, and
    /// globals compose parent-first in walk order.
    ///
    /// # Panics
    /// When `clip` is not an index into [`Model::clips`].
    pub fn pose(&self, clip: usize, time: f32) -> Pose {
        let animated = self.clips[clip].locals_at(&self.nodes, time);
        let mut globals: Vec<Affine3A> = Vec::with_capacity(self.nodes.len());
        for (node, animated) in self.nodes.iter().zip(animated) {
            let local = animated.map_or(node.local, |(scale, rotation, translation)| {
                Affine3A::from_scale_rotation_translation(scale, rotation, translation)
            });
            // The same product the loader forms for `rest`, so an unanimated chain
            // reproduces it bit for bit.
            let global = node.parent.map_or(local, |p| globals[p] * local);
            globals.push(global);
        }
        Pose::new(globals)
    }

    /// Per joint of `skin`, `pose[joint] * inverse_bind`, in model space.
    ///
    /// # Panics
    /// When `skin` is not an index into [`Model::skins`] or `pose` is not of this model.
    pub fn joint_palette(&self, skin: usize, pose: &Pose) -> Vec<Affine3A> {
        let skin = &self.skins[skin];
        skin.joints
            .iter()
            .zip(&skin.inverse_binds)
            .map(|(&joint, &inverse_bind)| pose.nodes()[joint] * inverse_bind)
            .collect()
    }

    /// [`Model::placements`] with the node transforms of `pose`: for each mesh in order,
    /// its index and `root * pose[mesh.node]`.
    ///
    /// # Panics
    /// When `pose` is not of this model.
    pub fn posed_placements<'a>(
        &'a self,
        root: Affine3A,
        pose: &'a Pose,
    ) -> impl Iterator<Item = (usize, Affine3A)> + 'a {
        self.placed(root, |node| pose.nodes()[node])
    }

    /// A model of `nodes` and `clips` alone, for unit tests.
    #[cfg(test)]
    pub(crate) fn for_tests(nodes: Vec<ModelNode>, clips: Vec<Clip>) -> Self {
        Self {
            nodes,
            meshes: Vec::new(),
            materials: Vec::new(),
            images: Vec::new(),
            skins: Vec::new(),
            clips,
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::{Affine3A, Mat3, Vec3};
    use moho_render_api::{RenderMaterial, WorldMesh};

    use super::{MODEL_MATERIAL_VARIANT, Model, ModelMaterial, ModelMesh, ModelNode};

    fn node(name: Option<&str>, parent: Option<usize>, rest: Affine3A) -> ModelNode {
        ModelNode {
            name: name.map(str::to_owned),
            parent,
            local: rest,
            rest,
        }
    }

    fn triangle() -> WorldMesh {
        WorldMesh::new(
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0.0, 0.0, 1.0]; 3],
            vec![1.0; 3],
            vec![[0.0; 3]; 3],
            vec![1.0; 3],
            vec![0; 3],
            vec![0, 1, 2],
        )
        .expect("a triangle is a valid mesh")
    }

    fn mesh_on(node: usize) -> ModelMesh {
        ModelMesh {
            mesh: triangle(),
            node,
            material: None,
            skin: None,
        }
    }

    fn colour(rgba: [f32; 4]) -> ModelMaterial {
        ModelMaterial {
            name: None,
            base_colour: rgba,
            base_colour_image: None,
        }
    }

    fn assert_affine_near(actual: Affine3A, expected: Affine3A) {
        assert!(
            actual.abs_diff_eq(expected, 1e-5),
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn three_roots_give_one_placement_per_mesh_each_in_call_order() {
        // Meshes sit on nodes 2, 2 and 1 behind a mesh-less node 0, so a mesh index
        // is never its own node index.
        let model = Model {
            nodes: vec![
                node(
                    Some("pivot"),
                    None,
                    Affine3A::from_translation(Vec3::Z * 3.0),
                ),
                node(
                    Some("a"),
                    Some(0),
                    Affine3A::from_translation(Vec3::new(1.0, 0.0, 3.0)),
                ),
                node(
                    Some("b"),
                    Some(0),
                    Affine3A::from_translation(Vec3::new(0.0, 2.0, 3.0)),
                ),
            ],
            meshes: vec![mesh_on(2), mesh_on(2), mesh_on(1)],
            materials: Vec::new(),
            images: Vec::new(),
            skins: Vec::new(),
            clips: Vec::new(),
        };
        let shifted = Affine3A::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let doubled = Affine3A::from_scale(Vec3::splat(2.0));
        let lowered = Affine3A::from_translation(Vec3::new(0.0, 0.0, -4.0));
        let scale_two =
            |t: Vec3| Affine3A::from_mat3_translation(Mat3::from_diagonal(Vec3::splat(2.0)), t);
        let translated = |t: Vec3| Affine3A::from_translation(t);

        let at_shifted: Vec<_> = model.placements(shifted).collect();
        let at_doubled: Vec<_> = model.placements(doubled).collect();
        let at_lowered: Vec<_> = model.placements(lowered).collect();

        let expected = [
            (
                at_shifted,
                [
                    translated(Vec3::new(10.0, 2.0, 3.0)),
                    translated(Vec3::new(10.0, 2.0, 3.0)),
                    translated(Vec3::new(11.0, 0.0, 3.0)),
                ],
            ),
            (
                at_doubled,
                [
                    scale_two(Vec3::new(0.0, 4.0, 6.0)),
                    scale_two(Vec3::new(0.0, 4.0, 6.0)),
                    scale_two(Vec3::new(2.0, 0.0, 6.0)),
                ],
            ),
            (
                at_lowered,
                [
                    translated(Vec3::new(0.0, 2.0, -1.0)),
                    translated(Vec3::new(0.0, 2.0, -1.0)),
                    translated(Vec3::new(1.0, 0.0, -1.0)),
                ],
            ),
        ];
        for (placed, transforms) in expected {
            let ids: Vec<usize> = placed.iter().map(|p| p.0).collect();
            assert_eq!(ids, vec![0, 1, 2]);
            for (&(_, actual), want) in placed.iter().zip(transforms) {
                assert_affine_near(actual, want);
            }
        }
    }

    #[test]
    fn duplicate_node_names_find_the_first_in_walk_order() {
        let model = Model {
            nodes: vec![
                node(Some("root"), None, Affine3A::IDENTITY),
                node(Some("twin"), Some(0), Affine3A::from_translation(Vec3::X)),
                node(Some("twin"), Some(0), Affine3A::from_translation(Vec3::Y)),
            ],
            meshes: Vec::new(),
            materials: Vec::new(),
            images: Vec::new(),
            skins: Vec::new(),
            clips: Vec::new(),
        };

        let found = model.node("twin").expect("twin exists");

        assert_affine_near(found.rest, Affine3A::from_translation(Vec3::X));
        assert!(model.node("absent").is_none());
    }

    #[test]
    fn model_material_draws_its_colour_on_top_and_sides() {
        let material = colour([0.8, 0.2, 0.1, 0.5]);

        let gpu = material.to_gpu();

        assert_eq!(gpu.albedo, [0.8, 0.2, 0.1, 0.0]);
        assert_eq!(gpu.params, [0.8, 0.2, 0.1, 0.0]);
    }

    #[test]
    fn model_material_keys_differ_by_colour_and_use_the_model_variant() {
        let grey = colour([0.5, 0.5, 0.5, 1.0]);
        let less_red = colour([0.25, 0.5, 0.5, 1.0]);
        let less_green = colour([0.5, 0.25, 0.5, 1.0]);
        let less_blue = colour([0.5, 0.5, 0.25, 1.0]);

        let grey_key = grey.dedup_key();

        assert_eq!(grey_key.variant, MODEL_MATERIAL_VARIANT);
        assert_eq!(MODEL_MATERIAL_VARIANT, 0x80);
        assert_eq!(grey_key, colour([0.5, 0.5, 0.5, 1.0]).dedup_key());
        assert_ne!(grey_key, less_red.dedup_key());
        assert_ne!(grey_key, less_green.dedup_key());
        assert_ne!(grey_key, less_blue.dedup_key());
    }
}
