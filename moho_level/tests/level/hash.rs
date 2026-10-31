use std::fs;
use std::path::Path;

use crate::support::{
    Node, Scene, Xform, cube_prim, floor_prim, write_glb, write_gltf, write_gltf_edited,
};
use moho_level::{
    LevelError, LevelErrorKind, LevelHash, LoadedLevel, level_hash, load, load_pinned,
};
use moho_render_api::{WorldMeshId, WorldMeshRange};
use serde_json::json;
use sha2::{Digest, Sha256};

fn floor_scene() -> Scene {
    Scene::with_meshes(vec![vec![floor_prim(0.0)]])
}

fn two_mesh_scene() -> Scene {
    Scene::with_meshes(vec![vec![floor_prim(0.0)], vec![cube_prim()]])
}

fn flip_last_byte(path: &Path) {
    let mut bytes = fs::read(path).expect("read");
    let last = bytes.last_mut().expect("non-empty");
    *last ^= 0xFF;
    fs::write(path, bytes).expect("write");
}

/// SHA-256 over each source preceded by its length as 8 bytes little-endian, computed here
/// without the crate under test.
fn expected_digest(sources: &[&[u8]]) -> [u8; 32] {
    let mut framed = Vec::new();
    for source in sources {
        framed.extend_from_slice(&(source.len() as u64).to_le_bytes());
        framed.extend_from_slice(source);
    }
    Sha256::digest(&framed).into()
}

#[test]
fn copied_file_hashes_the_same() {
    let (dir, path) = write_gltf(&floor_scene());
    let elsewhere = tempfile::tempdir().expect("temp dir");
    fs::copy(&path, elsewhere.path().join("level.gltf")).expect("copy gltf");
    fs::copy(
        dir.path().join("level.bin"),
        elsewhere.path().join("level.bin"),
    )
    .expect("copy bin");

    let original = level_hash(&path).expect("hashes");
    let copy = level_hash(&elsewhere.path().join("level.gltf")).expect("hashes");

    assert_eq!(original, copy);
    let (_other_dir, other) = write_gltf(&two_mesh_scene());
    assert_ne!(original, level_hash(&other).expect("hashes"));
}

#[test]
fn a_changed_external_buffer_changes_the_hash() {
    let (dir, path) = write_gltf(&floor_scene());
    let before = level_hash(&path).expect("hashes");

    flip_last_byte(&dir.path().join("level.bin"));

    let after = level_hash(&path).expect("hashes");
    assert_ne!(before, after);
}

#[test]
fn a_changed_glb_blob_changes_the_hash() {
    let (_dir, path) = write_glb(&floor_scene());
    let before = level_hash(&path).expect("hashes");

    flip_last_byte(&path);

    assert_ne!(before, level_hash(&path).expect("hashes"));
}

#[test]
fn hash_covers_main_file_then_buffers_in_index_order() {
    let scene = floor_scene();
    let (dir, path) = write_gltf_edited(&scene, |doc| {
        doc["buffers"]
            .as_array_mut()
            .expect("buffers")
            .push(json!({ "byteLength": 4, "uri": "aaa_second.bin" }));
    });
    fs::write(dir.path().join("aaa_second.bin"), [9u8, 8, 7, 6]).expect("write second buffer");
    let main = fs::read(&path).expect("read gltf");
    let first = fs::read(dir.path().join("level.bin")).expect("read bin");

    let hash = level_hash(&path).expect("hashes");

    let expected = expected_digest(&[&main, &first, &[9, 8, 7, 6]]);
    assert_eq!(hash.as_bytes(), &expected);
    assert_ne!(
        hash.as_bytes(),
        &expected_digest(&[&main, &[9, 8, 7, 6], &first])
    );
}

#[test]
fn a_glb_blob_is_hashed_once_as_part_of_the_main_file() {
    let (_dir, path) = write_glb(&floor_scene());
    let glb = fs::read(&path).expect("read glb");

    let hash = level_hash(&path).expect("hashes");

    assert_eq!(hash.as_bytes(), &expected_digest(&[&glb]));
}

#[test]
fn a_data_uri_buffer_is_refused_by_the_loader() {
    let mut scene = floor_scene();
    scene.bin_uri = "data:application/octet-stream;base64,AAAA".into();
    scene.bin_file = None;
    let (_dir, path) = write_gltf(&scene);

    let err = level_hash(&path).expect_err("data uri refused");

    assert!(
        matches!(err.kind(), LevelErrorKind::UnsupportedBufferUri { uri } if uri.starts_with("data:")),
        "{err}"
    );
}

#[test]
fn level_hash_equals_the_hash_open_records() {
    let (_dir, path) = write_gltf(&two_mesh_scene());

    let hashed = level_hash(&path).expect("hashes");
    let opened = LoadedLevel::open(&path, None, WorldMeshId(3)).expect("opens");

    assert_eq!(opened.hash(), Some(&hashed));
    assert_eq!(opened.level().meshes.len(), 2);
    assert_eq!(
        opened.ids(),
        WorldMeshRange::new(WorldMeshId(3), 2).expect("fits")
    );
}

#[test]
fn open_with_a_matching_pin_loads_and_records_that_hash() {
    let (_dir, path) = write_gltf(&two_mesh_scene());
    let pin = level_hash(&path).expect("hashes");

    let opened = LoadedLevel::open(&path, Some(&pin), WorldMeshId(0)).expect("opens");

    assert_eq!(opened.hash(), Some(&pin));
    assert_eq!(opened.path(), Some(path.as_path()));
    assert_eq!(opened.level().meshes.len(), 2);
}

#[test]
fn a_pinned_load_of_a_changed_file_refuses() {
    let (dir, path) = write_gltf(&floor_scene());
    let recorded = level_hash(&path).expect("hashes");
    flip_last_byte(&dir.path().join("level.bin"));
    let found = level_hash(&path).expect("hashes");

    let err = load_pinned(&path, &recorded).expect_err("refused");

    assert_eq!(err.path(), path.as_path());
    match err.kind() {
        LevelErrorKind::HashMismatch {
            expected,
            found: actual,
        } => {
            assert_eq!(*expected, recorded);
            assert_eq!(*actual, found);
        }
        kind => panic!("expected a hash mismatch, got {kind:?}"),
    }
    let shown = err.to_string();
    assert!(shown.contains(&recorded.to_string()), "{shown}");
    assert!(shown.contains(&found.to_string()), "{shown}");
}

#[test]
fn a_wrong_pin_refuses_before_any_mesh_is_built() {
    let mut prim = floor_prim(0.0);
    prim.normals.clear();
    let (_dir, path) = write_gltf(&Scene::with_meshes(vec![vec![prim]]));
    let wrong = LevelHash::from_bytes([1; 32]);

    let err = load_pinned(&path, &wrong).expect_err("refused");

    assert!(
        matches!(err.kind(), LevelErrorKind::HashMismatch { .. }),
        "the mismatch is reported, not the bad mesh: {err}"
    );
    assert!(matches!(
        load(&path).expect_err("mesh is bad").kind(),
        LevelErrorKind::BadMesh { .. }
    ));
}

#[test]
fn a_pinned_load_of_the_same_file_equals_a_plain_load() {
    let mut scene = two_mesh_scene();
    scene.materials = vec![Some("brick".into())];
    scene.meshes[1][0].material = Some(0);
    scene.roots.push(scene.nodes.len());
    scene.nodes.push(Node {
        xform: Xform::translate([3.0, 0.0, -2.0]),
        name: Some("spawn_player".into()),
        ..Node::default()
    });
    let (_dir, path) = write_gltf(&scene);
    let pin = level_hash(&path).expect("hashes");

    let pinned = load_pinned(&path, &pin).expect("loads");
    let plain = load(&path).expect("loads");

    assert_eq!(plain.meshes.len(), 2);
    assert_eq!(plain.meshes[1].material.as_deref(), Some("brick"));
    assert_eq!(plain.markers.len(), 1);
    assert_eq!(pinned.markers, plain.markers);
    assert_eq!(pinned.meshes.len(), plain.meshes.len());
    for (a, b) in pinned.meshes.iter().zip(&plain.meshes) {
        assert_eq!(a.material, b.material);
        assert_eq!(a.mesh.positions(), b.mesh.positions());
        assert_eq!(a.mesh.indices(), b.mesh.indices());
    }
}

#[test]
fn a_malformed_file_reports_not_gltf_not_mismatch() {
    let pin = LevelHash::from_bytes([7; 32]);
    let both = |path: &Path| -> [LevelError; 2] {
        [
            level_hash(path).expect_err("fails"),
            load_pinned(path, &pin).expect_err("fails"),
        ]
    };

    let garbage_dir = tempfile::tempdir().expect("temp dir");
    let garbage = garbage_dir.path().join("level.gltf");
    fs::write(&garbage, b"this is not a level").expect("write");
    let missing_dir = tempfile::tempdir().expect("temp dir");
    let missing = missing_dir.path().join("absent.gltf");
    let mut no_bin = floor_scene();
    no_bin.bin_file = None;
    let (_no_bin_dir, no_bin_path) = write_gltf(&no_bin);

    for err in both(&garbage) {
        assert!(matches!(err.kind(), LevelErrorKind::NotGltf(_)), "{err}");
        assert_eq!(err.path(), garbage.as_path());
    }
    for err in both(&missing) {
        assert!(matches!(err.kind(), LevelErrorKind::Read(_)), "{err}");
        assert_eq!(err.path(), missing.as_path());
    }
    for err in both(&no_bin_path) {
        assert!(
            matches!(err.kind(), LevelErrorKind::MissingBuffer { .. }),
            "{err}"
        );
        assert_eq!(err.path(), no_bin_path.as_path());
    }
}

#[test]
fn reopening_the_path_after_an_edit_loads_the_new_content() {
    let (_dir, path) = write_gltf(&floor_scene());
    let first = LoadedLevel::open(&path, None, WorldMeshId(0)).expect("opens");
    let old_hash = *first.hash().expect("recorded");
    assert_eq!(first.level().meshes.len(), 1);
    let (edited_dir, edited_path) = write_gltf(&two_mesh_scene());
    fs::copy(&edited_path, &path).expect("overwrite gltf");
    fs::copy(
        edited_dir.path().join("level.bin"),
        path.with_file_name("level.bin"),
    )
    .expect("overwrite bin");

    let reopened =
        LoadedLevel::open(first.path().expect("path"), None, WorldMeshId(0)).expect("opens");

    assert_eq!(reopened.level().meshes.len(), 2);
    assert_ne!(reopened.hash(), Some(&old_hash));
    assert_eq!(reopened.hash(), Some(&level_hash(&path).expect("hashes")));
}
