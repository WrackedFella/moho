//! Per-chunk lighting storage.
//!
//! Each loaded chunk owns a `ChunkLight`:
//! - `sky_exposed` — 1 bit per voxel; true iff no opaque voxel sits at or above
//!   this voxel in its world-space (x,z) column.
//! - `block_light_r/g/b` — RGB block-light, each channel `0..=15`. Each channel
//!   propagates independently with decay 1 per step.
//! - `column_max_y` — highest opaque-voxel chunk-local Y per (x,z) column, or
//!   `i8::MIN` for fully air through this chunk. Combined across vertically
//!   stacked chunks to derive world-column heights for sky exposure.
//! - `light_dirty` / `sky_dirty` — track which recompute is needed.
//!
//! `CHUNK_SIZE = 16` is hardcoded; non-16 grids do not get lighting.

use glam::IVec3;

pub const CHUNK_SIZE: i32 = 16;
pub const CHUNK_USIZE: usize = CHUNK_SIZE as usize;
pub const CHUNK_VOLUME: usize = CHUNK_USIZE * CHUNK_USIZE * CHUNK_USIZE;

/// Linear index into a 16³ chunk, given chunk-local (x, y, z) in `[0, 16)`.
#[inline]
pub fn local_index(x: i32, y: i32, z: i32) -> usize {
    debug_assert!(
        (0..CHUNK_SIZE).contains(&x)
            && (0..CHUNK_SIZE).contains(&y)
            && (0..CHUNK_SIZE).contains(&z),
        "local_index out of range: ({x}, {y}, {z})"
    );
    (x as usize) + (y as usize) * CHUNK_USIZE + (z as usize) * CHUNK_USIZE * CHUNK_USIZE
}

/// Convert a world-space block position to (chunk_pos, chunk-local index, chunk-local xyz).
#[inline]
pub fn world_to_chunk_local(pos: IVec3) -> (IVec3, usize, IVec3) {
    let chunk_pos = IVec3::new(
        pos.x.div_euclid(CHUNK_SIZE),
        pos.y.div_euclid(CHUNK_SIZE),
        pos.z.div_euclid(CHUNK_SIZE),
    );
    let local = IVec3::new(
        pos.x.rem_euclid(CHUNK_SIZE),
        pos.y.rem_euclid(CHUNK_SIZE),
        pos.z.rem_euclid(CHUNK_SIZE),
    );
    let idx = local_index(local.x, local.y, local.z);
    (chunk_pos, idx, local)
}

/// Per-chunk lighting state.
///
/// All arrays are dense (4096 voxels) for cache-friendly per-channel BFS.
#[derive(Debug, Clone)]
pub struct ChunkLight {
    /// 1 bit per voxel; `true` if exposed to sky.
    sky_exposed: [u64; CHUNK_VOLUME / 64],

    /// RGB block-light, each channel `0..=15`.
    pub block_light_r: [u8; CHUNK_VOLUME],
    pub block_light_g: [u8; CHUNK_VOLUME],
    pub block_light_b: [u8; CHUNK_VOLUME],

    /// Highest opaque-voxel chunk-local Y per (x, z), or `i8::MIN` for fully air.
    /// Indexed as `column_max_y[x][z]`.
    column_max_y: [[i8; CHUNK_USIZE]; CHUNK_USIZE],

    /// Block-light needs recompute (block placed/removed/emission changed).
    pub light_dirty: bool,
    /// Sky-exposed bitmask needs recompute (column heightmap shifted).
    pub sky_dirty: bool,
}

impl ChunkLight {
    /// Empty chunk-light: all dark, all sky-exposed cleared, no opaque columns.
    pub fn new() -> Self {
        Self {
            sky_exposed: [0u64; CHUNK_VOLUME / 64],
            block_light_r: [0u8; CHUNK_VOLUME],
            block_light_g: [0u8; CHUNK_VOLUME],
            block_light_b: [0u8; CHUNK_VOLUME],
            column_max_y: [[i8::MIN; CHUNK_USIZE]; CHUNK_USIZE],
            light_dirty: true,
            sky_dirty: true,
        }
    }

    // --- sky-exposed bitset ---

    #[inline]
    pub fn sky_exposed_at(&self, idx: usize) -> bool {
        let word = idx / 64;
        let bit = idx % 64;
        (self.sky_exposed[word] >> bit) & 1 == 1
    }

    #[inline]
    pub fn set_sky_exposed(&mut self, idx: usize, value: bool) {
        let word = idx / 64;
        let bit = idx % 64;
        if value {
            self.sky_exposed[word] |= 1u64 << bit;
        } else {
            self.sky_exposed[word] &= !(1u64 << bit);
        }
    }

    /// Clear all sky-exposed bits.
    pub fn clear_sky(&mut self) {
        self.sky_exposed.fill(0);
    }

    // --- RGB block-light ---

    #[inline]
    pub fn block_light_rgb(&self, idx: usize) -> [u8; 3] {
        [
            self.block_light_r[idx],
            self.block_light_g[idx],
            self.block_light_b[idx],
        ]
    }

    #[inline]
    pub fn set_block_light_rgb(&mut self, idx: usize, rgb: [u8; 3]) {
        self.block_light_r[idx] = rgb[0].min(15);
        self.block_light_g[idx] = rgb[1].min(15);
        self.block_light_b[idx] = rgb[2].min(15);
    }

    /// Zero all block-light channels.
    pub fn clear_block_light(&mut self) {
        self.block_light_r.fill(0);
        self.block_light_g.fill(0);
        self.block_light_b.fill(0);
    }

    // --- column heightmap ---

    /// Highest opaque chunk-local Y at (x, z), or `i8::MIN` if column is air.
    #[inline]
    pub fn column_max_y(&self, x: i32, z: i32) -> i8 {
        debug_assert!((0..CHUNK_SIZE).contains(&x) && (0..CHUNK_SIZE).contains(&z));
        self.column_max_y[x as usize][z as usize]
    }

    #[inline]
    pub fn set_column_max_y(&mut self, x: i32, z: i32, y: i8) {
        debug_assert!((0..CHUNK_SIZE).contains(&x) && (0..CHUNK_SIZE).contains(&z));
        self.column_max_y[x as usize][z as usize] = y;
    }

    /// Note that an opaque block was placed at chunk-local (x, y, z). Updates column
    /// max-Y if this raises it.
    pub fn note_opaque_placed(&mut self, x: i32, y: i32, z: i32) {
        let cur = self.column_max_y(x, z);
        let yi8 = y as i8;
        if yi8 > cur {
            self.set_column_max_y(x, z, yi8);
            self.sky_dirty = true;
        }
    }

    /// Direct read of the column heightmap (16×16).
    pub fn column_max_y_grid(&self) -> &[[i8; CHUNK_USIZE]; CHUNK_USIZE] {
        &self.column_max_y
    }
}

impl Default for ChunkLight {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_index_corners() {
        assert_eq!(local_index(0, 0, 0), 0);
        assert_eq!(local_index(15, 0, 0), 15);
        assert_eq!(local_index(0, 1, 0), 16);
        assert_eq!(local_index(0, 0, 1), 256);
        assert_eq!(local_index(15, 15, 15), 4095);
    }

    #[test]
    fn world_to_chunk_local_negative() {
        let (cp, idx, local) = world_to_chunk_local(IVec3::new(-1, -1, -1));
        assert_eq!(cp, IVec3::new(-1, -1, -1));
        assert_eq!(local, IVec3::new(15, 15, 15));
        assert_eq!(idx, 4095);
    }

    #[test]
    fn sky_exposed_set_get() {
        let mut cl = ChunkLight::new();
        assert!(!cl.sky_exposed_at(0));
        cl.set_sky_exposed(0, true);
        cl.set_sky_exposed(63, true);
        cl.set_sky_exposed(64, true);
        cl.set_sky_exposed(4095, true);
        assert!(cl.sky_exposed_at(0));
        assert!(cl.sky_exposed_at(63));
        assert!(cl.sky_exposed_at(64));
        assert!(cl.sky_exposed_at(4095));
        assert!(!cl.sky_exposed_at(1));
        cl.set_sky_exposed(0, false);
        assert!(!cl.sky_exposed_at(0));
    }

    #[test]
    fn block_light_rgb_set_get() {
        let mut cl = ChunkLight::new();
        cl.set_block_light_rgb(100, [15, 8, 2]);
        assert_eq!(cl.block_light_rgb(100), [15, 8, 2]);
        // clamp
        cl.set_block_light_rgb(100, [200, 30, 0]);
        assert_eq!(cl.block_light_rgb(100), [15, 15, 0]);
    }

    #[test]
    fn note_opaque_placed_raises_height() {
        let mut cl = ChunkLight::new();
        assert_eq!(cl.column_max_y(5, 5), i8::MIN);
        cl.note_opaque_placed(5, 3, 5);
        assert_eq!(cl.column_max_y(5, 5), 3);
        assert!(cl.sky_dirty);
        cl.sky_dirty = false;
        // Lower placement does not raise; sky_dirty stays clean.
        cl.note_opaque_placed(5, 1, 5);
        assert_eq!(cl.column_max_y(5, 5), 3);
        assert!(!cl.sky_dirty);
        // Higher placement raises and sets dirty.
        cl.note_opaque_placed(5, 7, 5);
        assert_eq!(cl.column_max_y(5, 5), 7);
        assert!(cl.sky_dirty);
    }
}
