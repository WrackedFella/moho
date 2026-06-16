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
use moho_core::scene_builders::TerrainConfig;
use moho_core::voxel::streaming::generate_chunk;
use moho_core::voxel::{StreamingConfig, VoxelGrid};

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
    /// remove ECS entities for evicted chunks.
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
                    log::warn!("Failed to save evicted chunk {:?}: {}", pos, e);
                } else {
                    log::debug!("Saved evicted chunk {:?}", pos);
                }
            }
            grid.remove_chunk(pos);
        }

        if !to_evict.is_empty() {
            log::debug!("Evicted {} chunks", to_evict.len());
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
            log::debug!("Loaded {} chunk(s)", loaded.len());
        }
        loaded
    }

    /// Load one chunk: from disk if a save exists, otherwise generate from seed.
    /// Returns `true` if the chunk has any blocks (not all-air).
    fn load_chunk(&self, grid: &mut VoxelGrid, pos: IVec3) -> bool {
        // Try disk first (player-modified chunk saved on a previous eviction).
        if let Some(data) = save::read_chunk_file(&self.world_name, pos) {
            if grid.deserialize_chunk_into(pos, &data) {
                log::trace!("Loaded chunk {:?} from disk", pos);
                return true;
            }
            log::warn!("Corrupt chunk file for {:?}; regenerating", pos);
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
