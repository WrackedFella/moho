use std::fs;
use std::path::Path;

use crate::support::{
    MODE_LINES, Node, Scene, Xform, cube_prim, floor_prim, line_prim, write_glb, write_gltf,
};
use moho_level::{Level, LevelErrorKind, load};

fn x_range(level: &Level, mesh: usize) -> (f32, f32) {
    let xs = level.meshes[mesh].mesh.positions().iter().map(|p| p[0]);
    (
        xs.clone().fold(f32::MAX, f32::min),
        xs.fold(f32::MIN, f32::max),
    )
}

fn assert_near(actual: [f32; 3], expected: [f32; 3]) {
    for i in 0..3 {
        assert!(
            (actual[i] - expected[i]).abs() < 1e-4,
            "{actual:?} != {expected:?}"
        );
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[test]
fn two_meshes_load_as_two_world_meshes() {
    let scene = Scene::with_meshes(vec![vec![floor_prim(0.0)], vec![cube_prim()]]);
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes.len(), 2);
    assert_eq!(level.meshes[0].mesh.positions().len(), 4);
    assert_eq!(level.meshes[1].mesh.positions().len(), 24);
    assert_eq!(level.meshes[1].mesh.indices().len(), 36);
}

#[test]
fn child_cube_under_moved_parent_lies_near_x_10() {
    let mut scene = Scene::new();
    scene.meshes = vec![vec![cube_prim()]];
    scene.nodes = vec![
        Node {
            xform: Xform::translate([10.0, 0.0, 0.0]),
            mesh: None,
            children: vec![1],
        },
        Node {
            mesh: Some(0),
            ..Node::default()
        },
    ];
    scene.roots = vec![0];
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes.len(), 1);
    let (min, max) = x_range(&level, 0);
    assert!(
        (min - 9.5).abs() < 1e-4 && (max - 10.5).abs() < 1e-4,
        "{min}..{max}"
    );
}

#[test]
fn floor_reports_material_concrete() {
    let mut floor = floor_prim(0.0);
    floor.material = Some(0);
    let mut scene = Scene::with_meshes(vec![vec![floor]]);
    scene.materials = vec![Some("concrete".into())];
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes[0].material.as_deref(), Some("concrete"));
}

#[test]
fn primitive_without_material_reports_none() {
    let scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes[0].material, None);
}

#[test]
fn glb_loads_the_same_meshes_as_gltf() {
    let scene = Scene::with_meshes(vec![vec![floor_prim(1.0)], vec![cube_prim()]]);
    let (_gltf_dir, gltf_path) = write_gltf(&scene);
    let (_glb_dir, glb_path) = write_glb(&scene);

    let from_gltf = load(&gltf_path).expect("gltf loads");
    let from_glb = load(&glb_path).expect("glb loads");

    assert_eq!(from_glb.meshes.len(), 2);
    for (a, b) in from_gltf.meshes.iter().zip(&from_glb.meshes) {
        assert_eq!(a.mesh.positions(), b.mesh.positions());
        assert_eq!(a.mesh.normals(), b.mesh.normals());
        assert_eq!(a.mesh.indices(), b.mesh.indices());
    }
    assert_near(from_glb.meshes[0].mesh.positions()[0], [-10.0, 1.0, -10.0]);
}

#[test]
fn strided_positions_load() {
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(2.0)]]);
    scene.interleave = true;
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    let mesh = &level.meshes[0].mesh;
    assert_near(mesh.positions()[1], [-10.0, 2.0, 10.0]);
    assert_near(mesh.positions()[2], [10.0, 2.0, 10.0]);
    assert_near(mesh.normals()[3], [0.0, 1.0, 0.0]);
    assert_eq!(mesh.indices(), &[0, 1, 2, 0, 2, 3]);
}

#[test]
fn mesh_without_indices_gets_sequential_indices() {
    let mut prim = floor_prim(0.0);
    prim.positions.truncate(3);
    prim.normals.truncate(3);
    prim.indices = None;
    let scene = Scene::with_meshes(vec![vec![prim]]);
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes[0].mesh.indices(), &[0, 1, 2]);
}

#[test]
fn mesh_used_by_two_nodes_loads_twice_at_each_transform() {
    let mut scene = Scene::new();
    scene.meshes = vec![vec![cube_prim()]];
    scene.nodes = vec![
        Node {
            xform: Xform::translate([5.0, 0.0, 0.0]),
            mesh: Some(0),
            children: vec![],
        },
        Node {
            xform: Xform::translate([-5.0, 0.0, 0.0]),
            mesh: Some(0),
            children: vec![],
        },
    ];
    scene.roots = vec![0, 1];
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes.len(), 2);
    let mut ranges = [x_range(&level, 0), x_range(&level, 1)];
    ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!((ranges[0].0 + 5.5).abs() < 1e-4 && (ranges[0].1 + 4.5).abs() < 1e-4);
    assert!((ranges[1].0 - 4.5).abs() < 1e-4 && (ranges[1].1 - 5.5).abs() < 1e-4);
}

#[test]
fn mirrored_node_keeps_triangles_facing_out() {
    let mut scene = Scene::with_meshes(vec![vec![cube_prim()]]);
    scene.nodes[0].xform = Xform::scale([-1.0, 1.0, 1.0]);
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    let mesh = &level.meshes[0].mesh;
    let positions = mesh.positions();
    let normals = mesh.normals();
    assert_eq!(mesh.indices().len(), 36);
    for tri in mesh.indices().chunks(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| i as usize);
        let geometric = cross(
            sub(positions[b], positions[a]),
            sub(positions[c], positions[a]),
        );
        assert!(
            dot(geometric, normals[a]) > 0.0,
            "triangle {tri:?} faces against its normal"
        );
        assert!(
            dot(geometric, positions[a]) > 0.0,
            "triangle {tri:?} faces into the cube"
        );
    }
    for n in normals {
        assert!(
            (dot(*n, *n) - 1.0).abs() < 1e-4,
            "normal {n:?} not unit length"
        );
    }
}

#[test]
fn rotated_node_rotates_normals() {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    scene.nodes[0].xform = Xform::rotate([half, 0.0, 0.0, half]);
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    let mesh = &level.meshes[0].mesh;
    for n in mesh.normals() {
        assert_near(*n, [0.0, 0.0, 1.0]);
    }
    assert_near(mesh.positions()[1], [-10.0, -10.0, 0.0]);
}

#[test]
fn non_uniform_scale_keeps_normals_perpendicular_and_unit() {
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    scene.nodes[0].xform = Xform::scale([1.0, 4.0, 1.0]);
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    for n in level.meshes[0].mesh.normals() {
        assert_near(*n, [0.0, 1.0, 0.0]);
    }
}

#[test]
fn line_primitive_is_skipped() {
    let scene = Scene::with_meshes(vec![vec![floor_prim(0.0), line_prim()]]);
    assert_eq!(scene.meshes[0][1].mode, MODE_LINES);
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes.len(), 1);
    assert_eq!(level.meshes[0].mesh.positions().len(), 4);
}

#[test]
fn percent_escaped_buffer_uri_resolves() {
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    scene.bin_uri = "my%20level.bin".into();
    scene.bin_file = Some("my level.bin".into());
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert_eq!(level.meshes.len(), 1);
}

#[test]
fn data_uri_buffer_is_refused() {
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    scene.bin_uri = "data:application/octet-stream;base64,AAAA".into();
    scene.bin_file = None;
    let (_dir, path) = write_gltf(&scene);

    let err = load(&path).expect_err("data uri refused");

    assert!(
        matches!(err.kind(), LevelErrorKind::UnsupportedBufferUri { uri } if uri.starts_with("data:")),
        "{err}"
    );
    assert_eq!(err.path(), path);
}

#[test]
fn absolute_buffer_uri_is_refused() {
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    scene.bin_uri = "file:///etc/passwd".into();
    scene.bin_file = None;
    let (_dir, path) = write_gltf(&scene);

    let err = load(&path).expect_err("absolute uri refused");

    assert!(
        matches!(err.kind(), LevelErrorKind::UnsupportedBufferUri { .. }),
        "{err}"
    );
}

#[test]
fn baked_channels_are_neutral() {
    let scene = Scene::with_meshes(vec![vec![cube_prim()]]);
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    let mesh = &level.meshes[0].mesh;
    let n = mesh.positions().len();
    assert_eq!(mesh.ao(), vec![1.0; n]);
    assert_eq!(mesh.light_rgb(), vec![[0.0; 3]; n]);
    assert_eq!(mesh.sky_exposure(), vec![1.0; n]);
    assert_eq!(mesh.surface(), vec![0; n]);
}

#[test]
fn scene_without_nodes_loads_as_empty_level() {
    let scene = Scene::new();
    let (_dir, path) = write_gltf(&scene);

    let level = load(&path).expect("level loads");

    assert!(level.meshes.is_empty());
}

#[test]
fn triangle_without_normals_is_a_bad_mesh() {
    let mut prim = floor_prim(0.0);
    prim.normals.clear();
    let scene = Scene::with_meshes(vec![vec![prim]]);
    let (_dir, path) = write_gltf(&scene);

    let err = load(&path).expect_err("normals required");

    assert!(
        matches!(err.kind(), LevelErrorKind::BadMesh { mesh: 0, .. }),
        "{err}"
    );
    assert!(err.to_string().starts_with(&path.display().to_string()));
}

#[test]
fn missing_file_error_names_it() {
    let path = Path::new("/definitely/not/here/level.gltf");

    let err = load(path).expect_err("no such file");

    assert!(matches!(err.kind(), LevelErrorKind::Read(_)), "{err}");
    assert_eq!(err.path(), path);
    assert!(err.to_string().starts_with(&path.display().to_string()));
}

#[test]
fn non_gltf_file_error_names_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("notes.gltf");
    fs::write(&path, "this is not a level").expect("write");

    let err = load(&path).expect_err("not gltf");

    assert!(matches!(err.kind(), LevelErrorKind::NotGltf(_)), "{err}");
    assert_eq!(err.path(), path);
    assert!(err.to_string().starts_with(&path.display().to_string()));
}

#[test]
fn gltf_with_missing_bin_error_names_it() {
    let mut scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    scene.bin_file = None;
    let (_dir, path) = write_gltf(&scene);

    let err = load(&path).expect_err("buffer missing");

    assert!(
        matches!(err.kind(), LevelErrorKind::MissingBuffer { uri, .. } if uri == "level.bin"),
        "{err}"
    );
    assert_eq!(err.path(), path);
    assert!(err.to_string().starts_with(&path.display().to_string()));
}
