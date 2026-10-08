use crate::persist::PersistError;
use std::collections::HashMap;

pub const CHUNK_VOL: usize = 4096; // 16³

/// Serializable snapshot of a chunk's block data (no runtime-only flags).
#[derive(serde::Serialize, serde::Deserialize)]
struct ChunkSnapshot {
    palette: Vec<u32>,
    /// Flattened indices (CHUNK_VOL entries).
    indices: Vec<u16>,
    /// Sparse resource entries as (local_idx, resource_id).
    resources: Vec<(u16, u32)>,
}

/// Storage for one 16³ chunk using palette compression.
///
/// `indices[i] == 0` always means air, regardless of `palette[0]` (which is a
/// reserved sentinel). Real material IDs live at `palette[1..]`. An all-air
/// chunk is never inserted into the outer map, so an absent entry == all air.
#[derive(Debug, Clone)]
pub struct PalettedChunk {
    /// `palette[0]` = reserved sentinel (never read as a material id).
    /// `palette[n]` for n ≥ 1 = actual material id in use.
    palette: Vec<u32>,
    /// Dense per-voxel palette index. 0 = air.
    indices: [u16; CHUNK_VOL],
    /// Sparse local-index → resource id (most blocks have none).
    resources: HashMap<u16, u32>,
    pub mesh_dirty: bool,
    pub light_dirty: bool,
    /// Set when any block in this chunk was placed or removed since the last
    /// save. Used by the streaming system to decide whether to write to disk
    /// on eviction.
    modified: bool,
}

impl Default for PalettedChunk {
    fn default() -> Self {
        Self::new()
    }
}

impl PalettedChunk {
    pub fn new() -> Self {
        Self {
            palette: vec![u32::MAX], // index 0 = air sentinel
            indices: [0u16; CHUNK_VOL],
            resources: HashMap::new(),
            mesh_dirty: false,
            light_dirty: false,
            modified: false,
        }
    }

    #[inline]
    pub fn material_at(&self, idx: usize) -> Option<u32> {
        let pi = self.indices[idx] as usize;
        if pi == 0 {
            None
        } else {
            Some(self.palette[pi])
        }
    }

    #[inline]
    pub fn resource_at(&self, idx: usize) -> Option<u32> {
        self.resources.get(&(idx as u16)).copied()
    }

    pub fn set_block(&mut self, idx: usize, material_id: u32, resource_id: Option<u32>) {
        // Find existing palette slot for this material (skip sentinel at [0]).
        let pi = self.palette[1..]
            .iter()
            .position(|&m| m == material_id)
            .map_or_else(
                || {
                    let i = self.palette.len();
                    self.palette.push(material_id);
                    i
                },
                |p| p + 1,
            );
        self.indices[idx] = pi as u16;
        match resource_id {
            Some(rid) => {
                self.resources.insert(idx as u16, rid);
            }
            None => {
                self.resources.remove(&(idx as u16));
            }
        }
        self.modified = true;
    }

    /// Returns `true` if a block was present and removed.
    pub fn clear_block(&mut self, idx: usize) -> bool {
        if self.indices[idx] == 0 {
            return false;
        }
        self.indices[idx] = 0;
        self.resources.remove(&(idx as u16));
        self.modified = true;
        true
    }

    /// Whether any block in this chunk was placed or removed since the last
    /// `clear_modified()` call.
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Reset the modified flag (called after saving the chunk to disk).
    pub fn clear_modified(&mut self) {
        self.modified = false;
    }

    /// Serialize block data (palette + indices + resources) to bytes.
    /// Runtime-only flags (`mesh_dirty`, `light_dirty`, `modified`) are not
    /// included; they are re-derived when the chunk is loaded back.
    pub fn to_bytes(&self) -> Vec<u8> {
        // HashMap order is unspecified; sort so equal chunks encode to equal bytes.
        let mut resources: Vec<(u16, u32)> = self.resources.iter().map(|(&k, &v)| (k, v)).collect();
        resources.sort_unstable_by_key(|&(idx, _)| idx);
        let snap = ChunkSnapshot {
            palette: self.palette.clone(),
            indices: self.indices.to_vec(),
            resources,
        };
        crate::persist::encode(crate::persist::FileKind::Chunk, &snap)
            .expect("PalettedChunk serialization must not fail")
    }

    /// Deserialize a chunk from bytes produced by `to_bytes`.
    /// Fails if the data is malformed, the index count is wrong, or an index
    /// points outside the palette.
    pub fn from_bytes(data: &[u8]) -> Result<Self, PersistError> {
        let snap: ChunkSnapshot = crate::persist::decode(crate::persist::FileKind::Chunk, data)?;
        if snap.indices.len() != CHUNK_VOL
            || snap.palette.is_empty()
            || snap
                .indices
                .iter()
                .any(|&i| usize::from(i) >= snap.palette.len())
        {
            return Err(PersistError::Corrupt);
        }
        let mut indices = [0u16; CHUNK_VOL];
        indices.copy_from_slice(&snap.indices);
        let resources = snap.resources.into_iter().collect();
        Ok(Self {
            palette: snap.palette,
            indices,
            resources,
            mesh_dirty: true,  // needs re-mesh after load
            light_dirty: true, // needs re-light after load
            modified: false,
        })
    }

    pub fn block_count(&self) -> usize {
        self.indices.iter().filter(|&&i| i != 0).count()
    }

    /// Iterate non-air blocks as `(linear_idx, material_id, resource_id)`.
    pub fn iter_blocks(&self) -> impl Iterator<Item = (usize, u32, Option<u32>)> + '_ {
        self.indices.iter().enumerate().filter_map(|(idx, &pi)| {
            if pi == 0 {
                return None;
            }
            Some((
                idx,
                self.palette[pi as usize],
                self.resources.get(&(idx as u16)).copied(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edited_chunk() -> PalettedChunk {
        let mut c = PalettedChunk::new();
        c.set_block(0, 5, None);
        c.set_block(17, 9, Some(3));
        c.set_block(4095, 5, Some(8));
        c
    }

    #[test]
    fn chunk_round_trips_through_bytes() {
        let chunk = edited_chunk();

        let bytes = chunk.to_bytes();
        let back = PalettedChunk::from_bytes(&bytes).expect("decode");

        assert_eq!(&bytes[..5], b"MOHO\x03", "chunk envelope magic and kind");
        assert_eq!(
            back.iter_blocks().collect::<Vec<_>>(),
            chunk.iter_blocks().collect::<Vec<_>>()
        );
        assert_eq!(back.block_count(), 3);
        assert!(back.mesh_dirty && back.light_dirty);
        assert!(!back.is_modified());
    }

    #[test]
    fn bytes_ignore_resource_insertion_order() {
        let mut ascending = PalettedChunk::new();
        let mut descending = PalettedChunk::new();
        for i in 0..64usize {
            ascending.set_block(i, 1, Some(i as u32 + 100));
        }
        for i in (0..64usize).rev() {
            descending.set_block(i, 1, Some(i as u32 + 100));
        }

        assert_eq!(ascending.to_bytes(), descending.to_bytes());
    }

    #[test]
    fn wrong_index_count_returns_corrupt() {
        let short = (vec![u32::MAX], vec![0u16; 3], Vec::<(u16, u32)>::new());
        let bytes =
            crate::persist::encode(crate::persist::FileKind::Chunk, &short).expect("encode");

        let err = PalettedChunk::from_bytes(&bytes).expect_err("must reject");

        assert!(matches!(err, PersistError::Corrupt), "{err:?}");
    }

    #[test]
    fn index_outside_palette_returns_corrupt() {
        let mut indices = vec![0u16; CHUNK_VOL];
        indices[7] = 2;
        for (palette, indices) in [
            (vec![u32::MAX, 5], indices),
            (Vec::new(), vec![0u16; CHUNK_VOL]),
        ] {
            let snap = (palette, indices, Vec::<(u16, u32)>::new());
            let bytes =
                crate::persist::encode(crate::persist::FileKind::Chunk, &snap).expect("encode");

            let err = PalettedChunk::from_bytes(&bytes).expect_err("must reject");

            assert!(matches!(err, PersistError::Corrupt), "{err:?}");
        }
    }

    #[test]
    fn checksum_mismatch_in_chunk_returns_corrupt() {
        let mut bytes = edited_chunk().to_bytes();
        assert_eq!(&bytes[..5], b"MOHO\x03", "chunk envelope magic and kind");
        bytes[15] ^= 0xFF; // inside the stored CRC

        let err = PalettedChunk::from_bytes(&bytes).expect_err("must reject");

        assert!(matches!(err, PersistError::Corrupt), "{err:?}");
    }

    #[test]
    fn test_palette_deduplication() {
        let mut c = PalettedChunk::new();
        c.set_block(0, 7, None);
        c.set_block(1, 7, None);
        // sentinel + one real entry
        assert_eq!(c.palette.len(), 2);
        assert_eq!(c.material_at(0), Some(7));
        assert_eq!(c.material_at(1), Some(7));
    }

    #[test]
    fn test_resource_roundtrip() {
        let mut c = PalettedChunk::new();
        c.set_block(10, 3, Some(42));
        assert_eq!(c.resource_at(10), Some(42));
        c.clear_block(10);
        assert_eq!(c.resource_at(10), None);
    }

    #[test]
    fn test_clear_absent_block_returns_false() {
        let mut c = PalettedChunk::new();
        assert!(!c.clear_block(500));
    }

    #[test]
    fn test_iter_blocks() {
        let mut c = PalettedChunk::new();
        c.set_block(5, 2, None);
        c.set_block(100, 9, Some(1));
        let blocks: Vec<_> = c.iter_blocks().collect();
        assert_eq!(blocks.len(), 2);
        assert!(blocks.iter().any(|&(idx, m, _)| idx == 5 && m == 2));
        assert!(
            blocks
                .iter()
                .any(|&(idx, m, r)| idx == 100 && m == 9 && r == Some(1))
        );
    }
}
