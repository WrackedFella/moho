//! Model import: a glTF file becomes plain data a game places node by node.

use std::path::Path;

use glam::Affine3A;
use moho_render_api::{MaterialGpu, MaterialKey, RenderMaterial, WorldMesh};

use crate::LevelError;

/// `MaterialKey::variant` of every [`ModelMaterial`]; outside the variants searchlight
/// (0..4) and flagstone (0, 1) use, so a game that also uses `0x80` would merge keys.
pub const MODEL_MATERIAL_VARIANT: u8 = 0x80;

/// A loaded model: meshes in node space, a named node tree and materials.
#[derive(Debug)]
pub struct Model {
    nodes: Vec<ModelNode>,
    meshes: Vec<ModelMesh>,
    materials: Vec<ModelMaterial>,
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
}

/// A glTF material's base colour.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelMaterial {
    pub name: Option<String>,
    /// Linear RGBA from `pbrMetallicRoughness.baseColorFactor`.
    pub base_colour: [f32; 4],
}

impl ModelMaterial {
    /// The material a primitive without one uses: white.
    pub const GLTF_DEFAULT: Self = Self {
        name: None,
        base_colour: [1.0; 4],
    };
}

impl RenderMaterial for ModelMaterial {
    fn to_gpu(&self) -> MaterialGpu {
        todo!()
    }

    fn dedup_key(&self) -> MaterialKey {
        todo!()
    }
}

/// Load a `.gltf` or `.glb` model from `path`.
///
/// # Errors
/// Returns a [`LevelError`] naming `path` when the file cannot be read, is not glTF,
/// references an unreadable buffer, reaches a node twice, or holds a malformed mesh.
pub fn load_model(path: &Path) -> Result<Model, LevelError> {
    let _ = path;
    todo!()
}

impl Model {
    pub fn nodes(&self) -> &[ModelNode] {
        todo!()
    }

    pub fn meshes(&self) -> &[ModelMesh] {
        todo!()
    }

    pub fn materials(&self) -> &[ModelMaterial] {
        todo!()
    }

    /// The first node called `name` in walk order.
    pub fn node(&self, name: &str) -> Option<&ModelNode> {
        let _ = name;
        todo!()
    }

    /// The material `mesh` draws with.
    pub fn material_of(&self, mesh: &ModelMesh) -> &ModelMaterial {
        let _ = mesh;
        todo!()
    }

    /// For each mesh in order, its index and `root * nodes[mesh.node].rest`.
    pub fn placements(&self, root: Affine3A) -> impl Iterator<Item = (usize, Affine3A)> + '_ {
        let _ = root;
        let placed: std::iter::Empty<(usize, Affine3A)> = todo!();
        placed
    }
}

#[cfg(test)]
mod tests {
    use glam::{Affine3A, Quat, Vec3};
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
        }
    }

    fn two_mesh_model() -> Model {
        Model {
            nodes: vec![
                node(Some("a"), None, Affine3A::from_translation(Vec3::X)),
                node(
                    Some("b"),
                    Some(0),
                    Affine3A::from_translation(Vec3::new(1.0, 2.0, 0.0)),
                ),
            ],
            meshes: vec![mesh_on(0), mesh_on(1)],
            materials: Vec::new(),
        }
    }

    fn colour(rgba: [f32; 4]) -> ModelMaterial {
        ModelMaterial {
            name: None,
            base_colour: rgba,
        }
    }

    fn assert_affine_near(actual: Affine3A, expected: Affine3A) {
        assert!(
            actual.abs_diff_eq(expected, 1e-5),
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn placing_at_a_new_root_changes_only_transforms() {
        let model = two_mesh_model();
        let first = Affine3A::from_translation(Vec3::new(5.0, 0.0, 0.0));
        let second = Affine3A::from_rotation_translation(
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Vec3::new(0.0, 3.0, 0.0),
        );

        let before: Vec<_> = model.placements(first).collect();
        let after: Vec<_> = model.placements(second).collect();

        let ids = |placed: &[(usize, Affine3A)]| placed.iter().map(|p| p.0).collect::<Vec<_>>();
        assert_eq!(ids(&before), vec![0, 1]);
        assert_eq!(ids(&after), vec![0, 1]);
        for (placed, root) in [(&before, first), (&after, second)] {
            for &(mesh, transform) in placed {
                assert_affine_near(
                    transform,
                    root * model.nodes()[model.meshes()[mesh].node].rest,
                );
            }
        }
        assert_eq!(model.meshes()[0].mesh.positions(), triangle().positions());
        assert_ne!(before[1].1, after[1].1);
    }

    #[test]
    fn three_roots_give_one_placement_per_mesh_each_in_call_order() {
        let model = two_mesh_model();
        let roots = [
            Affine3A::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            Affine3A::from_scale(Vec3::splat(2.0)),
            Affine3A::from_translation(Vec3::new(0.0, 0.0, -4.0)),
        ];

        let placed: Vec<Vec<_>> = roots
            .iter()
            .map(|&r| model.placements(r).collect())
            .collect();

        assert_eq!(placed.len(), 3);
        for (root, copy) in roots.iter().zip(&placed) {
            assert_eq!(copy.len(), model.meshes().len());
            for (expected_mesh, &(mesh, transform)) in copy.iter().enumerate() {
                assert_eq!(mesh, expected_mesh);
                let offset = model.nodes()[model.meshes()[mesh].node].rest;
                assert_affine_near(transform, *root * offset);
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
        let red = colour([1.0, 0.0, 0.0, 1.0]);
        let green = colour([0.0, 1.0, 0.0, 1.0]);

        let red_key = red.dedup_key();

        assert_eq!(red_key.variant, MODEL_MATERIAL_VARIANT);
        assert_eq!(MODEL_MATERIAL_VARIANT, 0x80);
        assert_eq!(red_key, colour([1.0, 0.0, 0.0, 1.0]).dedup_key());
        assert_ne!(red_key, green.dedup_key());
        assert_ne!(
            colour([0.5, 0.5, 0.5, 1.0]).dedup_key(),
            colour([0.5, 0.5, 0.25, 1.0]).dedup_key()
        );
    }
}
