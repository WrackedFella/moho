use glam::IVec3;
use moho_core::persist::{self, FileKind, PersistError};
use moho_game::scene_builders::WorldSpec;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, remove_file, rename};
use std::io::{self, Write};
use std::path::Path;

/// Compact block record for persisting VoxelGrid state alongside the scene.
///
/// Saves only what is needed to reconstruct block logic (positions, material,
/// resource); mesh data is derived at runtime and not persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRecord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub material_id: u32,
    pub resource_id: Option<u32>,
}

impl BlockRecord {
    pub fn from_block_data(block: moho_core::voxel::BlockData) -> Self {
        Self {
            x: block.position.x,
            y: block.position.y,
            z: block.position.z,
            material_id: block.material_id,
            resource_id: block.resource_id,
        }
    }
}

/// Payload of a world save. `scene` holds an enveloped scene file.
#[derive(Serialize, Deserialize)]
struct WorldFile {
    spec: WorldSpec,
    scene: Vec<u8>,
    blocks: Vec<BlockRecord>,
}

/// Atomically write a world save that bundles the scene bytes together with
/// the UI-provided `WorldSpec` metadata and voxel block records.
pub fn write_scene_with_metadata<P: AsRef<Path>>(
    path: P,
    scene_bytes: &[u8],
    world_spec: &WorldSpec,
    block_records: &[BlockRecord],
) -> Result<(), PersistError> {
    let path = path.as_ref();
    let tmp_path = path.with_extension("tmp");

    let bytes = persist::encode(
        FileKind::World,
        &WorldFile {
            spec: world_spec.clone(),
            scene: scene_bytes.to_vec(),
            blocks: block_records.to_vec(),
        },
    )?;

    let mut f = File::create(&tmp_path)?;
    f.write_all(&bytes)?;
    f.flush()?;
    f.sync_all()?;

    if path.exists() {
        remove_file(path)?;
    }
    rename(&tmp_path, path)?;
    Ok(())
}

/// Payload returned by [`read_scene_and_metadata`].
pub type SavePayload = (WorldSpec, Vec<u8>, Vec<BlockRecord>);

/// Read a world save. Returns the `WorldSpec`, the enveloped scene bytes, and
/// the block records.
pub fn read_scene_and_metadata<P: AsRef<Path>>(path: P) -> Result<SavePayload, PersistError> {
    let bytes = fs::read(path.as_ref())?;
    let file: WorldFile = persist::decode(FileKind::World, &bytes)?;
    Ok((file.spec, file.scene, file.blocks))
}

/// Persist a serialized chunk to `saves/<world_name>/chunks/<cx>_<cy>_<cz>.bin`.
///
/// `data` is an already-enveloped chunk file. Called by the streaming system
/// when a modified chunk is evicted. Uses an atomic rename so a crash
/// mid-write never leaves a corrupt file.
pub fn write_chunk_file(world_name: &str, pos: IVec3, data: &[u8]) -> io::Result<()> {
    let dir = chunk_dir(world_name);
    fs::create_dir_all(&dir)?;
    let path = chunk_path(world_name, pos);
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(data)?;
    f.flush()?;
    f.sync_all()?;
    if path.exists() {
        remove_file(&path)?;
    }
    rename(&tmp, &path)?;
    Ok(())
}

/// Load a serialized chunk from `saves/<world_name>/chunks/<cx>_<cy>_<cz>.bin`.
///
/// Returns `Ok(None)` if the file does not exist (chunk was never modified, so
/// the caller should fall back to generating it from the terrain function).
pub fn read_chunk_file(world_name: &str, pos: IVec3) -> Result<Option<Vec<u8>>, PersistError> {
    match fs::read(chunk_path(world_name, pos)) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn chunk_dir(world_name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("saves")
        .join(world_name)
        .join("chunks")
}

/// Remove all saved chunk files for `world_name`. Called when generating a new
/// world so stale chunk data from a previous session cannot override fresh terrain.
pub fn clear_chunk_files(world_name: &str) -> io::Result<()> {
    let dir = chunk_dir(world_name);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
        tracing::info!(world = world_name, "Cleared chunk save files for world");
    }
    Ok(())
}

fn chunk_path(world_name: &str, pos: IVec3) -> std::path::PathBuf {
    chunk_dir(world_name).join(format!("{}_{}_{}.bin", pos.x, pos.y, pos.z))
}
#[cfg(test)]
mod tests {
    use super::*;
    use moho_core::persist::FileKind;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(suffix: &str) -> std::path::PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time ok")
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("moho_test_save_{now}_{suffix}.bin"));
        path
    }

    fn spec(name: &str) -> moho_game::scene_builders::WorldSpec {
        moho_game::scene_builders::WorldSpec {
            name: name.to_string(),
            seed: Some(1234),
            size_xz: 64,
            day_length_seconds: 600.0,
            night_length_seconds: 420.0,
            initial_time_of_day: 6.0,
        }
    }

    fn read_error(bytes: &[u8], suffix: &str) -> PersistError {
        let path = temp_path(suffix);
        std::fs::write(&path, bytes).expect("write fixture");
        let result = read_scene_and_metadata(&path);
        let _ = std::fs::remove_file(&path);
        result.expect_err("must reject")
    }

    /// Header of a pre-migration save: `MOHO` + `u32` version + meta, scene
    /// and block lengths of zero.
    fn pre_migration_save(version: u32) -> Vec<u8> {
        let mut bytes = b"MOHO".to_vec();
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes
    }

    #[test]
    fn world_save_round_trips() {
        let path = temp_path("round_trip");
        let empty_path = temp_path("round_trip_empty");
        let scene_bytes = vec![9u8, 8, 7, 6];
        let world_spec = spec("test-mod");
        let blocks = vec![
            BlockRecord {
                x: 0,
                y: 0,
                z: 0,
                material_id: 2,
                resource_id: None,
            },
            BlockRecord {
                x: -1,
                y: 5,
                z: 0,
                material_id: 1,
                resource_id: Some(1),
            },
        ];

        write_scene_with_metadata(&path, &scene_bytes, &world_spec, &blocks).expect("write ok");
        write_scene_with_metadata(&empty_path, &[], &spec("empty-world"), &[]).expect("write ok");
        let on_disk = std::fs::read(&path).expect("read back");
        let (read_spec, read_bytes, read_blocks) = read_scene_and_metadata(&path).expect("read ok");
        let (empty_spec, empty_bytes, empty_blocks) =
            read_scene_and_metadata(&empty_path).expect("read ok");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&empty_path);

        assert_eq!(
            &on_disk[..7],
            b"MOHO\x01\x01\x00",
            "world envelope: magic, kind World, version 1"
        );
        assert_eq!(read_spec, world_spec);
        assert_eq!(read_bytes, scene_bytes);
        let fields: Vec<_> = read_blocks
            .iter()
            .map(|b| (b.x, b.y, b.z, b.material_id, b.resource_id))
            .collect();
        assert_eq!(fields, vec![(0, 0, 0, 2, None), (-1, 5, 0, 1, Some(1))]);
        assert_eq!(empty_spec.name, "empty-world");
        assert!(empty_bytes.is_empty());
        assert!(empty_blocks.is_empty());
    }

    #[test]
    fn foreign_file_returns_not_a_save_file() {
        for (suffix, bytes) in [
            (
                "foreign",
                b"\x89PNG\r\n\x1a\n and then some more bytes".to_vec(),
            ),
            ("tiny", b"MO".to_vec()),
            ("empty_file", Vec::new()),
        ] {
            let err = read_error(&bytes, suffix);

            assert!(
                matches!(err, PersistError::NotASaveFile),
                "{suffix}: {err:?}"
            );
        }
    }

    #[test]
    fn unknown_world_version_returns_unsupported_format() {
        let path = temp_path("future");
        write_scene_with_metadata(&path, &[], &spec("w"), &[]).expect("write ok");
        let mut bytes = std::fs::read(&path).expect("read back");
        let _ = std::fs::remove_file(&path);
        bytes[5..7].copy_from_slice(&(FileKind::World.current_version() + 1).to_le_bytes());

        let err = read_error(&bytes, "future_bumped");

        assert!(
            matches!(
                err,
                PersistError::UnsupportedFormat {
                    kind: FileKind::World,
                    version: 2
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn corrupted_world_save_returns_corrupt() {
        let path = temp_path("damaged");
        write_scene_with_metadata(&path, &[1, 2, 3], &spec("w"), &[]).expect("write ok");
        let mut bytes = std::fs::read(&path).expect("read back");
        let _ = std::fs::remove_file(&path);
        assert_eq!(&bytes[..7], b"MOHO\x01\x01\x00", "world envelope header");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;

        let err = read_error(&bytes, "damaged_flipped");

        assert!(matches!(err, PersistError::Corrupt), "{err:?}");
    }

    #[test]
    fn missing_world_save_returns_io_error() {
        let path = temp_path("missing");

        let err = read_scene_and_metadata(&path).expect_err("must fail");

        assert!(matches!(err, PersistError::Io(_)), "{err:?}");
    }

    #[test]
    fn pre_migration_world_save_returns_unsupported_format() {
        let v1 = read_error(&pre_migration_save(1), "old_v1");
        let v2 = read_error(&pre_migration_save(2), "old_v2");

        assert!(
            matches!(
                v1,
                PersistError::UnsupportedFormat {
                    kind: FileKind::World,
                    version: 0
                }
            ),
            "{v1:?}"
        );
        assert!(
            matches!(
                v2,
                PersistError::UnsupportedFormat {
                    kind: FileKind::Scene,
                    version: 0
                }
            ),
            "{v2:?}"
        );
    }
}
