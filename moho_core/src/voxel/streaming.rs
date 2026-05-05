//! Chunk streaming configuration and per-chunk terrain generation.
//!
//! `StreamingConfig` controls how many chunks are loaded/unloaded per frame and
//! at what radii. `generate_chunk` produces the canonical block set for a single
//! 16³ chunk using the same deterministic algorithm as the full terrain generator,
//! so an evicted-then-regenerated unmodified chunk is byte-for-byte identical.

use crate::scene_builders::{
    TerrainConfig, BASE_ELEVATION, MAX_TERRAIN_HEIGHT, determine_material_id, determine_resource_id,
    is_solid, sample_surface_height,
};
use crate::voxel::{BiomeMap, BlockPos};
use glam::IVec3;
use noise::Perlin;

/// Chunk streaming radius and budget parameters.
///
/// Persisted in `config/prefs.ini` under `[world]` so players can tune them
/// without recompiling. Loaded via `moho_ui::Prefs` → `AppConfig` → `ChunkStreamer`.
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// XZ Chebyshev radius (in chunks) within which chunks are kept loaded.
    pub load_radius_chunks: u32,
    /// XZ Chebyshev radius (in chunks) beyond which chunks are evicted.
    /// Must be >= `load_radius_chunks`; enforced at construction.
    pub unload_radius_chunks: u32,
    /// Maximum number of new XZ columns generated per frame.
    pub chunks_per_frame: u32,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            load_radius_chunks: 8,
            unload_radius_chunks: 12,
            chunks_per_frame: 4,
        }
    }
}

/// Chunk size constant — must match `light_storage::CHUNK_SIZE`.
const CHUNK_SIZE: i32 = 16;

/// Generate the solid blocks for a single 16³ chunk at `chunk_pos`.
///
/// Returns `(world_pos, material_id, resource_id)` for each solid block in the
/// chunk, using the same algorithm and seed as the full terrain generator.
///
/// Returns an empty `Vec` when the chunk lies entirely outside the world's XZ
/// bounds (i.e., more than `world_size/2` blocks from the origin).
pub fn generate_chunk(
    config: &TerrainConfig,
    chunk_pos: IVec3,
) -> Vec<(BlockPos, u32, Option<u32>)> {
    let half = (config.world_size / 2) as i32;
    let chunk_min = chunk_pos * CHUNK_SIZE;
    let chunk_max = chunk_min + IVec3::splat(CHUNK_SIZE);

    // Fast-reject: if this chunk has no overlap with [-half, half) in both X and Z → empty.
    if chunk_max.x <= -half || chunk_min.x >= half
        || chunk_max.z <= -half || chunk_min.z >= half
    {
        return vec![];
    }

    let noise = Perlin::new(config.seed);
    let biome_map = BiomeMap::new(config.seed);

    let x_range = chunk_min.x.max(-half)..chunk_max.x.min(half);
    let z_range = chunk_min.z.max(-half)..chunk_max.z.min(half);

    let mut blocks = Vec::new();

    for x in x_range {
        for z in z_range.clone() {
            let biome = biome_map.biome_at(x, z, &config.enabled_biomes);
            let params = biome.params();
            let surface_h = BASE_ELEVATION + sample_surface_height(&noise, x, z, &biome, &params);
            let column_height = (surface_h as i32).clamp(0, MAX_TERRAIN_HEIGHT);

            // Only emit blocks whose Y falls within this chunk's Y range.
            let y_start = chunk_min.y.max(0);
            let y_end = chunk_max.y.min(column_height + 1);

            for y in y_start..y_end {
                if !is_solid(&noise, x, y, z, &biome, &params) {
                    continue;
                }
                let material_id = determine_material_id(column_height, y, &params);
                let resource_id = determine_resource_id(column_height, y, x, z, config.seed);
                blocks.push((BlockPos::new(x, y, z), material_id, resource_id));
            }
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_builders::TerrainConfig;

    fn small_config(seed: u32) -> TerrainConfig {
        TerrainConfig {
            seed,
            world_size: 32,
            ..TerrainConfig::default()
        }
    }

    #[test]
    fn generate_chunk_same_seed_is_deterministic() {
        let cfg = small_config(7);
        let a = generate_chunk(&cfg, IVec3::ZERO);
        let b = generate_chunk(&cfg, IVec3::ZERO);
        assert_eq!(a, b, "same seed + chunk_pos must yield identical blocks");
    }

    #[test]
    fn generate_chunk_outside_world_returns_empty() {
        let cfg = small_config(42);
        // world_size=32, so bounds are -16..16; chunk (5,0,0) → x 80..96 — fully outside
        let far = IVec3::new(5, 0, 0);
        assert!(
            generate_chunk(&cfg, far).is_empty(),
            "chunk outside world bounds must be empty"
        );
    }

    #[test]
    fn generate_chunk_blocks_within_chunk_bounds() {
        let cfg = small_config(1);
        let cp = IVec3::new(0, 0, 0);
        let chunk_min = cp * 16;
        let chunk_max = chunk_min + IVec3::splat(16);
        for (pos, _, _) in generate_chunk(&cfg, cp) {
            assert!(
                pos.x >= chunk_min.x && pos.x < chunk_max.x
                    && pos.y >= chunk_min.y && pos.y < chunk_max.y
                    && pos.z >= chunk_min.z && pos.z < chunk_max.z,
                "block {:?} outside chunk bounds {:?}..{:?}",
                pos, chunk_min, chunk_max
            );
        }
    }

    #[test]
    fn generate_chunk_matches_full_terrain_for_origin_chunk() {
        use crate::voxel::VoxelGrid;

        let cfg = small_config(42);

        // Full terrain generator
        let mut full_grid = VoxelGrid::new(16);
        let mut full_world = legion::World::default();
        crate::scene_builders::voxel_terrain_scene_with_config(&mut full_world, &cfg);
        // We need the grid; use scene_builders internal for comparison
        // (easier: run generate_chunk for all chunks and compare)

        // Collect blocks from generate_chunk for chunk (0,0,0)
        let chunk_blocks: std::collections::HashSet<(i32,i32,i32)> = generate_chunk(&cfg, IVec3::ZERO)
            .into_iter()
            .map(|(p, _, _)| (p.x, p.y, p.z))
            .collect();

        // Generate via full terrain, filter to chunk (0,0,0)
        let mut world2 = legion::World::default();
        let grid2 = crate::scene_builders::voxel_terrain_scene_with_config(&mut world2, &cfg);
        let full_blocks: std::collections::HashSet<(i32,i32,i32)> = grid2
            .iter_block_data()
            .filter(|b| b.position.x >= 0 && b.position.x < 16
                && b.position.y >= 0 && b.position.y < 16
                && b.position.z >= 0 && b.position.z < 16)
            .map(|b| (b.position.x, b.position.y, b.position.z))
            .collect();

        assert_eq!(
            chunk_blocks, full_blocks,
            "generate_chunk must match full terrain for origin chunk"
        );
        let _ = full_grid; // suppress unused warning
    }
}
