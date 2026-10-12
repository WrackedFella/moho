use std::f32::consts::FRAC_PI_2;

use glam::{Quat, Vec3};
use serde_json::{Value, json};

use crate::support::{Node, Scene, Xform, cube_prim, write_gltf};
use moho_level::{Level, load};

const EPS: f32 = 1e-5;

fn empty(name: &str, xform: Xform) -> Node {
    Node {
        xform,
        name: Some(name.into()),
        ..Node::default()
    }
}

fn load_nodes(nodes: Vec<Node>, roots: Vec<usize>) -> Level {
    let mut scene = Scene::new();
    scene.nodes = nodes;
    scene.roots = roots;
    let (_dir, path) = write_gltf(&scene);

    load(&path).expect("level loads")
}

fn turn_y() -> [f32; 4] {
    let q = Quat::from_rotation_y(FRAC_PI_2);
    q.to_array()
}

fn assert_at(actual: Vec3, expected: Vec3) {
    assert!(
        actual.distance(expected) < EPS,
        "{actual:?} != {expected:?}"
    );
}

/// Compares components rather than `angle_between`: its `acos` near 1 turns a one-ulp
/// difference into ~3e-4 rad. `q` and `-q` are the same rotation.
fn assert_facing(actual: Quat, expected: Quat) {
    assert!(
        actual.abs_diff_eq(expected, EPS) || actual.abs_diff_eq(-expected, EPS),
        "{actual:?} != {expected:?}"
    );
}

fn names(level: &Level) -> Vec<&str> {
    level.markers.iter().map(|m| m.name.as_str()).collect()
}

fn marker_with_extras(extras: Value) -> Level {
    let node = Node {
        extras: Some(extras),
        ..empty("spawn_enemy", Xform::default())
    };

    load_nodes(vec![node], vec![0])
}

#[test]
fn spawn_player_loads_at_its_position_facing_90_degrees() {
    let node = empty(
        "spawn_player",
        Xform::Trs {
            t: [3.0, 0.0, -2.0],
            r: turn_y(),
            s: [1.0; 3],
        },
    );

    let level = load_nodes(vec![node], vec![0]);

    assert_eq!(names(&level), ["spawn_player"]);
    assert_at(level.markers[0].position, Vec3::new(3.0, 0.0, -2.0));
    assert_facing(level.markers[0].rotation, Quat::from_rotation_y(FRAC_PI_2));
}

#[test]
fn child_marker_adds_parent_translation() {
    let parent = Node {
        xform: Xform::translate([10.0, 0.0, 0.0]),
        children: vec![1],
        ..Node::default()
    };
    let child = empty("spawn_enemy", Xform::translate([1.0, 0.0, 0.0]));

    let level = load_nodes(vec![parent, child], vec![0]);

    assert_eq!(names(&level), ["spawn_enemy"]);
    assert_at(level.markers[0].position, Vec3::new(11.0, 0.0, 0.0));
}

#[test]
fn rotated_parent_rotates_child_position_and_facing() {
    let parent = Node {
        xform: Xform::rotate(turn_y()),
        children: vec![1],
        ..Node::default()
    };
    let child = empty("spawn_enemy", Xform::translate([1.0, 0.0, 0.0]));

    let level = load_nodes(vec![parent, child], vec![0]);

    assert_eq!(names(&level), ["spawn_enemy"]);
    assert_at(level.markers[0].position, Vec3::new(0.0, 0.0, -1.0));
    assert_facing(level.markers[0].rotation, Quat::from_rotation_y(FRAC_PI_2));
}

#[test]
fn scaled_parent_scales_position_not_rotation() {
    let parent = Node {
        xform: Xform::scale([2.0, 2.0, 2.0]),
        children: vec![1],
        ..Node::default()
    };
    let child = empty("spawn_enemy", Xform::translate([1.0, 0.0, 0.0]));

    let level = load_nodes(vec![parent, child], vec![0]);

    assert_eq!(names(&level), ["spawn_enemy"]);
    assert_at(level.markers[0].position, Vec3::new(2.0, 0.0, 0.0));
    assert_facing(level.markers[0].rotation, Quat::IDENTITY);
}

#[test]
fn string_property_comes_with_marker() {
    let level = marker_with_extras(json!({ "wave": "2" }));

    assert_eq!(level.markers.len(), 1);
    assert_eq!(level.markers[0].properties.len(), 1);
    assert_eq!(level.markers[0].properties["wave"], "2");
}

#[test]
fn number_property_reads_as_its_text() {
    let level = marker_with_extras(json!({ "wave": 2, "boss": true }));

    assert_eq!(level.markers[0].properties["wave"], "2");
    assert_eq!(level.markers[0].properties["boss"], "true");
}

#[test]
fn nested_property_reads_as_compact_json() {
    let level = marker_with_extras(json!({ "path": { "a": [1, 2] }, "list": [1, 2] }));

    assert_eq!(level.markers[0].properties["path"], r#"{"a":[1,2]}"#);
    assert_eq!(level.markers[0].properties["list"], "[1,2]");
}

#[test]
fn non_object_extras_give_no_properties() {
    for extras in [json!("wave"), json!([1, 2]), json!(7)] {
        let level = marker_with_extras(extras);

        assert_eq!(names(&level), ["spawn_enemy"]);
        assert!(level.markers[0].properties.is_empty());
    }
}

#[test]
fn node_without_extras_has_no_properties() {
    let level = load_nodes(vec![empty("spawn_enemy", Xform::default())], vec![0]);

    assert_eq!(names(&level), ["spawn_enemy"]);
    assert!(level.markers[0].properties.is_empty());
}

#[test]
fn two_meshes_and_one_empty_node_give_one_marker() {
    let mut scene = Scene::with_meshes(vec![vec![cube_prim()], vec![cube_prim()]]);
    scene.roots.push(scene.nodes.len());
    scene.nodes.push(empty("spawn_player", Xform::default()));
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes.len(), 2);
    assert_eq!(names(&level), ["spawn_player"]);
}

#[test]
fn mesh_node_with_extras_is_not_a_marker() {
    let mut scene = Scene::with_meshes(vec![vec![cube_prim()]]);
    scene.nodes[0].name = Some("crate".into());
    scene.nodes[0].extras = Some(json!({ "wave": "2" }));
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes.len(), 1);
    assert!(level.markers.is_empty());
}

#[test]
fn unnamed_empty_node_is_not_a_marker() {
    let node = Node {
        xform: Xform::translate([1.0, 0.0, 0.0]),
        ..Node::default()
    };

    let level = load_nodes(vec![node], vec![0]);

    assert!(level.markers.is_empty());
}

#[test]
fn markers_sharing_a_name_both_load() {
    let nodes = vec![
        empty("spawn_enemy", Xform::translate([1.0, 0.0, 0.0])),
        empty("spawn_enemy", Xform::translate([5.0, 0.0, 0.0])),
    ];

    let level = load_nodes(nodes, vec![0, 1]);

    assert_eq!(names(&level), ["spawn_enemy", "spawn_enemy"]);
    assert_at(level.markers[0].position, Vec3::new(1.0, 0.0, 0.0));
    assert_at(level.markers[1].position, Vec3::new(5.0, 0.0, 0.0));
}

#[test]
fn markers_come_in_scene_walk_order() {
    // File order is D, C, B, A; the walk visits A, B, C, D.
    let a = Node {
        children: vec![2, 1],
        ..empty("A", Xform::default())
    };
    let nodes = vec![
        empty("D", Xform::default()),
        empty("C", Xform::default()),
        empty("B", Xform::default()),
        a,
    ];

    let level = load_nodes(nodes, vec![3, 0]);

    assert_eq!(names(&level), ["A", "B", "C", "D"]);
}

#[test]
fn node_outside_the_loaded_scene_is_not_a_marker() {
    let nodes = vec![
        empty("inside", Xform::default()),
        empty("outside", Xform::default()),
    ];

    let level = load_nodes(nodes, vec![0]);

    assert_eq!(names(&level), ["inside"]);
}
