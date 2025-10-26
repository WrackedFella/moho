#[cfg(feature = "ui-egui")]
use std::error::Error;
#[cfg(feature = "ui-egui")]
use std::fs::{File, remove_file, rename};
#[cfg(feature = "ui-egui")]
use std::io::{Read, Write};
#[cfg(feature = "ui-egui")]
use std::path::Path;

#[cfg(feature = "ui-egui")]
const SAVE_MAGIC: &[u8; 4] = b"MOHO";
#[cfg(feature = "ui-egui")]
const SAVE_VERSION: u32 = 1;

/// Atomically write a save file that envelopes the scene bytes together with
/// the UI-provided `WorldSpec` metadata. Format:
/// [4 bytes magic][u32 version][u64 meta_len][meta bytes][u64 scene_len][scene bytes]
#[cfg(feature = "ui-egui")]
pub fn write_scene_with_metadata<P: AsRef<Path>>(
    path: P,
    scene_bytes: &[u8],
    world_spec: &moho_ui::WorldSpec,
) -> Result<(), Box<dyn Error>> {
    let path = path.as_ref();
    let tmp_path = path.with_extension("tmp");

    // Serialize metadata with bincode (use same config as engine_renderer)
    let meta_bytes = bincode::encode_to_vec(world_spec, bincode::config::standard())?;

    // Write to temporary file in the same directory
    let mut f = File::create(&tmp_path)?;

    // Header
    f.write_all(SAVE_MAGIC)?;
    f.write_all(&SAVE_VERSION.to_le_bytes())?;

    // Meta length and meta
    let meta_len = meta_bytes.len() as u64;
    f.write_all(&meta_len.to_le_bytes())?;
    f.write_all(&meta_bytes)?;

    // Scene length and scene bytes
    let scene_len = scene_bytes.len() as u64;
    f.write_all(&scene_len.to_le_bytes())?;
    f.write_all(scene_bytes)?;

    // Ensure data is flushed to disk
    f.flush()?;
    f.sync_all()?;

    // Replace the original file atomically. On Windows rename will fail if
    // target exists, so remove original first if present.
    if path.exists() {
        remove_file(path)?;
    }
    rename(&tmp_path, path)?;
    Ok(())
}

/// Read our envelope format. If the file contains the new envelope, return
/// Some(WorldSpec) and the raw scene bytes. If the file appears to be the
/// legacy flat scene file, return None and the raw file bytes so the caller
/// can decode using the existing Scene::load_from_bytes helper.
#[cfg(feature = "ui-egui")]
pub fn read_scene_and_metadata<P: AsRef<Path>>(
    path: P,
) -> Result<(moho_ui::WorldSpec, Vec<u8>), Box<dyn Error>> {
    let mut f = File::open(path.as_ref())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;

    // Expect the envelope magic
    if buf.len() < 4 || &buf[0..4] != SAVE_MAGIC {
        return Err("invalid save file: missing MOHO magic".into());
    }

    // Parse envelope: 4 magic, 4 version, 8 meta_len, meta, 8 scene_len, scene
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

    // Decode metadata
    let (spec, _): (moho_ui::WorldSpec, usize) =
        bincode::decode_from_slice(meta_bytes, bincode::config::standard())?;
    Ok((spec, scene_bytes))
}

#[cfg(test)]
#[cfg(feature = "ui-egui")]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn write_and_read_envelope_roundtrip() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time ok")
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("moho_test_save_{}.bin", now));

        let scene_bytes = vec![9u8, 8, 7, 6];
        let spec = moho_ui::WorldSpec {
            name: "test-mod".to_string(),
            seed: Some(1234),
            size_xz: 64,
        };

        write_scene_with_metadata(&path, &scene_bytes, &spec).expect("write ok");
        let (read_spec, read_bytes) = read_scene_and_metadata(&path).expect("read ok");

        assert_eq!(spec.name, read_spec.name);
        assert_eq!(spec.seed, read_spec.seed);
        assert_eq!(spec.size_xz, read_spec.size_xz);
        assert_eq!(scene_bytes, read_bytes);

        let _ = std::fs::remove_file(&path);
    }
}
