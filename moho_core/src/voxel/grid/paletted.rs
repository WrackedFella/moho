use std::collections::HashMap;

pub const CHUNK_VOL: usize = 4096; // 16³

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
        }
    }

    #[inline]
    pub fn material_at(&self, idx: usize) -> Option<u32> {
        let pi = self.indices[idx] as usize;
        if pi == 0 { None } else { Some(self.palette[pi]) }
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
            .map(|p| p + 1)
            .unwrap_or_else(|| {
                let i = self.palette.len();
                self.palette.push(material_id);
                i
            });
        self.indices[idx] = pi as u16;
        match resource_id {
            Some(rid) => {
                self.resources.insert(idx as u16, rid);
            }
            None => {
                self.resources.remove(&(idx as u16));
            }
        }
    }

    /// Returns `true` if a block was present and removed.
    pub fn clear_block(&mut self, idx: usize) -> bool {
        if self.indices[idx] == 0 {
            return false;
        }
        self.indices[idx] = 0;
        self.resources.remove(&(idx as u16));
        true
    }

    #[allow(dead_code)] // used by chunk streaming (Phase 5) for eviction
    pub fn is_all_air(&self) -> bool {
        self.indices.iter().all(|&i| i == 0)
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
            Some((idx, self.palette[pi as usize], self.resources.get(&(idx as u16)).copied()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_chunk_is_all_air() {
        let c = PalettedChunk::new();
        assert!(c.is_all_air());
        assert_eq!(c.block_count(), 0);
        assert!(c.material_at(0).is_none());
    }

    #[test]
    fn test_set_and_clear_block() {
        let mut c = PalettedChunk::new();
        c.set_block(0, 5, None);
        assert_eq!(c.material_at(0), Some(5));
        assert!(!c.is_all_air());
        assert_eq!(c.block_count(), 1);
        assert!(c.clear_block(0));
        assert!(c.material_at(0).is_none());
        assert!(c.is_all_air());
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
        assert!(blocks.iter().any(|&(idx, m, r)| idx == 100 && m == 9 && r == Some(1)));
    }
}
