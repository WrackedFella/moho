use bincode::{Decode, Encode};
use glam::IVec3;
use moho_core::persist::PersistError;
use std::error::Error;
use std::fs::{self, File, remove_file, rename};
use std::io::{self, Read, Write};
use std::path::Path;

const SAVE_MAGIC: &[u8; 4] = b"MOHO";
/// Version 1: magic + version + meta + scene (no block data)
/// Version 2: magic + version + meta + scene + blocks
const SAVE_VERSION: u32 = 2;

/// Compact block record for persisting VoxelGrid state alongside the scene.
///
/// Saves only what is needed to reconstruct block logic (positions, material,
/// resource); mesh data is derived at runtime and not persisted.
#[derive(Debug, Clone, Encode, Decode)]
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

/// Atomically write a save file that envelopes the scene bytes together with
/// the UI-provided `WorldSpec` metadata and voxel block records.
///
/// Format v2:
/// [4 bytes magic][u32 version][u64 meta_len][meta bytes]
///               [u64 scene_len][scene bytes][u64 blocks_len][blocks bytes]
pub fn write_scene_with_metadata<P: AsRef<Path>>(
    path: P,
    scene_bytes: &[u8],
    world_spec: &moho_game::scene_builders::WorldSpec,
    block_records: &[BlockRecord],
) -> Result<(), Box<dyn Error>> {
    let path = path.as_ref();
    let tmp_path = path.with_extension("tmp");

    let meta_bytes = bincode::encode_to_vec(world_spec, bincode::config::standard())?;
    let blocks_bytes = bincode::encode_to_vec(block_records, bincode::config::standard())?;

    let mut f = File::create(&tmp_path)?;

    f.write_all(SAVE_MAGIC)?;
    f.write_all(&SAVE_VERSION.to_le_bytes())?;

    let meta_len = meta_bytes.len() as u64;
    f.write_all(&meta_len.to_le_bytes())?;
    f.write_all(&meta_bytes)?;

    let scene_len = scene_bytes.len() as u64;
    f.write_all(&scene_len.to_le_bytes())?;
    f.write_all(scene_bytes)?;

    let blocks_len = blocks_bytes.len() as u64;
    f.write_all(&blocks_len.to_le_bytes())?;
    f.write_all(&blocks_bytes)?;

    f.flush()?;
    f.sync_all()?;

    if path.exists() {
        remove_file(path)?;
    }
    rename(&tmp_path, path)?;
    Ok(())
}

/// Payload returned by [`read_scene_and_metadata`].
pub type SavePayload = (
    moho_game::scene_builders::WorldSpec,
    Vec<u8>,
    Vec<BlockRecord>,
);

/// Read our envelope format. Returns the WorldSpec, raw scene bytes, and
/// block records. For v1 saves the block records vec will be empty; callers
/// should treat an empty vec as "block data not available" and log accordingly.
pub fn read_scene_and_metadata<P: AsRef<Path>>(path: P) -> Result<SavePayload, PersistError> {
    read_scene_and_metadata_inner(path.as_ref()).map_err(|e| match e.downcast::<io::Error>() {
        Ok(io_err) => PersistError::Io(*io_err),
        Err(_) => PersistError::Corrupt,
    })
}

fn read_scene_and_metadata_inner<P: AsRef<Path>>(path: P) -> Result<SavePayload, Box<dyn Error>> {
    let mut f = File::open(path.as_ref())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;

    if buf.len() < 4 || &buf[0..4] != SAVE_MAGIC {
        return Err("invalid save file: missing MOHO magic".into());
    }

    if buf.len() < 4 + 4 + 8 {
        return Err("invalid envelope: too small".into());
    }
    let mut offset = 4;
    let version = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap());
    offset += 4;
    if version < 1 {
        return Err(format!("unsupported save version: {version}").into());
    }

    let meta_len = u64::from_le_bytes(buf[offset..offset + 8].try_into().unwrap()) as usize;
    offset += 8;
    if buf.len() < offset + meta_len + 8 {
        return Err("invalid envelope: meta length out of range".into());
    }
    let meta_bytes = &buf[offset..offset + meta_len];
    offset += meta_len;

    let scene_len = u64::from_le_bytes(buf[offset..offset + 8].try_into().unwrap()) as usize;
    offset += 8;
    if buf.len() < offset + scene_len {
        return Err("invalid envelope: scene length out of range".into());
    }
    let scene_bytes = buf[offset..offset + scene_len].to_vec();
    offset += scene_len;

    let (spec, _): (moho_game::scene_builders::WorldSpec, usize) =
        bincode::decode_from_slice(meta_bytes, bincode::config::standard())?;

    // v2+ includes a block data section
    let block_records = if version >= 2 && buf.len() >= offset + 8 {
        let blocks_len = u64::from_le_bytes(buf[offset..offset + 8].try_into().unwrap()) as usize;
        offset += 8;
        if buf.len() < offset + blocks_len {
            return Err("invalid envelope: blocks length out of range".into());
        }
        let (records, _): (Vec<BlockRecord>, usize) = bincode::decode_from_slice(
            &buf[offset..offset + blocks_len],
            bincode::config::standard(),
        )?;
        records
    } else {
        Vec::new()
    };

    Ok((spec, scene_bytes, block_records))
}

/// Persist a serialized chunk to `saves/<world_name>/chunks/<cx>_<cy>_<cz>.bin`.
///
/// Called by the streaming system when a modified chunk is evicted.
/// Uses an atomic rename so a crash mid-write never leaves a corrupt file.
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
/// Returns `None` if the file does not exist (chunk was never modified, so
/// the caller should fall back to generating it from the terrain function).
pub fn read_chunk_file(world_name: &str, pos: IVec3) -> Option<Vec<u8>> {
    let path = chunk_path(world_name, pos);
    fs::read(path).ok()
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
