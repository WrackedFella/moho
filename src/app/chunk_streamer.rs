//! Per-frame chunk streaming: loads chunks near the player, evicts distant ones.
//!
//! `ChunkStreamer` is the per-frame orchestrator. It owns the terrain config and
//! streaming parameters (sourced from `config/prefs.ini`) and drives all
//! load/evict decisions against the `VoxelGrid` passed in from `LightSystem`.
//!
//! # Invariants
//! - Only generates/loads XZ columns (all Y-chunks in a column at once).
//! - Freshly-generated (unmodified) chunks have their `modified` flag cleared so
//!   they are NOT persisted on eviction — only player edits need disk saves.
//! - Eviction saves happen synchronously in the calling frame, budgeted by
//!   `streaming_config.chunks_per_frame`.

use crate::save;
use glam::{IVec3, Vec3};
use moho_core::voxel::{StreamingConfig, VoxelGrid};
use moho_game::scene_builders::TerrainConfig;
use moho_game::scene_builders::generate_chunk;

/// Inclusive Y-chunk range to generate per XZ column.
///
/// Base elevation is 64, max terrain height is 128 (chunk-Y 8 = y 128..143).
/// MIN=0 preserves the full depth below sea level for future underground content.
const MIN_CHUNK_Y: i32 = 0;
const MAX_CHUNK_Y: i32 = 8;

pub struct ChunkStreamer {
    terrain_config: TerrainConfig,
    streaming_config: StreamingConfig,
    world_name: String,
    last_player_chunk_xz: (i32, i32),
}

impl ChunkStreamer {
    pub fn new(
        terrain_config: TerrainConfig,
        streaming_config: StreamingConfig,
        world_name: impl Into<String>,
    ) -> Self {
        Self {
            terrain_config,
            streaming_config,
            world_name: world_name.into(),
            last_player_chunk_xz: (i32::MAX, i32::MAX),
        }
    }

    /// Per-frame streaming update.
    ///
    /// 1. Evicts chunks beyond `unload_radius`, saving modified ones to disk.
    /// 2. Loads up to `chunks_per_frame` new XZ columns within `load_radius`.
    ///
    /// Returns `(loaded_positions, evicted_positions)` so the caller can
    /// remove stored chunks for evicted chunks.
    pub fn update(&mut self, grid: &mut VoxelGrid, player_pos: Vec3) -> (Vec<IVec3>, Vec<IVec3>) {
        let cx = player_pos.x.floor() as i32 / 16;
        let cz = player_pos.z.floor() as i32 / 16;
        let player_chunk_xz = (cx, cz);

        let load_r = self.streaming_config.load_radius_chunks as i32;
        let unload_r = self.streaming_config.unload_radius_chunks as i32;

        // --- Eviction pass ---
        let evicted = self.evict_distant(grid, player_chunk_xz, unload_r);

        // --- Load pass (budgeted) ---
        let loaded = self.load_nearby(grid, player_chunk_xz, load_r);

        self.last_player_chunk_xz = player_chunk_xz;
        (loaded, evicted)
    }

    fn evict_distant(
        &self,
        grid: &mut VoxelGrid,
        (pcx, pcz): (i32, i32),
        unload_r: i32,
    ) -> Vec<IVec3> {
        let to_evict: Vec<IVec3> = grid
            .chunk_positions()
            .filter(|p| chebyshev_xz(*p, pcx, pcz) > unload_r)
            .collect();

        for &pos in &to_evict {
            if let Some(data) = grid.serialize_chunk(pos) {
                if let Err(e) = save::write_chunk_file(&self.world_name, pos, &data) {
                    tracing::warn!(chunk = ?pos, error = %e, "Failed to save evicted chunk");
                } else {
                    tracing::debug!(chunk = ?pos, "Saved evicted chunk");
                }
            }
            grid.remove_chunk(pos);
        }

        if !to_evict.is_empty() {
            tracing::debug!(count = to_evict.len(), "Evicted chunks");
        }
        to_evict
    }

    fn load_nearby(&self, grid: &mut VoxelGrid, (pcx, pcz): (i32, i32), load_r: i32) -> Vec<IVec3> {
        // Collect XZ columns that need loading (none of their Y-chunks are in the grid yet).
        let loaded_set: std::collections::HashSet<(i32, i32)> =
            grid.chunk_positions().map(|p| (p.x, p.z)).collect();

        let budget = self.streaming_config.chunks_per_frame as usize;
        let mut candidates: Vec<(i32, i32, i32)> = Vec::new(); // (dist, cx, cz)

        for dcx in -load_r..=load_r {
            for dcz in -load_r..=load_r {
                let column_cx = pcx + dcx;
                let column_cz = pcz + dcz;
                if loaded_set.contains(&(column_cx, column_cz)) {
                    continue;
                }
                let dist = chebyshev(dcx, dcz);
                candidates.push((dist, column_cx, column_cz));
            }
        }

        // Closest columns first.
        candidates.sort_unstable();
        candidates.truncate(budget);

        let mut loaded = Vec::new();
        for (_, column_cx, column_cz) in candidates {
            for cy in MIN_CHUNK_Y..=MAX_CHUNK_Y {
                let pos = IVec3::new(column_cx, cy, column_cz);
                let chunk_loaded = self.load_chunk(grid, pos);
                if chunk_loaded {
                    loaded.push(pos);
                }
            }
        }

        if !loaded.is_empty() {
            tracing::debug!(count = loaded.len(), "Loaded chunk(s)");
        }
        loaded
    }

    /// Load one chunk: from disk if a save exists, otherwise generate from seed.
    /// Returns `true` if the chunk has any blocks (not all-air).
    fn load_chunk(&self, grid: &mut VoxelGrid, pos: IVec3) -> bool {
        // Try disk first (player-modified chunk saved on a previous eviction).
        let loaded = save::read_chunk_file(&self.world_name, pos).and_then(|data| {
            data.map_or(Ok(false), |data| {
                grid.deserialize_chunk_into(pos, &data).map(|()| true)
            })
        });
        match loaded {
            Ok(true) => {
                tracing::trace!(chunk = ?pos, "Loaded chunk from disk");
                return true;
            }
            Ok(false) => {}
            Err(error) => {
                tracing::error!(chunk = ?pos, %error, "Unreadable chunk file; regenerating from seed");
            }
        }

        // Generate from the deterministic terrain function.
        let blocks = generate_chunk(&self.terrain_config, pos);
        if blocks.is_empty() {
            return false;
        }
        for (world_pos, material_id, resource_id) in blocks {
            grid.mutator().place(world_pos, material_id, resource_id);
        }
        // Freshly generated chunks are not player-edited — clear the modified flag
        // so they are not needlessly written to disk on eviction.
        grid.clear_chunk_modified(pos);
        true
    }
}

/// Chebyshev distance in XZ from a chunk position to a reference column.
#[inline]
fn chebyshev_xz(pos: IVec3, pcx: i32, pcz: i32) -> i32 {
    chebyshev(pos.x - pcx, pos.z - pcz)
}

#[inline]
fn chebyshev(dx: i32, dz: i32) -> i32 {
    dx.abs().max(dz.abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    // No chunk files exist under saves/ for this name, so every load generates from seed.
    const WORLD: &str = "chunk_streamer_unit_test_nonexistent_world";

    fn streamer(load_radius: u32, budget: u32) -> ChunkStreamer {
        ChunkStreamer::new(
            TerrainConfig::default(),
            StreamingConfig {
                load_radius_chunks: load_radius,
                unload_radius_chunks: load_radius,
                chunks_per_frame: budget,
            },
            WORLD,
        )
    }

    fn columns(positions: &[IVec3]) -> BTreeSet<(i32, i32)> {
        positions.iter().map(|p| (p.x, p.z)).collect()
    }

    fn assert_only_column_loaded(x: f32, z: f32, expected: (i32, i32)) {
        let mut streamer = streamer(0, 1);
        let mut grid = VoxelGrid::new(16);

        let (loaded, evicted) = streamer.update(&mut grid, Vec3::new(x, 64.0, z));

        assert_eq!(columns(&loaded), BTreeSet::from([expected]));
        assert!(evicted.is_empty());
        let in_grid: Vec<IVec3> = grid.chunk_positions().collect();
        assert_eq!(columns(&in_grid), BTreeSet::from([expected]));
    }

    #[test]
    fn update_centers_on_euclidean_chunk_for_negative_positions() {
        assert_only_column_loaded(-1.0, -1.0, (-1, -1));
        assert_only_column_loaded(-17.0, 5.0, (-2, 0));
    }

    #[test]
    fn update_centers_on_chunk_for_positive_positions() {
        assert_only_column_loaded(15.0, 16.0, (0, 1));
    }

    #[test]
    fn update_loads_closest_columns_first_within_budget() {
        let mut streamer = streamer(1, 1);
        let mut grid = VoxelGrid::new(16);

        let (loaded, _) = streamer.update(&mut grid, Vec3::new(0.0, 64.0, 0.0));

        assert_eq!(columns(&loaded), BTreeSet::from([(0, 0)]));
        let in_grid: Vec<IVec3> = grid.chunk_positions().collect();
        assert_eq!(columns(&in_grid), BTreeSet::from([(0, 0)]));
    }
}
