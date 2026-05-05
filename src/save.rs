use bincode::{Decode, Encode};
use glam::IVec3;
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
    world_spec: &moho_core::scene_builders::WorldSpec,
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
    moho_core::scene_builders::WorldSpec,
    Vec<u8>,
    Vec<BlockRecord>,
);

/// Read our envelope format. Returns the WorldSpec, raw scene bytes, and
/// block records. For v1 saves the block records vec will be empty; callers
/// should treat an empty vec as "block data not available" and log accordingly.
pub fn read_scene_and_metadata<P: AsRef<Path>>(path: P) -> Result<SavePayload, Box<dyn Error>> {
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
        return Err(format!("unsupported save version: {}", version).into());
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

    let (spec, _): (moho_core::scene_builders::WorldSpec, usize) =
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
    std::path::PathBuf::from("saves").join(world_name).join("chunks")
}

/// Remove all saved chunk files for `world_name`. Called when generating a new
/// world so stale chunk data from a previous session cannot override fresh terrain.
pub fn clear_chunk_files(world_name: &str) -> io::Result<()> {
    let dir = chunk_dir(world_name);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
        log::info!("Cleared chunk save files for world '{}'", world_name);
    }
    Ok(())
}

fn chunk_path(world_name: &str, pos: IVec3) -> std::path::PathBuf {
    chunk_dir(world_name).join(format!("{}_{}_{}.bin", pos.x, pos.y, pos.z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(suffix: &str) -> std::path::PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time ok")
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("moho_test_save_{}_{}.bin", now, suffix));
        path
    }

    #[test]
    fn write_and_read_envelope_roundtrip() {
        let path = temp_path("v2");

        let scene_bytes = vec![9u8, 8, 7, 6];
        let spec = moho_core::scene_builders::WorldSpec {
            name: "test-mod".to_string(),
            seed: Some(1234),
            size_xz: 64,
            day_length_seconds: 600.0,
            night_length_seconds: 420.0,
            initial_time_of_day: 6.0,
        };
        let blocks = vec![
            BlockRecord {
                x: 0,
                y: 0,
                z: 0,
                material_id: 2,
                resource_id: None,
            },
            BlockRecord {
                x: 1,
                y: 0,
                z: 0,
                material_id: 1,
                resource_id: Some(1),
            },
        ];

        write_scene_with_metadata(&path, &scene_bytes, &spec, &blocks).expect("write ok");
        let (read_spec, read_bytes, read_blocks) = read_scene_and_metadata(&path).expect("read ok");

        assert_eq!(spec.name, read_spec.name);
        assert_eq!(spec.seed, read_spec.seed);
        assert_eq!(spec.size_xz, read_spec.size_xz);
        assert_eq!(scene_bytes, read_bytes);
        assert_eq!(read_blocks.len(), 2);
        assert_eq!(read_blocks[0].x, 0);
        assert_eq!(read_blocks[0].material_id, 2);
        assert_eq!(read_blocks[1].resource_id, Some(1));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_blocks_roundtrip() {
        let path = temp_path("empty");

        let spec = moho_core::scene_builders::WorldSpec {
            name: "empty-world".to_string(),
            seed: None,
            size_xz: 32,
            day_length_seconds: 600.0,
            night_length_seconds: 420.0,
            initial_time_of_day: 6.0,
        };

        write_scene_with_metadata(&path, &[], &spec, &[]).expect("write ok");
        let (read_spec, read_bytes, read_blocks) = read_scene_and_metadata(&path).expect("read ok");

        assert_eq!(read_spec.name, "empty-world");
        assert!(read_bytes.is_empty());
        assert!(read_blocks.is_empty());

        let _ = std::fs::remove_file(&path);
    }
}
