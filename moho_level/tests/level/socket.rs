use std::f32::consts::FRAC_PI_2;
use std::sync::LazyLock;

use glam::{Affine3A, Quat, Vec3};
use moho_level::{Assembly, Model, SocketError, load_model};
use proptest::prelude::*;

use crate::support::{Node, Scene, Xform, cube_prim, floor_prim, write_gltf};

const RIFLE: usize = 0;
const SCOPE: usize = 1;
const LASER: usize = 2;
const OTHER_SCOPE: usize = 3;
const FORKED: usize = 4;

const QUARTER_TURN_Z: [f32; 4] = [
    0.0,
    0.0,
    std::f32::consts::FRAC_1_SQRT_2,
    std::f32::consts::FRAC_1_SQRT_2,
];

fn named(name: &str, xform: Xform, mesh: Option<usize>, children: Vec<usize>) -> Node {
    Node {
        xform,
        mesh,
        children,
        name: Some(name.to_owned()),
        ..Node::default()
    }
}

fn load(meshes: Vec<Vec<crate::support::Prim>>, nodes: Vec<Node>) -> Model {
    let mut scene = Scene::new();
    scene.meshes = meshes;
    scene.nodes = nodes;
    scene.roots = vec![0];
    let (_dir, path) = write_gltf(&scene);
    load_model(&path).expect("model loads")
}

/// Index = key: rifle, scope, laser, other scope, forked.
fn models() -> Vec<Model> {
    let rifle = load(
        vec![vec![cube_prim()], vec![floor_prim(0.0)]],
        vec![
            named(
                "body",
                Xform::translate([0.0, 0.0, 3.0]),
                Some(0),
                vec![1, 2],
            ),
            named(
                "rail",
                Xform::Trs {
                    t: [0.0, 1.0, 0.0],
                    r: QUARTER_TURN_Z,
                    s: [1.0; 3],
                },
                None,
                vec![],
            ),
            named("grip", Xform::translate([1.0, 0.0, 0.0]), Some(1), vec![]),
        ],
    );
    let scope = load(
        vec![vec![cube_prim()]],
        vec![
            named("tube", Xform::translate([0.0, 0.0, 0.25]), Some(0), vec![1]),
            named("mount", Xform::translate([0.0, 0.5, 0.0]), None, vec![]),
        ],
    );
    let laser = load(
        vec![vec![cube_prim()]],
        vec![named(
            "beam",
            Xform::translate([0.1, 0.0, 0.0]),
            Some(0),
            vec![],
        )],
    );
    let other_scope = load(
        vec![vec![floor_prim(0.0)]],
        vec![named(
            "tube",
            Xform::translate([0.0, 0.0, 0.5]),
            Some(0),
            vec![],
        )],
    );
    // Array order differs from walk order: "rail" at index 1 is walked after the one
    // under "arm" (index 2 -> child 3).
    let forked = load(
        vec![vec![cube_prim()]],
        vec![
            named("root", Xform::Identity, Some(0), vec![2, 1]),
            named("rail", Xform::translate([0.0, 0.0, 9.0]), None, vec![]),
            named("arm", Xform::translate([0.0, 4.0, 0.0]), None, vec![3]),
            named("rail", Xform::translate([2.0, 0.0, 0.0]), None, vec![]),
        ],
    );
    vec![rifle, scope, laser, other_scope, forked]
}

fn near(actual: Affine3A, expected: Affine3A) {
    assert!(
        actual.abs_diff_eq(expected, 1e-4),
        "{actual:?} != {expected:?}"
    );
}

fn rest(models: &[Model], key: usize, node: &str) -> Affine3A {
    models[key].node(node).expect("node exists").rest
}

fn moved_and_turned() -> Affine3A {
    Affine3A::from_rotation_translation(Quat::from_rotation_y(FRAC_PI_2), Vec3::new(5.0, -2.0, 1.0))
}

fn offset() -> Affine3A {
    Affine3A::from_rotation_translation(Quat::from_rotation_x(0.3), Vec3::new(0.0, 0.0, 0.2))
}

fn unknown_socket(err: &SocketError) -> &str {
    let SocketError::UnknownSocket { socket } = err else {
        panic!("expected UnknownSocket, got {err:?}");
    };
    socket
}

#[test]
fn a_part_is_placed_at_root_times_socket_rest_times_offset() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);
    rifle
        .attach(&lookup, "rail", Assembly::new(SCOPE), offset())
        .expect("rail exists");
    let root = moved_and_turned();

    let placed = rifle.placements(&lookup, root);

    assert_eq!(placed.len(), 3);
    let scope_root = root * rest(&models, RIFLE, "rail") * offset();
    let (key, mesh, transform) = placed[2];
    assert_eq!((key, mesh), (SCOPE, 0));
    near(transform, scope_root * rest(&models, SCOPE, "tube"));
    let (_, _, rifle_body) = placed[0];
    near(rifle_body, root * rest(&models, RIFLE, "body"));
}

#[test]
fn a_nested_part_composes_both_sockets() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);
    let off_scope = Affine3A::from_translation(Vec3::new(0.0, 0.1, 0.0));
    let off_laser = Affine3A::from_rotation_translation(Quat::from_rotation_z(0.7), Vec3::X);
    rifle
        .attach(&lookup, "rail", Assembly::new(SCOPE), off_scope)
        .expect("rail exists");
    rifle
        .part_mut(&lookup, "rail")
        .expect("rail exists")
        .expect("scope is attached")
        .attach(&lookup, "mount", Assembly::new(LASER), off_laser)
        .expect("mount exists");
    let root = moved_and_turned();

    let placed = rifle.placements(&lookup, root);

    let (key, mesh, transform) = *placed.last().expect("placements");
    assert_eq!((key, mesh), (LASER, 0));
    let scope_root = root * rest(&models, RIFLE, "rail") * off_scope;
    let laser_root = scope_root * rest(&models, SCOPE, "mount") * off_laser;
    near(transform, laser_root * rest(&models, LASER, "beam"));
    assert_eq!(placed.len(), 4);
}

#[test]
fn attaching_at_an_occupied_socket_replaces_the_part_and_returns_the_old_one() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);
    let old_offset = Affine3A::from_translation(Vec3::new(0.0, 0.1, 0.0));
    let new_offset = offset();
    let root = moved_and_turned();
    let first = rifle
        .attach(&lookup, "rail", Assembly::new(SCOPE), old_offset)
        .expect("rail exists");

    let replaced = rifle
        .attach(&lookup, "rail", Assembly::new(OTHER_SCOPE), new_offset)
        .expect("rail exists");

    assert!(first.is_none());
    assert_eq!(replaced, Some(Assembly::new(SCOPE)));
    let placed = rifle.placements(&lookup, root);
    let keys: Vec<usize> = placed.iter().map(|p| p.0).collect();
    assert_eq!(keys, vec![RIFLE, RIFLE, OTHER_SCOPE]);
    let expected =
        root * rest(&models, RIFLE, "rail") * new_offset * rest(&models, OTHER_SCOPE, "tube");
    near(placed[2].2, expected);
}

#[test]
fn replacing_a_part_keeps_its_place_in_attach_order() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);
    rifle
        .attach(&lookup, "rail", Assembly::new(SCOPE), Affine3A::IDENTITY)
        .expect("rail exists");
    rifle
        .attach(&lookup, "grip", Assembly::new(LASER), Affine3A::IDENTITY)
        .expect("grip exists");

    rifle
        .attach(
            &lookup,
            "rail",
            Assembly::new(OTHER_SCOPE),
            Affine3A::IDENTITY,
        )
        .expect("rail exists");

    let keys: Vec<usize> = rifle
        .placements(&lookup, Affine3A::IDENTITY)
        .iter()
        .map(|p| p.0)
        .collect();
    assert_eq!(keys, vec![RIFLE, RIFLE, OTHER_SCOPE, LASER]);
}

#[test]
fn a_detached_part_is_returned_and_no_longer_placed() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);
    rifle
        .attach(&lookup, "rail", Assembly::new(SCOPE), Affine3A::IDENTITY)
        .expect("rail exists");

    let removed = rifle.detach(&lookup, "rail").expect("rail exists");

    assert_eq!(removed, Some(Assembly::new(SCOPE)));
    let keys: Vec<usize> = rifle
        .placements(&lookup, Affine3A::IDENTITY)
        .iter()
        .map(|p| p.0)
        .collect();
    assert_eq!(keys, vec![RIFLE, RIFLE]);
    assert_eq!(rifle, Assembly::new(RIFLE));
}

#[test]
fn two_assemblies_of_one_model_place_only_their_own_parts() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut armed = Assembly::new(RIFLE);
    let bare = Assembly::new(RIFLE);
    armed
        .attach(&lookup, "rail", Assembly::new(SCOPE), Affine3A::IDENTITY)
        .expect("rail exists");

    let armed_keys: Vec<usize> = armed
        .placements(&lookup, Affine3A::IDENTITY)
        .iter()
        .map(|p| p.0)
        .collect();
    let bare_keys: Vec<usize> = bare
        .placements(&lookup, Affine3A::IDENTITY)
        .iter()
        .map(|p| p.0)
        .collect();

    assert!(armed_keys.contains(&SCOPE));
    assert_eq!(bare_keys, vec![RIFLE, RIFLE]);
}

#[test]
fn attaching_at_a_missing_node_fails_naming_it_and_leaves_the_assembly_unchanged() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);
    rifle
        .attach(&lookup, "rail", Assembly::new(SCOPE), offset())
        .expect("rail exists");
    let before = rifle.clone();

    let err = rifle
        .attach(&lookup, "bayonet", Assembly::new(LASER), Affine3A::IDENTITY)
        .expect_err("the rifle has no bayonet node");

    assert_eq!(unknown_socket(&err), "bayonet");
    assert_eq!(err.to_string(), "model has no node named `bayonet`");
    assert_eq!(rifle, before);
}

#[test]
fn placements_list_the_model_first_then_parts_depth_first_in_attach_order() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);
    rifle
        .attach(&lookup, "grip", Assembly::new(SCOPE), Affine3A::IDENTITY)
        .expect("grip exists");
    rifle
        .part_mut(&lookup, "grip")
        .expect("grip exists")
        .expect("scope is attached")
        .attach(&lookup, "mount", Assembly::new(LASER), Affine3A::IDENTITY)
        .expect("mount exists");
    rifle
        .attach(
            &lookup,
            "rail",
            Assembly::new(OTHER_SCOPE),
            Affine3A::IDENTITY,
        )
        .expect("rail exists");

    let placed = rifle.placements(&lookup, Affine3A::IDENTITY);

    let order: Vec<(usize, usize)> = placed.iter().map(|p| (p.0, p.1)).collect();
    assert_eq!(
        order,
        vec![
            (RIFLE, 0),
            (RIFLE, 1),
            (SCOPE, 0),
            (LASER, 0),
            (OTHER_SCOPE, 0)
        ]
    );
}

#[test]
fn a_duplicate_socket_name_uses_the_first_node_in_walk_order() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut copy = Assembly::new(FORKED);
    copy.attach(&lookup, "rail", Assembly::new(LASER), Affine3A::IDENTITY)
        .expect("rail exists");

    let placed = copy.placements(&lookup, Affine3A::IDENTITY);

    let walked_first = Affine3A::from_translation(Vec3::new(2.0, 4.0, 0.0));
    let (key, _, transform) = placed[1];
    assert_eq!(key, LASER);
    near(transform, walked_first * rest(&models, LASER, "beam"));
}

#[test]
fn detach_and_part_mut_at_a_missing_node_fail_naming_it() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);
    rifle
        .attach(&lookup, "rail", Assembly::new(SCOPE), Affine3A::IDENTITY)
        .expect("rail exists");
    let before = rifle.clone();

    let detach = rifle
        .detach(&lookup, "bayonet")
        .expect_err("no bayonet node");
    let reach = rifle
        .part_mut(&lookup, "bayonet")
        .map(|_| ())
        .expect_err("no bayonet node");

    assert_eq!(unknown_socket(&detach), "bayonet");
    assert_eq!(unknown_socket(&reach), "bayonet");
    assert_eq!(rifle, before);
}

#[test]
fn detaching_an_empty_socket_returns_none() {
    let models = models();
    let lookup = |k: usize| &models[k];
    let mut rifle = Assembly::new(RIFLE);

    let detached = rifle.detach(&lookup, "rail").expect("rail exists");
    let reached = rifle
        .part_mut(&lookup, "rail")
        .expect("rail exists")
        .is_none();

    assert_eq!(detached, None);
    assert!(reached);
    assert_eq!(rifle, Assembly::new(RIFLE));
}

fn arbitrary_affine() -> impl Strategy<Value = Affine3A> {
    (
        (-10.0f32..10.0, -10.0f32..10.0, -10.0f32..10.0),
        (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
        -3.0f32..3.0,
    )
        .prop_map(|((x, y, z), (ax, ay, az), angle)| {
            let axis = Vec3::new(ax, ay, az).try_normalize().unwrap_or(Vec3::Y);
            Affine3A::from_rotation_translation(
                Quat::from_axis_angle(axis, angle),
                Vec3::new(x, y, z),
            )
        })
}

/// Built once: writing and loading glTF files per proptest case is the slow part.
static SHARED_MODELS: LazyLock<Vec<Model>> = LazyLock::new(models);

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn moving_the_root_by_t_moves_every_placement_by_t(
        root in arbitrary_affine(),
        t in arbitrary_affine(),
    ) {
        let lookup = |k: usize| &SHARED_MODELS[k];
        let mut rifle = Assembly::new(RIFLE);
        rifle.attach(&lookup, "rail", Assembly::new(SCOPE), offset()).expect("rail exists");
        rifle
            .part_mut(&lookup, "rail")
            .expect("rail exists")
            .expect("scope is attached")
            .attach(&lookup, "mount", Assembly::new(LASER), offset())
            .expect("mount exists");

        let before = rifle.placements(&lookup, root);
        let after = rifle.placements(&lookup, t * root);

        prop_assert_eq!(before.len(), 4);
        prop_assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            prop_assert_eq!((b.0, b.1), (a.0, a.1));
            let expected = t * b.2;
            prop_assert!(
                a.2.abs_diff_eq(expected, 1e-3),
                "{:?} != {:?}",
                a.2,
                expected
            );
        }
    }
}
