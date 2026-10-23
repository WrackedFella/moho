use std::f32::consts::FRAC_PI_2;
use std::fs;
use std::path::Path;

use glam::{Affine3A, Quat, Vec3};
use moho_level::{LevelErrorKind, ModelMaterial, load_model};
use serde_json::json;

use crate::support::{
    Node, Prim, Scene, Xform, cube_prim, floor_prim, line_prim, write_gltf, write_gltf_edited,
};

fn assert_affine_near(actual: Affine3A, expected: Affine3A) {
    assert!(
        actual.abs_diff_eq(expected, 1e-4),
        "{actual:?} != {expected:?}"
    );
}

fn assert_vec_near(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, 1e-4),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn two_node_model_places_each_mesh_at_root_times_its_node_offset() {
    let mut scene = Scene::with_meshes(vec![vec![cube_prim()], vec![floor_prim(0.0)]]);
    scene.nodes[0].xform = Xform::translate([1.0, 0.0, 0.0]);
    scene.nodes[1].xform = Xform::translate([0.0, 2.0, 0.0]);
    let (_dir, path) = write_gltf(&scene);
    let root = Affine3A::from_rotation_translation(
        Quat::from_rotation_y(FRAC_PI_2),
        Vec3::new(5.0, 0.0, 0.0),
    );

    let model = load_model(&path).expect("model loads");
    let placed: Vec<_> = model.placements(root).collect();

    assert_eq!(model.meshes().len(), 2);
    assert_eq!(placed.len(), 2);
    assert_eq!(placed[0].0, 0);
    assert_eq!(placed[1].0, 1);
    assert_affine_near(
        placed[0].1,
        root * Affine3A::from_translation(Vec3::new(1.0, 0.0, 0.0)),
    );
    assert_affine_near(
        placed[1].1,
        root * Affine3A::from_translation(Vec3::new(0.0, 2.0, 0.0)),
    );
    let cube = &model.meshes()[0].mesh;
    assert!(
        cube.positions().iter().all(|p| p[0].abs() <= 0.5 + 1e-5),
        "vertices stay in node space, not baked with the node offset"
    );
}

#[test]
fn named_nodes_are_found_with_offsets_from_the_model_origin() {
    let mut scene = Scene::new();
    scene.meshes = vec![vec![cube_prim()]];
    scene.nodes = vec![
        Node {
            xform: Xform::Trs {
                t: [1.0, 0.0, 0.0],
                r: [0.0, 0.0, 0.0, 1.0],
                s: [2.0, 2.0, 2.0],
            },
            mesh: Some(0),
            children: vec![1],
            name: Some("grip".into()),
            ..Node::default()
        },
        Node {
            xform: Xform::translate([0.0, 0.0, 2.0]),
            name: Some("muzzle".into()),
            ..Node::default()
        },
    ];
    scene.roots = vec![0];
    let (_dir, path) = write_gltf(&scene);

    let model = load_model(&path).expect("model loads");

    let grip = model.node("grip").expect("grip found");
    let muzzle = model.node("muzzle").expect("muzzle found");
    assert_eq!(grip.parent, None);
    assert_vec_near(Vec3::from(grip.rest.translation), Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(
        model.nodes()[muzzle.parent.expect("muzzle has a parent")]
            .name
            .as_deref(),
        Some("grip")
    );
    assert_affine_near(
        muzzle.local,
        Affine3A::from_translation(Vec3::new(0.0, 0.0, 2.0)),
    );
    // grip's scale of 2 doubles muzzle's local offset: 1 + 0 on x, 2 * 2 on z.
    assert_vec_near(
        Vec3::from(muzzle.rest.translation),
        Vec3::new(1.0, 0.0, 4.0),
    );
    assert!(model.node("stock").is_none());
    assert!(
        model
            .nodes()
            .iter()
            .position(|n| n.name.as_deref() == Some("grip"))
            < model
                .nodes()
                .iter()
                .position(|n| n.name.as_deref() == Some("muzzle"))
    );
}

#[test]
fn missing_model_file_fails_naming_the_path_as_unreadable() {
    let path = Path::new("/definitely/not/here/rifle.gltf");

    let err = load_model(path).expect_err("no such file");

    assert!(matches!(err.kind(), LevelErrorKind::Read(_)), "{err}");
    assert_eq!(err.path(), path);
    assert!(err.to_string().starts_with(&path.display().to_string()));
}

#[test]
fn non_gltf_model_fails_naming_the_path_as_not_gltf() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("notes.gltf");
    fs::write(&path, "this is not a model").expect("write");

    let err = load_model(&path).expect_err("not gltf");

    assert!(matches!(err.kind(), LevelErrorKind::NotGltf(_)), "{err}");
    assert_eq!(err.path(), path);
    assert!(err.to_string().starts_with(&path.display().to_string()));
}

#[test]
fn model_with_a_missing_buffer_fails_naming_the_path_and_buffer() {
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    scene.bin_file = None;
    let (_dir, path) = write_gltf(&scene);

    let err = load_model(&path).expect_err("buffer missing");

    assert!(
        matches!(err.kind(), LevelErrorKind::MissingBuffer { uri, .. } if uri == "level.bin"),
        "{err}"
    );
    assert_eq!(err.path(), path);
    assert!(err.to_string().starts_with(&path.display().to_string()));
    assert!(err.to_string().contains("level.bin"));
}

#[test]
fn scaled_node_keeps_node_space_normals_and_puts_the_scale_in_its_placement() {
    let slope = std::f32::consts::FRAC_1_SQRT_2;
    let mut face = floor_prim(0.0);
    face.normals = vec![[slope, slope, 0.0]; 4];
    let mut scene = Scene::with_meshes(vec![vec![face]]);
    scene.nodes[0].xform = Xform::scale([1.0, 2.0, 1.0]);
    let (_dir, path) = write_gltf(&scene);

    let model = load_model(&path).expect("model loads");
    let placed: Vec<_> = model.placements(Affine3A::IDENTITY).collect();

    for normal in model.meshes()[0].mesh.normals() {
        assert_vec_near(Vec3::from(*normal), Vec3::new(slope, slope, 0.0));
    }
    assert_eq!(placed.len(), 1);
    assert_affine_near(placed[0].1, Affine3A::from_scale(Vec3::new(1.0, 2.0, 1.0)));
}

fn red_and_plain_scene() -> Scene {
    let mut red = cube_prim();
    red.material = Some(0);
    let mut plain = floor_prim(0.0);
    plain.material = Some(1);
    let mut scene = Scene::with_meshes(vec![vec![plain], vec![red]]);
    scene.materials = vec![Some("red".into()), Some("plain".into())];
    scene
}

#[test]
fn base_colour_factor_becomes_the_material_colour() {
    let scene = red_and_plain_scene();
    let (_dir, path) = write_gltf_edited(&scene, |doc| {
        doc["materials"][0]["pbrMetallicRoughness"] =
            json!({ "baseColorFactor": [1.0, 0.0, 0.0, 1.0] });
    });

    let model = load_model(&path).expect("model loads");

    assert_eq!(model.materials().len(), 2);
    assert_eq!(model.materials()[0].name.as_deref(), Some("red"));
    assert_eq!(model.materials()[0].base_colour, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(model.materials()[1].name.as_deref(), Some("plain"));
    assert_eq!(model.materials()[1].base_colour, [1.0; 4]);
    assert_eq!(model.meshes()[0].material, Some(1));
    assert_eq!(model.meshes()[1].material, Some(0));
    assert_eq!(
        model.material_of(&model.meshes()[1]).base_colour,
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(model.material_of(&model.meshes()[0]).base_colour, [1.0; 4]);
}

#[test]
fn primitive_without_material_uses_the_gltf_default_white() {
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    scene.materials = vec![Some("unused".into())];
    let (_dir, path) = write_gltf(&scene);

    let model = load_model(&path).expect("model loads");

    assert_eq!(model.meshes()[0].material, None);
    assert_eq!(
        model.material_of(&model.meshes()[0]),
        &ModelMaterial::GLTF_DEFAULT
    );
    assert_eq!(ModelMaterial::GLTF_DEFAULT.base_colour, [1.0; 4]);
}

fn winding(prim: Prim, scale: [f32; 3]) -> Vec<u32> {
    let mut scene = Scene::with_meshes(vec![vec![prim]]);
    scene.nodes[0].xform = Xform::scale(scale);
    let (_dir, path) = write_gltf(&scene);
    let model = load_model(&path).expect("model loads");
    model.meshes()[0].mesh.indices().to_vec()
}

#[test]
fn mirrored_node_flips_its_mesh_winding() {
    let upright = winding(floor_prim(0.0), [1.0, 1.0, 1.0]);
    let doubled = winding(floor_prim(0.0), [2.0, 2.0, 2.0]);
    let mirrored = winding(floor_prim(0.0), [-1.0, 1.0, 1.0]);

    assert_eq!(upright, vec![0, 1, 2, 0, 2, 3]);
    assert_eq!(doubled, upright);
    assert_eq!(mirrored, vec![0, 2, 1, 0, 3, 2]);
}

#[test]
fn node_reached_twice_fails_naming_the_path() {
    let mut scene = Scene::new();
    scene.meshes = vec![vec![cube_prim()]];
    scene.nodes = vec![
        Node {
            children: vec![1, 2],
            ..Node::default()
        },
        Node {
            children: vec![2],
            ..Node::default()
        },
        Node {
            mesh: Some(0),
            ..Node::default()
        },
    ];
    scene.roots = vec![0];
    let (_dir, path) = write_gltf(&scene);

    let err = load_model(&path).expect_err("shared child refused");

    assert!(matches!(err.kind(), LevelErrorKind::NotGltf(_)), "{err}");
    assert_eq!(err.path(), path);
}

#[test]
fn non_triangle_primitive_is_skipped() {
    let scene = Scene::with_meshes(vec![vec![line_prim(), floor_prim(0.0)]]);
    let (_dir, path) = write_gltf(&scene);

    let model = load_model(&path).expect("model loads");

    assert_eq!(model.meshes().len(), 1);
    assert_eq!(model.meshes()[0].mesh.positions().len(), 4);
    assert_eq!(model.meshes()[0].node, 0);
}
