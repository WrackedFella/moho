use std::f32::consts::FRAC_PI_2;
use std::sync::LazyLock;

use glam::{Affine3A, Quat, Vec3};
use moho_level::{Assembly, Model, SocketError, load_model};
use proptest::prelude::*;

use crate::support::{Node, Scene, Xform, cube_prim, floor_prim, write_gltf};
use moho_level::anim::{ClipPlayer, Playback, Pose};

use crate::support::humanoid::{BODY_MESH, HAND_MESH, HumanoidSpec, write_humanoid};

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

const CHARACTER: usize = 5;

const TICK: f32 = 1.0 / 60.0;

/// [`models`] plus the humanoid at key [`CHARACTER`].
fn with_character() -> Vec<Model> {
    let mut all = models();
    let (_dir, path) = write_humanoid(&HumanoidSpec::default());
    all.push(load_model(&path).expect("humanoid loads"));
    all
}

/// The character's `walk` pose after `ticks` ticks from tick 0.
fn walk_pose(models: &[Model], ticks: usize) -> Pose {
    let model = &models[CHARACTER];
    let mut player = ClipPlayer::new(model, "walk", Playback::Loop).expect("walk exists");
    for _ in 0..ticks {
        player.advance(model, TICK);
    }
    player.pose(model)
}

fn node_index(models: &[Model], key: usize, name: &str) -> usize {
    models[key]
        .nodes()
        .iter()
        .position(|n| n.name.as_deref() == Some(name))
        .expect("node exists")
}

#[test]
fn rifle_rides_hand_r_through_walk() {
    let models = with_character();
    let lookup = |k: usize| &models[k];
    let root = moved_and_turned();

    for socket in ["hand_r", "ik_hand_gun"] {
        let mut character = Assembly::new(CHARACTER);
        character
            .attach(&lookup, socket, Assembly::new(RIFLE), offset())
            .expect("socket exists");
        let node = node_index(&models, CHARACTER, socket);
        let mut rifle_at = Vec::new();

        for ticks in [0, 30, 60] {
            let pose = walk_pose(&models, ticks);

            let placed = character.posed_placements(&lookup, root, &pose);

            assert_eq!(placed.len(), 4, "{socket} tick {ticks}");
            let (key, mesh, rifle_body) = placed[2];
            assert_eq!((key, mesh), (RIFLE, 0));
            assert_eq!(
                rifle_body,
                root * pose.nodes()[node] * offset() * rest(&models, RIFLE, "body"),
                "{socket} tick {ticks}"
            );
            rifle_at.push(rifle_body);
        }

        assert_ne!(rifle_at[0], rifle_at[1], "{socket}");
        assert_ne!(rifle_at[1], rifle_at[2], "{socket}");
        assert_ne!(rifle_at[0], rifle_at[2], "{socket}");
    }
}

#[test]
fn posed_placements_at_rest_pose_equal_placements() {
    let models = with_character();
    let lookup = |k: usize| &models[k];
    let mut character = Assembly::new(CHARACTER);
    character
        .attach(&lookup, "hand_r", Assembly::new(RIFLE), offset())
        .expect("hand_r exists");
    character
        .part_mut(&lookup, "hand_r")
        .expect("hand_r exists")
        .expect("rifle is attached")
        .attach(&lookup, "rail", Assembly::new(SCOPE), offset())
        .expect("rail exists");
    let root = moved_and_turned();

    let posed = character.posed_placements(&lookup, root, &models[CHARACTER].rest_pose());

    let rested = character.placements(&lookup, root);
    assert_eq!(rested.len(), 5);
    assert_eq!(posed, rested);
}

#[test]
fn part_on_a_posed_bone_keeps_its_own_parts_at_rest() {
    let models = with_character();
    let lookup = |k: usize| &models[k];
    let mut character = Assembly::new(CHARACTER);
    let off_scope = Affine3A::from_translation(Vec3::new(0.0, 0.1, 0.0));
    character
        .attach(&lookup, "hand_r", Assembly::new(RIFLE), offset())
        .expect("hand_r exists");
    character
        .part_mut(&lookup, "hand_r")
        .expect("hand_r exists")
        .expect("rifle is attached")
        .attach(&lookup, "rail", Assembly::new(SCOPE), off_scope)
        .expect("rail exists");
    let root = moved_and_turned();
    let hand = node_index(&models, CHARACTER, "hand_r");
    let at_start = walk_pose(&models, 0);
    let at_30 = walk_pose(&models, 30);

    let start = character.posed_placements(&lookup, root, &at_start);
    let placed = character.posed_placements(&lookup, root, &at_30);

    let (key, mesh, scope_tube) = *placed.last().expect("placements");
    assert_eq!((key, mesh), (SCOPE, 0));
    let expected = root
        * at_30.nodes()[hand]
        * offset()
        * rest(&models, RIFLE, "rail")
        * off_scope
        * rest(&models, SCOPE, "tube");
    near(scope_tube, expected);
    assert!(
        !start
            .last()
            .expect("placements")
            .2
            .abs_diff_eq(scope_tube, 1e-3),
        "the scope moves with the hand"
    );
}

#[test]
fn the_characters_own_meshes_follow_the_pose() {
    let models = with_character();
    let lookup = |k: usize| &models[k];
    let character = Assembly::new(CHARACTER);
    let root = moved_and_turned();
    let hand = node_index(&models, CHARACTER, "hand_r");
    let pose = walk_pose(&models, 30);

    let placed = character.posed_placements(&lookup, root, &pose);

    assert_eq!(placed.len(), 2);
    let (key, mesh, on_hand) = placed[HAND_MESH];
    assert_eq!((key, mesh), (CHARACTER, HAND_MESH));
    assert_eq!(on_hand, root * pose.nodes()[hand]);
    let rested = character.placements(&lookup, root);
    assert!(
        !on_hand.abs_diff_eq(rested[HAND_MESH].2, 1e-3),
        "the mesh on hand_r is carried off its rest"
    );
    let body = node_index(&models, CHARACTER, "body");
    assert_eq!(placed[BODY_MESH].2, root * pose.nodes()[body]);
}

#[test]
fn a_yawed_and_moved_root_gives_exact_products_for_meshes_and_a_part() {
    let models = with_character();
    let lookup = |k: usize| &models[k];
    let mut character = Assembly::new(CHARACTER);
    character
        .attach(&lookup, "hand_r", Assembly::new(LASER), offset())
        .expect("hand_r exists");
    let root = Affine3A::from_rotation_translation(
        Quat::from_rotation_y(FRAC_PI_2),
        Vec3::new(-4.0, 0.5, 7.0),
    );
    let pose = walk_pose(&models, 30);
    let hand = node_index(&models, CHARACTER, "hand_r");
    let body = node_index(&models, CHARACTER, "body");

    let placed = character.posed_placements(&lookup, root, &pose);

    assert_eq!(placed.len(), 3);
    assert_eq!(placed[HAND_MESH].2, root * pose.nodes()[hand]);
    assert_eq!(placed[BODY_MESH].2, root * pose.nodes()[body]);
    assert_eq!(
        placed[2].2,
        root * pose.nodes()[hand] * offset() * rest(&models, LASER, "beam")
    );
}

#[test]
fn a_part_two_deep_under_a_posed_hand_sees_the_pose_only_at_the_first_socket() {
    let models = with_character();
    let lookup = |k: usize| &models[k];
    let mut character = Assembly::new(CHARACTER);
    let off_rifle = Affine3A::from_translation(Vec3::new(0.0, 0.2, 0.0));
    let off_scope = Affine3A::from_rotation_translation(Quat::from_rotation_z(0.4), Vec3::Y);
    character
        .attach(&lookup, "hand_r", Assembly::new(RIFLE), off_rifle)
        .expect("hand_r exists");
    character
        .part_mut(&lookup, "hand_r")
        .expect("hand_r exists")
        .expect("rifle is attached")
        .attach(&lookup, "rail", Assembly::new(SCOPE), off_scope)
        .expect("rail exists");
    let root = moved_and_turned();
    let hand = node_index(&models, CHARACTER, "hand_r");
    let pose = walk_pose(&models, 30);

    let placed = character.posed_placements(&lookup, root, &pose);

    let (key, mesh, scope_tube) = *placed.last().expect("placements");
    assert_eq!((key, mesh), (SCOPE, 0));
    assert_eq!(
        scope_tube,
        root * pose.nodes()[hand]
            * off_rifle
            * rest(&models, RIFLE, "rail")
            * off_scope
            * rest(&models, SCOPE, "tube")
    );
}

use std::fs;
use std::path::Path;

use moho_level::{Posed, Released};
use moho_physics::{LooseShape, LooseSpawn, PhysicsWorld};
use serde_json::{Value, json};

const RELOAD_CHARACTER: usize = 0;
const RELOAD_RIFLE: usize = 1;
const MAGAZINE: usize = 2;

/// Seconds `reload_fast` runs; the left hand moves along local +Z and turns about +Y.
const RELOAD_SECONDS: f32 = 4.0;

fn append_accessor(
    doc: &mut Value,
    bin: &mut Vec<u8>,
    values: &[f32],
    count: usize,
    kind: &str,
    bounds: Option<(f32, f32)>,
) -> usize {
    let start = bin.len();
    bin.extend(values.iter().flat_map(|f| f.to_le_bytes()));
    let views = doc["bufferViews"].as_array_mut().expect("buffer views");
    views.push(json!({ "buffer": 0, "byteOffset": start, "byteLength": values.len() * 4 }));
    let view = views.len() - 1;
    let mut accessor =
        json!({ "bufferView": view, "componentType": 5126, "count": count, "type": kind });
    if let Some((min, max)) = bounds {
        accessor["min"] = json!([min]);
        accessor["max"] = json!([max]);
    }
    let accessors = doc["accessors"].as_array_mut().expect("accessors");
    accessors.push(accessor);
    accessors.len() - 1
}

/// Add a `reload_fast` clip that moves `hand_l` and `hand_r` to the humanoid written at `path`.
fn add_reload_fast(path: &Path) {
    let bin_path = path.with_file_name("humanoid.bin");
    let mut bin = fs::read(&bin_path).expect("read bin");
    let mut doc: Value = serde_json::from_slice(&fs::read(path).expect("read gltf")).expect("json");
    let hand_l = doc["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .position(|n| n["name"] == "hand_l")
        .expect("hand_l is written");
    let hand_r = doc["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .position(|n| n["name"] == "hand_r")
        .expect("hand_r is written");
    let times = append_accessor(
        &mut doc,
        &mut bin,
        &[0.0, RELOAD_SECONDS],
        2,
        "SCALAR",
        Some((0.0, RELOAD_SECONDS)),
    );
    let moves = append_accessor(
        &mut doc,
        &mut bin,
        &[-0.25, 0.0, 0.0, -0.25, 0.0, 2.4],
        2,
        "VEC3",
        None,
    );
    let (sin, cos) = 1.0_f32.sin_cos();
    let turns = append_accessor(
        &mut doc,
        &mut bin,
        &[0.0, 0.0, 0.0, 1.0, 0.0, sin, 0.0, cos],
        2,
        "VEC4",
        None,
    );
    let right_moves = append_accessor(
        &mut doc,
        &mut bin,
        &[0.25, 0.0, 0.0, 0.25, 0.3, 0.8],
        2,
        "VEC3",
        None,
    );
    let (right_sin, right_cos) = 0.4_f32.sin_cos();
    let right_turns = append_accessor(
        &mut doc,
        &mut bin,
        &[0.0, 0.0, 0.0, 1.0, right_sin, 0.0, 0.0, right_cos],
        2,
        "VEC4",
        None,
    );
    doc["buffers"][0]["byteLength"] = json!(bin.len());
    doc["animations"]
        .as_array_mut()
        .expect("animations")
        .push(json!({
            "name": "reload_fast",
            "samplers": [
                { "input": times, "output": moves, "interpolation": "LINEAR" },
                { "input": times, "output": turns, "interpolation": "LINEAR" },
                { "input": times, "output": right_moves, "interpolation": "LINEAR" },
                { "input": times, "output": right_turns, "interpolation": "LINEAR" },
            ],
            "channels": [
                { "sampler": 0, "target": { "node": hand_l, "path": "translation" } },
                { "sampler": 1, "target": { "node": hand_l, "path": "rotation" } },
                { "sampler": 2, "target": { "node": hand_r, "path": "translation" } },
                { "sampler": 3, "target": { "node": hand_r, "path": "rotation" } },
            ],
        }));
    fs::write(&bin_path, bin).expect("write bin");
    fs::write(path, serde_json::to_vec_pretty(&doc).expect("json")).expect("write gltf");
}

/// Index = key: the humanoid with `reload_fast`, a rifle with a `magazine` node and a `rail`
/// node, a magazine.
fn reload_models() -> Vec<Model> {
    let (_dir, path) = write_humanoid(&HumanoidSpec::default());
    add_reload_fast(&path);
    let character = load_model(&path).expect("humanoid loads");
    let rifle = load(
        vec![vec![cube_prim()]],
        vec![
            named(
                "body",
                Xform::translate([0.0, 0.0, 3.0]),
                Some(0),
                vec![1, 2],
            ),
            named(
                "magazine",
                Xform::translate([0.0, -0.1, 0.05]),
                None,
                vec![],
            ),
            named("rail", Xform::translate([0.0, 0.1, 0.0]), None, vec![]),
        ],
    );
    let magazine = load(
        vec![vec![cube_prim()]],
        vec![named(
            "mag",
            Xform::translate([0.0, 0.0, 0.02]),
            Some(0),
            vec![],
        )],
    );
    vec![character, rifle, magazine]
}

fn rifle_offset() -> Affine3A {
    offset()
}

fn magazine_offset() -> Affine3A {
    Affine3A::from_rotation_translation(Quat::from_rotation_z(0.2), Vec3::new(0.02, -0.03, 0.01))
}

fn grip() -> Affine3A {
    Affine3A::from_rotation_translation(Quat::from_rotation_y(0.5), Vec3::new(0.03, 0.01, -0.02))
}

/// The character with the rifle on `hand_r` and the magazine in the rifle.
fn armed<'m>(lookup: &impl Fn(usize) -> &'m Model) -> Assembly<usize> {
    let mut character = Assembly::new(RELOAD_CHARACTER);
    character
        .attach(
            lookup,
            "hand_r",
            Assembly::new(RELOAD_RIFLE),
            rifle_offset(),
        )
        .expect("hand_r exists");
    character
        .part_mut(lookup, "hand_r")
        .expect("hand_r exists")
        .expect("rifle is attached")
        .attach(
            lookup,
            "magazine",
            Assembly::new(MAGAZINE),
            magazine_offset(),
        )
        .expect("magazine exists");
    character
}

/// The character's `reload_fast` playback after `ticks` ticks from tick 0.
fn reload_player(models: &[Model], ticks: usize) -> ClipPlayer {
    let model = &models[RELOAD_CHARACTER];
    let mut player = ClipPlayer::new(model, "reload_fast", Playback::Once).expect("reload_fast");
    for _ in 0..ticks {
        player.advance(model, TICK);
    }
    player
}

fn reload_pose(models: &[Model], ticks: usize) -> Pose {
    reload_player(models, ticks).pose(&models[RELOAD_CHARACTER])
}

fn within(actual: Affine3A, expected: Affine3A, epsilon: f32) {
    assert!(
        actual.abs_diff_eq(expected, epsilon),
        "{actual:?} != {expected:?}"
    );
}

fn vec_near(actual: Vec3, expected: Vec3, epsilon: f32) {
    assert!(
        actual.abs_diff_eq(expected, epsilon),
        "{actual} != {expected}"
    );
}

fn placed_transform(placed: &[(usize, usize, Affine3A)], key: usize) -> Affine3A {
    placed
        .iter()
        .find(|p| p.0 == key)
        .expect("the part is placed")
        .2
}

fn empty_socket(err: &SocketError) -> &str {
    let SocketError::EmptySocket { socket } = err else {
        panic!("expected EmptySocket, got {err:?}");
    };
    socket
}

#[test]
fn magazine_moved_to_hand_l_at_tick_110_stays_put_then_follows_hand_l() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = armed(&lookup);
    let root = moved_and_turned();
    let hand_r = node_index(&models, RELOAD_CHARACTER, "hand_r");
    let hand_l = node_index(&models, RELOAD_CHARACTER, "hand_l");
    let at_110 = reload_pose(&models, 110);
    let held = root
        * at_110.nodes()[hand_r]
        * rifle_offset()
        * rest(&models, RELOAD_RIFLE, "magazine")
        * magazine_offset();
    let new_offset = at_110.nodes()[hand_l].inverse()
        * at_110.nodes()[hand_r]
        * rifle_offset()
        * rest(&models, RELOAD_RIFLE, "magazine")
        * magazine_offset();
    let mag_node = rest(&models, MAGAZINE, "mag");

    let displaced = character
        .move_part(&lookup, &["hand_r", "magazine"], &["hand_l"], &at_110)
        .expect("both sockets exist");

    assert_eq!(displaced, None);
    let at_move = character.posed_placements(&lookup, root, &at_110);
    within(placed_transform(&at_move, MAGAZINE), held * mag_node, 1e-5);
    let mut seen = vec![];
    for tick in 110..=130 {
        let pose = reload_pose(&models, tick);
        let expected = root * pose.nodes()[hand_l] * new_offset;

        let placed = character.posed_placements(&lookup, root, &pose);

        within(
            placed_transform(&placed, MAGAZINE),
            expected * mag_node,
            1e-5,
        );
        within(
            character
                .part_transform(&lookup, &["hand_l"], root, &pose)
                .expect("hand_l holds the magazine"),
            expected,
            1e-5,
        );
        seen.push(placed_transform(&placed, MAGAZINE));
    }
    assert!(
        !seen[0].abs_diff_eq(seen[20], 0.05),
        "the magazine follows the moving hand"
    );
    let rifle = character
        .part_mut(&lookup, "hand_r")
        .expect("hand_r exists")
        .expect("rifle is still on hand_r");
    assert_eq!(*rifle, Assembly::new(RELOAD_RIFLE));
}

#[test]
fn moving_a_part_between_nested_sockets_keeps_its_transform() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = armed(&lookup);
    let root = moved_and_turned();
    let hand_r = node_index(&models, RELOAD_CHARACTER, "hand_r");
    let pose = reload_pose(&models, 40);
    let held = root
        * pose.nodes()[hand_r]
        * rifle_offset()
        * rest(&models, RELOAD_RIFLE, "magazine")
        * magazine_offset();
    let new_offset = rest(&models, RELOAD_RIFLE, "rail").inverse()
        * rest(&models, RELOAD_RIFLE, "magazine")
        * magazine_offset();

    let displaced = character
        .move_part(&lookup, &["hand_r", "magazine"], &["hand_r", "rail"], &pose)
        .expect("both sockets exist");

    assert_eq!(displaced, None);
    let moved = character
        .part_transform(&lookup, &["hand_r", "rail"], root, &pose)
        .expect("rail holds the magazine");
    within(moved, held, 1e-5);
    let later = reload_pose(&models, 90);
    within(
        character
            .part_transform(&lookup, &["hand_r", "rail"], root, &later)
            .expect("rail holds the magazine"),
        root * later.nodes()[hand_r]
            * rifle_offset()
            * rest(&models, RELOAD_RIFLE, "rail")
            * new_offset,
        1e-5,
    );
}

#[test]
fn released_magazine_spawns_a_loose_object_at_its_pose_and_velocity() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = Assembly::new(RELOAD_CHARACTER);
    character
        .attach(&lookup, "hand_l", Assembly::new(MAGAZINE), grip())
        .expect("hand_l exists");
    let root = moved_and_turned();
    let hand = node_index(&models, RELOAD_CHARACTER, "hand_l");
    let last_tick = reload_player(&models, 114);
    let mut this_tick = last_tick.clone();
    this_tick.advance(&models[RELOAD_CHARACTER], TICK);
    let before_pose = last_tick.pose(&models[RELOAD_CHARACTER]);
    let now_pose = this_tick.pose(&models[RELOAD_CHARACTER]);
    let at = |pose: &Pose| root * pose.nodes()[hand] * grip();
    let (expected_now, expected_before) = (at(&now_pose), at(&before_pose));
    let rotation_of = |a: Affine3A| Quat::from_mat3a(&a.matrix3).normalize();
    let mut turn = (rotation_of(expected_now) * rotation_of(expected_before).inverse()).normalize();
    if turn.w < 0.0 {
        turn = -turn;
    }
    let expected_linear = Vec3::from(expected_now.translation - expected_before.translation) / TICK;
    let expected_angular = turn.to_scaled_axis() / TICK;

    let released = character
        .release(
            &lookup,
            &["hand_l"],
            Posed {
                root,
                pose: &now_pose,
            },
            Posed {
                root,
                pose: &before_pose,
            },
            TICK,
        )
        .expect("hand_l holds the magazine");

    let spawn = LooseSpawn {
        position: Vec3::from(released.transform.translation),
        rotation: rotation_of(released.transform),
        linear_velocity: released.linear_velocity,
        angular_velocity: released.angular_velocity,
        ..LooseSpawn::new(
            LooseShape::Box {
                half_extents: Vec3::splat(0.05),
            },
            0.2,
        )
    };
    let mut world = PhysicsWorld::new();
    let id = world.spawn_loose(&spawn);
    let state = world.loose(id).expect("spawned");
    vec_near(state.position, Vec3::from(expected_now.translation), 1e-4);
    assert!(
        state.rotation.angle_between(rotation_of(expected_now)) < 1e-4,
        "{:?} != {:?}",
        state.rotation,
        rotation_of(expected_now)
    );
    vec_near(state.linear_velocity, expected_linear, 1e-4);
    vec_near(state.angular_velocity, expected_angular, 1e-4);
    assert!(
        expected_linear.length() > 0.1,
        "the hand is moving at the release tick"
    );
    let angular = state.angular_velocity;
    assert!(
        angular.length() > 0.3 && angular.length() < 0.7,
        "the hand turns about half a radian a second, got {angular}"
    );
    vec_near(angular.normalize(), Vec3::Y, 1e-3);
}

#[test]
fn released_magazine_is_no_longer_placed() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = Assembly::new(RELOAD_CHARACTER);
    character
        .attach(&lookup, "hand_l", Assembly::new(MAGAZINE), grip())
        .expect("hand_l exists");
    let root = moved_and_turned();
    let before_pose = reload_pose(&models, 114);
    let now_pose = reload_pose(&models, 115);
    let held = character.placements(&lookup, root);
    assert!(held.iter().any(|p| p.0 == MAGAZINE));

    let Released { part, .. } = character
        .release(
            &lookup,
            &["hand_l"],
            Posed {
                root,
                pose: &now_pose,
            },
            Posed {
                root,
                pose: &before_pose,
            },
            TICK,
        )
        .expect("hand_l holds the magazine");

    assert_eq!(part, Assembly::new(MAGAZINE));
    assert_eq!(character, Assembly::new(RELOAD_CHARACTER));
    let keys: Vec<usize> = character
        .posed_placements(&lookup, root, &now_pose)
        .iter()
        .map(|p| p.0)
        .collect();
    assert_eq!(keys, vec![RELOAD_CHARACTER, RELOAD_CHARACTER]);
}

#[test]
fn released_velocity_includes_the_character_root_moving() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = Assembly::new(RELOAD_CHARACTER);
    character
        .attach(&lookup, "hand_l", Assembly::new(MAGAZINE), grip())
        .expect("hand_l exists");
    let rest_pose = models[RELOAD_CHARACTER].rest_pose();
    let hand = node_index(&models, RELOAD_CHARACTER, "hand_l");
    let root_before =
        Affine3A::from_rotation_translation(Quat::from_rotation_y(0.4), Vec3::new(5.0, -2.0, 1.0));
    let root_now = Affine3A::from_rotation_translation(
        Quat::from_rotation_y(0.41),
        Vec3::new(5.05, -2.0, 1.02),
    );
    let at = |root: Affine3A| root * rest_pose.nodes()[hand] * grip();
    let expected_linear = Vec3::from(at(root_now).translation - at(root_before).translation) / TICK;

    let released = character
        .release(
            &lookup,
            &["hand_l"],
            Posed {
                root: root_now,
                pose: &rest_pose,
            },
            Posed {
                root: root_before,
                pose: &rest_pose,
            },
            TICK,
        )
        .expect("hand_l holds the magazine");

    within(released.transform, at(root_now), 1e-5);
    vec_near(released.linear_velocity, expected_linear, 1e-3);
    assert!(expected_linear.length() > 1.0, "the root is moving");
    vec_near(released.angular_velocity, Vec3::new(0.0, 0.6, 0.0), 1e-2);
}

#[test]
fn move_to_a_node_under_the_part_is_refused_and_changes_nothing() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = armed(&lookup);
    character
        .attach(&lookup, "hand_l", Assembly::new(MAGAZINE), grip())
        .expect("hand_l exists");
    let before = character.clone();
    let pose = reload_pose(&models, 110);

    let cases: [(&[&str], &[&str]); 4] = [
        (&["hand_r"], &["hand_r", "magazine"]),
        (&["hand_r"], &["hand_r", "rail"]),
        (&["hand_l"], &["hand_l"]),
        (&["hand_r"], &["hand_r"]),
    ];
    for (from, to) in cases {
        let err = character
            .move_part(&lookup, from, to, &pose)
            .expect_err("a part cannot go into itself");

        assert!(
            matches!(err, SocketError::IntoItself),
            "{from:?} to {to:?}: {err:?}"
        );
        assert_eq!(character, before, "{from:?} to {to:?}");
    }
}

#[test]
fn move_from_an_empty_socket_is_refused_naming_it() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = armed(&lookup);
    let before = character.clone();
    let pose = reload_pose(&models, 110);

    let empty_hand = character
        .move_part(&lookup, &["hand_l"], &["hand_r", "rail"], &pose)
        .expect_err("hand_l holds nothing");
    let empty_rail = character
        .move_part(&lookup, &["hand_r", "rail"], &["hand_l"], &pose)
        .expect_err("the rail holds nothing");

    assert_eq!(empty_socket(&empty_hand), "hand_l");
    assert_eq!(empty_socket(&empty_rail), "rail");
    assert_eq!(character, before);
}

#[test]
fn move_to_an_unknown_or_empty_destination_is_refused_and_changes_nothing() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = armed(&lookup);
    let before = character.clone();
    let pose = reload_pose(&models, 110);

    let unknown = character
        .move_part(&lookup, &["hand_r", "magazine"], &["bayonet"], &pose)
        .expect_err("the character has no bayonet node");
    let unknown_from = character
        .move_part(&lookup, &["bayonet"], &["hand_l"], &pose)
        .expect_err("the character has no bayonet node");
    let through_empty = character
        .move_part(&lookup, &["hand_r", "magazine"], &["hand_l", "rail"], &pose)
        .expect_err("hand_l holds no rifle to carry a rail");

    assert_eq!(unknown_socket(&unknown), "bayonet");
    assert_eq!(unknown_socket(&unknown_from), "bayonet");
    assert_eq!(empty_socket(&through_empty), "hand_l");
    assert_eq!(character, before);
}

#[test]
fn move_to_an_occupied_socket_returns_the_displaced_part() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = armed(&lookup);
    character
        .attach(
            &lookup,
            "hand_l",
            Assembly::new(RELOAD_RIFLE),
            Affine3A::IDENTITY,
        )
        .expect("hand_l exists");
    let root = moved_and_turned();
    let hand_r = node_index(&models, RELOAD_CHARACTER, "hand_r");
    let pose = reload_pose(&models, 110);
    let held = root
        * pose.nodes()[hand_r]
        * rifle_offset()
        * rest(&models, RELOAD_RIFLE, "magazine")
        * magazine_offset();

    let displaced = character
        .move_part(&lookup, &["hand_r", "magazine"], &["hand_l"], &pose)
        .expect("both sockets exist");

    assert_eq!(displaced, Some(Assembly::new(RELOAD_RIFLE)));
    let placed = character.posed_placements(&lookup, root, &pose);
    let keys: Vec<usize> = placed.iter().map(|p| p.0).collect();
    assert_eq!(
        keys,
        vec![RELOAD_CHARACTER, RELOAD_CHARACTER, RELOAD_RIFLE, MAGAZINE]
    );
    within(
        placed_transform(&placed, MAGAZINE),
        held * rest(&models, MAGAZINE, "mag"),
        1e-5,
    );
}

#[test]
fn part_transform_at_rest_pose_matches_placements() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let character = armed(&lookup);
    let root = moved_and_turned();
    let rest_pose = models[RELOAD_CHARACTER].rest_pose();
    let placed = character.placements(&lookup, root);
    let mag_node = rest(&models, MAGAZINE, "mag");

    let magazine = character
        .part_transform(&lookup, &["hand_r", "magazine"], root, &rest_pose)
        .expect("the magazine is attached");
    let rifle = character
        .part_transform(&lookup, &["hand_r"], root, &rest_pose)
        .expect("the rifle is attached");

    within(
        magazine * mag_node,
        placed_transform(&placed, MAGAZINE),
        1e-4,
    );
    within(
        rifle * rest(&models, RELOAD_RIFLE, "body"),
        placed_transform(&placed, RELOAD_RIFLE),
        1e-4,
    );
}

#[test]
fn part_transform_under_a_pose_poses_only_the_first_socket() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let character = armed(&lookup);
    let root = moved_and_turned();
    let hand_r = node_index(&models, RELOAD_CHARACTER, "hand_r");
    let pose = reload_pose(&models, 60);
    let at_rest = models[RELOAD_CHARACTER].rest_pose();
    assert!(
        !pose.nodes()[hand_r].abs_diff_eq(at_rest.nodes()[hand_r], 0.05),
        "hand_r is off its rest at this tick"
    );

    let magazine = character
        .part_transform(&lookup, &["hand_r", "magazine"], root, &pose)
        .expect("the magazine is attached");

    within(
        magazine,
        root * pose.nodes()[hand_r]
            * rifle_offset()
            * rest(&models, RELOAD_RIFLE, "magazine")
            * magazine_offset(),
        1e-5,
    );
    let placed = character.posed_placements(&lookup, root, &pose);
    within(
        magazine * rest(&models, MAGAZINE, "mag"),
        placed_transform(&placed, MAGAZINE),
        1e-4,
    );
}

#[test]
fn part_transform_names_the_step_that_fails() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let character = armed(&lookup);
    let pose = reload_pose(&models, 60);
    let root = Affine3A::IDENTITY;

    let empty = character
        .part_transform(&lookup, &["hand_r", "rail"], root, &pose)
        .expect_err("the rail holds nothing");
    let unknown = character
        .part_transform(&lookup, &["hand_r", "bayonet"], root, &pose)
        .expect_err("the rifle has no bayonet node");

    assert_eq!(empty_socket(&empty), "rail");
    assert_eq!(unknown_socket(&unknown), "bayonet");
}

#[test]
fn release_from_an_empty_or_unknown_socket_is_refused_and_changes_nothing() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = armed(&lookup);
    let before = character.clone();
    let pose = reload_pose(&models, 110);
    let posed = Posed {
        root: Affine3A::IDENTITY,
        pose: &pose,
    };

    let empty = character
        .release(&lookup, &["hand_l"], posed, posed, TICK)
        .expect_err("hand_l holds nothing");
    let unknown = character
        .release(&lookup, &["hand_r", "bayonet"], posed, posed, TICK)
        .expect_err("the rifle has no bayonet node");

    assert_eq!(empty_socket(&empty), "hand_l");
    assert_eq!(unknown_socket(&unknown), "bayonet");
    assert_eq!(character, before);
}

#[test]
fn released_nested_magazine_leaves_the_rifle_on_the_hand() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let mut character = armed(&lookup);
    let root = moved_and_turned();
    let before_pose = reload_pose(&models, 114);
    let now_pose = reload_pose(&models, 115);
    let path = ["hand_r", "magazine"];
    let at_now = character
        .part_transform(&lookup, &path, root, &now_pose)
        .expect("the magazine is attached");
    let at_before = character
        .part_transform(&lookup, &path, root, &before_pose)
        .expect("the magazine is attached");

    let released = character
        .release(
            &lookup,
            &path,
            Posed {
                root,
                pose: &now_pose,
            },
            Posed {
                root,
                pose: &before_pose,
            },
            TICK,
        )
        .expect("the magazine is attached");

    within(released.transform, at_now, 1e-5);
    assert_eq!(released.part, Assembly::new(MAGAZINE));
    let expected_linear = Vec3::from(at_now.translation - at_before.translation) / TICK;
    assert!(expected_linear.length() > 0.05, "hand_r is moving");
    vec_near(released.linear_velocity, expected_linear, 1e-3);
    let rifle = character
        .part_mut(&lookup, "hand_r")
        .expect("hand_r exists")
        .expect("the rifle stays on hand_r");
    assert_eq!(*rifle, Assembly::new(RELOAD_RIFLE));
    let keys: Vec<usize> = character
        .posed_placements(&lookup, root, &now_pose)
        .iter()
        .map(|p| p.0)
        .collect();
    assert_eq!(keys, vec![RELOAD_CHARACTER, RELOAD_CHARACTER, RELOAD_RIFLE]);
}

#[test]
fn release_under_a_scaled_root_reports_the_unscaled_turn_and_an_unscaled_transform() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let before_pose = reload_pose(&models, 114);
    let now_pose = reload_pose(&models, 115);
    // Scaling along Y commutes with the hand's turn about Y, so the turn stays readable.
    let root_with = |scale: Vec3| {
        Affine3A::from_scale_rotation_translation(
            scale,
            Quat::from_rotation_y(0.3),
            Vec3::new(5.0, -2.0, 1.0),
        )
    };
    let released_under = |root: Affine3A| {
        let mut character = Assembly::new(RELOAD_CHARACTER);
        character
            .attach(&lookup, "hand_l", Assembly::new(MAGAZINE), grip())
            .expect("hand_l exists");
        character
            .release(
                &lookup,
                &["hand_l"],
                Posed {
                    root,
                    pose: &now_pose,
                },
                Posed {
                    root,
                    pose: &before_pose,
                },
                TICK,
            )
            .expect("hand_l holds the magazine")
    };

    let plain = released_under(root_with(Vec3::ONE));
    let scaled = released_under(root_with(Vec3::new(1.0, 2.0, 1.0)));

    assert!(plain.angular_velocity.length() > 0.3, "the hand is turning");
    vec_near(scaled.angular_velocity, plain.angular_velocity, 1e-4);
    let columns = scaled.transform.matrix3;
    for column in [columns.x_axis, columns.y_axis, columns.z_axis] {
        assert!(
            (column.length() - 1.0).abs() < 1e-5,
            "{column} is not a unit axis"
        );
    }
}

#[test]
fn release_under_an_unevenly_scaled_turned_root_is_rigid() {
    let models = reload_models();
    let lookup = |k: usize| &models[k];
    let before_pose = reload_pose(&models, 114);
    let now_pose = reload_pose(&models, 115);
    let root = Affine3A::from_scale_rotation_translation(
        Vec3::new(2.0, 1.0, 1.0),
        Quat::from_rotation_y(0.3),
        Vec3::new(5.0, -2.0, 1.0),
    );
    let mut character = Assembly::new(RELOAD_CHARACTER);
    character
        .attach(&lookup, "hand_l", Assembly::new(MAGAZINE), grip())
        .expect("hand_l exists");

    let released = character
        .release(
            &lookup,
            &["hand_l"],
            Posed {
                root,
                pose: &now_pose,
            },
            Posed {
                root,
                pose: &before_pose,
            },
            TICK,
        )
        .expect("hand_l holds the magazine");

    let columns = released.transform.matrix3;
    for column in [columns.x_axis, columns.y_axis, columns.z_axis] {
        assert!(
            (column.length() - 1.0).abs() < 1e-5,
            "{column} is not a unit axis"
        );
    }
    let axes = [columns.x_axis, columns.y_axis, columns.z_axis];
    for (a, b) in [(0, 1), (0, 2), (1, 2)] {
        assert!(
            axes[a].dot(axes[b]).abs() < 1e-5,
            "axes {a} and {b} are not orthogonal: {columns:?}"
        );
    }
    let rotation = Quat::from_mat3a(&columns);
    assert!(rotation.is_normalized(), "{rotation} is not normalised");
    let hand = node_index(&models, RELOAD_CHARACTER, "hand_l");
    let expected = root * now_pose.nodes()[hand] * grip();
    vec_near(
        Vec3::from(released.transform.translation),
        Vec3::from(expected.translation),
        1e-5,
    );
    // A sheared chain has no single rotation, so the bound covers that ambiguity while still
    // ruling out the identity, which is about 1.4 rad from the unscaled chain's turn.
    let unscaled_root =
        Affine3A::from_rotation_translation(Quat::from_rotation_y(0.3), root.translation.into());
    let unscaled = unscaled_root * now_pose.nodes()[hand] * grip();
    let expected_rotation = Quat::from_mat3a(&unscaled.matrix3).normalize();
    assert!(
        rotation.angle_between(expected_rotation) <= 0.5,
        "{rotation:?} is not the hand's turn {expected_rotation:?}"
    );
    let spawn = LooseSpawn {
        position: Vec3::from(released.transform.translation),
        rotation,
        ..LooseSpawn::new(
            LooseShape::Box {
                half_extents: Vec3::splat(0.05),
            },
            0.2,
        )
    };
    let mut world = PhysicsWorld::new();
    world.spawn_loose(&spawn);
}
