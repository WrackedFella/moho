use std::fs;
use std::path::Path;

use crate::support::{Scene, cube_prim, floor_prim, write_glb, write_gltf};
use moho_level::{LevelErrorKind, LevelHash, LoadedLevel, level_hash, load, load_pinned};
use moho_render_api::WorldMeshId;

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

#[test]
fn copied_file_hashes_the_same() {
    let (dir, path) = write_gltf(&floor_scene());
    let elsewhere = tempfile::tempdir().expect("temp dir");
    fs::copy(&path, elsewhere.path().join("level.gltf")).expect("copy gltf");
    fs::copy(dir.path().join("level.bin"), elsewhere.path().join("level.bin")).expect("copy bin");

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
fn level_hash_equals_the_hash_open_records() {
    let (_dir, path) = write_gltf(&two_mesh_scene());

    let hashed = level_hash(&path).expect("hashes");
    let opened = LoadedLevel::open(&path, None, WorldMeshId(3)).expect("opens");

    assert_eq!(opened.hash(), Some(&hashed));
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
    assert!(
        LoadedLevel::open(&path, Some(&recorded), WorldMeshId(0)).is_err(),
        "open refuses the same way"
    );
}

#[test]
fn a_pinned_load_of_the_same_file_equals_a_plain_load() {
    let (_dir, path) = write_gltf(&two_mesh_scene());
    let pin = level_hash(&path).expect("hashes");

    let pinned = load_pinned(&path, &pin).expect("loads");
    let plain = load(&path).expect("loads");

    assert_eq!(pinned.meshes.len(), plain.meshes.len());
    assert_eq!(plain.meshes.len(), 2);
    for (a, b) in pinned.meshes.iter().zip(&plain.meshes) {
        assert_eq!(a.mesh.positions(), b.mesh.positions());
        assert_eq!(a.mesh.indices(), b.mesh.indices());
    }
}

#[test]
fn a_malformed_file_reports_not_gltf_not_mismatch() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("level.gltf");
    fs::write(&path, b"this is not a level").expect("write");
    let pin = LevelHash::from_bytes([7; 32]);

    let hashed = level_hash(&path).expect_err("malformed");
    let pinned = load_pinned(&path, &pin).expect_err("malformed");

    for err in [hashed, pinned] {
        assert_eq!(err.path(), path.as_path());
        assert!(
            matches!(
                err.kind(),
                LevelErrorKind::NotGltf(_) | LevelErrorKind::Read(_)
            ),
            "got {:?}",
            err.kind()
        );
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
