use legion::World;

use bincode::{Decode, Encode};
use moho_core::voxel::{
    BiomeMap, BiomeParams, BiomeType, BlockPos, LightPropagator, OreLayout, VoxelChunk, VoxelGrid,
};
use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

/// Parameters describing a new world request coming from the UI or other
/// front-ends. Defined here so the generator and caller share a single type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode)]
pub struct WorldSpec {
    pub name: String,
    pub seed: Option<u64>,
    pub size_xz: u32,
    /// Length of daytime in real-world seconds (e.g., 600.0 = 10 minute day)
    pub day_length_seconds: f32,
    /// Length of nighttime in real-world seconds (e.g., 300.0 = 5 minute night)
    pub night_length_seconds: f32,
    /// Initial time when world starts (0.0 = midnight, 6.0 = dawn, 12.0 = noon, 18.0 = dusk)
    pub initial_time_of_day: f32,
}

impl Default for WorldSpec {
    fn default() -> Self {
        Self {
            name: "New World".to_string(),
            seed: None,
            size_xz: 64,
            day_length_seconds: 600.0,   // 10 minute days
            night_length_seconds: 420.0, // 7 minute nights
            initial_time_of_day: 6.0,    // Start at dawn
        }
    }
}

/// Terrain configuration for procedural generation.
///
/// Biomes replace the old single `terrain_type`: each column picks a biome
/// from `enabled_biomes` via a low-frequency biome map, and terrain params
/// are read from the biome's `BiomeParams`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainConfig {
    pub seed: u32,
    /// World size in blocks along the X/Z axes (full width). The generator
    /// treats this as the total side length; internal code uses half this
    /// value as the +/- loop bound when iterating from -size..size.
    pub world_size: u32,
    /// Biomes permitted in this world. Must be non-empty; the biome map
    /// picks among these per column.
    pub enabled_biomes: Vec<BiomeType>,
    pub ore_layout: OreLayout,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        TerrainConfig {
            seed: 42,
            world_size: 64,
            enabled_biomes: vec![BiomeType::GentleHills],
            ore_layout: OreLayout::default(),
        }
    }
}

/// Generate voxel-based terrain using Perlin noise
/// Two-pass algorithm: 1) Place blocks, 2) Smooth transitions
pub fn voxel_terrain_scene(world: &mut World) -> VoxelGrid {
    let config = TerrainConfig::default();
    voxel_terrain_scene_with_config(world, &config)
}

/// Variant that accepts a custom `TerrainConfig`. This allows callers to
/// control the PRNG seed (and later other parameters) when generating a
/// terrain for new-world generation.
///
/// Returns the VoxelGrid for use with light propagation system.
pub fn voxel_terrain_scene_with_config(world: &mut World, config: &TerrainConfig) -> VoxelGrid {
    // NOTE: Hybrid mesh generation currently requires chunk_size=16
    // due to hardcoded density field size in Marching Cubes
    let mut grid = VoxelGrid::new(16); // 16×16×16 chunks

    log::info!("Generating voxel terrain (seed={})...", config.seed);
    generate_terrain(&mut grid, config);

    // Initialize block light propagation from all emissive blocks.
    log::info!("Initializing light propagation...");
    let mut light_propagator = LightPropagator::new(grid.chunk_size());
    light_propagator.flood_fill_block_lights(&mut grid);
    log::info!("Block light propagation complete");

    // TODO: Consider making grid size configurable via the config struct
    // (e.g., grid_size: u32) so callers can control world extents.

    log::info!("Converting grid to renderable chunks...");
    let chunks = grid_to_chunks(&grid);

    // Push each chunk as an entity in the world
    for chunk in chunks {
        world.push((chunk,));
    }

    log::info!(
        "Voxel terrain scene ready with {} chunk entities",
        world.len()
    );

    // Return the grid so it can be stored by the caller for light propagation
    grid
}

/// Deterministic positional hash for use during terrain generation.
/// Produces a value in [0.0, 1.0) that depends only on position and seed —
/// no entropy-seeded RNG, so generation is a pure function of its inputs.
pub(crate) fn pos_hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = seed as u64;
    h ^= (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= (y as u64).wrapping_mul(0x6c62_272e_07bb_0142);
    h ^= (z as u64).wrapping_mul(0x5177_2d3d_ec2c_0b05);
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

/// Max block height for the terrain column (clamp for the legacy 32-tall range).
pub(crate) const MAX_TERRAIN_HEIGHT: i32 = 128;
pub(crate) const BASE_ELEVATION: f32 = 64.0;

/// Generate terrain blocks using a 3D density field.
///
/// For each column, we pick a biome, sample a continuous surface height,
/// then iterate blocks up to the column's solid top, emitting any block
/// whose `density(x, y, z) > 0.0`. This keeps today's heightmap-shaped
/// result while giving cave carving a natural hook (just subtract cave
/// noise from density) and keeping the ore scan mechanic pure.
fn generate_terrain(grid: &mut VoxelGrid, config: &TerrainConfig) {
    let noise = Perlin::new(config.seed);
    let biome_map = BiomeMap::new(config.seed);
    // `config.world_size` is the full width in blocks (e.g., 128 -> loop -64..64)
    let size = (config.world_size / 2) as i32;

    for x in -size..size {
        for z in -size..size {
            let biome = biome_map.biome_at(x, z, &config.enabled_biomes);
            let params = biome.params();
            let surface_h = BASE_ELEVATION + sample_surface_height(&noise, x, z, &biome, &params);
            let column_height = (surface_h as i32).clamp(0, MAX_TERRAIN_HEIGHT);

            for y in 0..=column_height {
                if !is_solid(&noise, x, y, z, &biome, &params) {
                    continue;
                }
                let pos = BlockPos::new(x, y, z);
                let material_id = determine_material_id(column_height, y, &params);
                let resource_id = determine_resource_id(column_height, y, x, z, config.seed);

                grid.mutator().place(pos, material_id, resource_id);
            }
        }
    }
}

/// Continuous surface height at `(x, z)`. Multi-octave Perlin noise scaled
/// by the biome's amplitude/frequency/octaves, shaped by `BiomeType::shape`,
/// clamped to non-negative.
pub(crate) fn sample_surface_height(
    noise: &Perlin,
    x: i32,
    z: i32,
    biome: &BiomeType,
    params: &BiomeParams,
) -> f32 {
    let mut value = 0.0;
    let mut amplitude = params.surface_amplitude;
    let mut frequency = params.surface_frequency;

    for _ in 0..params.octaves {
        value += noise.get([x as f64 * frequency, z as f64 * frequency]) * amplitude as f64;
        amplitude *= 0.5;
        frequency *= 2.0;
    }

    biome.shape(value).max(0.0) as f32
}

/// Density at `(x, y, z)`. Positive means solid rock, negative means air.
///
/// Compares world-space y against the full surface height (BASE_ELEVATION +
/// noise offset). Without BASE_ELEVATION here, `is_solid` would disagree with
/// the column_height used in the terrain loops and reject every block above y≈8.
pub(crate) fn density(
    noise: &Perlin,
    x: i32,
    y: i32,
    z: i32,
    biome: &BiomeType,
    params: &BiomeParams,
) -> f32 {
    let surface_h = BASE_ELEVATION + sample_surface_height(noise, x, z, biome, params);
    let base = surface_h - y as f32;
    // Cave carver hook (disabled until caves are enabled):
    // base - cave_noise(noise, x, y, z) * params.cave_density
    let _ = params.cave_density;
    base
}

/// Whether the block centered at `(x, y, z)` should be filled.
pub(crate) fn is_solid(
    noise: &Perlin,
    x: i32,
    y: i32,
    z: i32,
    biome: &BiomeType,
    params: &BiomeParams,
) -> bool {
    density(noise, x, y, z, biome, params) > 0.0
}

/// Determine material ID based on depth within the column, using the biome's materials.
///
/// The top solid block is at `column_height` when surface_h has a fractional
/// part, or at `column_height - 1` when it is exactly integer (Perlin = 0 at
/// origin). Checking `>= column_height - 1` covers both cases.
pub(crate) fn determine_material_id(column_height: i32, y: i32, params: &BiomeParams) -> u32 {
    if y >= column_height - 1 {
        params.surface_material
    } else if y > column_height - 4 {
        params.subsurface_material
    } else {
        params.base_material
    }
}

/// Determine resource ID based on depth (optional resources)
pub(crate) fn determine_resource_id(
    column_height: i32,
    y: i32,
    x: i32,
    z: i32,
    seed: u32,
) -> Option<u32> {
    // 10% chance of iron ore in mid-levels, determined by positional hash
    if y > 5 && y < column_height - 3 && pos_hash(x, y, z, seed) < 0.1 {
        Some(1) // Iron ore resource ID
    } else {
        None
    }
}

/// Convert VoxelGrid to optimized VoxelChunks for rendering
fn grid_to_chunks(grid: &VoxelGrid) -> Vec<VoxelChunk> {
    use std::collections::HashSet;

    // Find all unique chunk positions from the blocks
    let mut chunk_positions = HashSet::new();
    for block_pos in grid.block_positions() {
        let chunk_pos = VoxelGrid::get_chunk_pos(block_pos, grid.chunk_size());
        chunk_positions.insert(chunk_pos);
    }

    log::info!("Converting {} chunks from grid", chunk_positions.len());

    // Generate a VoxelChunk for each chunk position
    let mut chunks = Vec::new();
    for chunk_pos in chunk_positions {
        let chunk = VoxelChunk::from_grid_hybrid(grid, chunk_pos);

        // Only include non-empty chunks
        if !chunk.is_empty() {
            log::debug!(
                "Chunk at {:?}: {} vertices, {} indices",
                chunk_pos,
                chunk.vertices().len(),
                chunk.indices().len()
            );
            chunks.push(chunk);
        }
    }

    log::info!("Generated {} non-empty chunks", chunks.len());
    chunks
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
    chunk_pos: glam::IVec3,
) -> Vec<(BlockPos, u32, Option<u32>)> {
    let half = (config.world_size / 2) as i32;
    let chunk_min = chunk_pos * CHUNK_SIZE;
    let chunk_max = chunk_min + glam::IVec3::splat(CHUNK_SIZE);

    // Fast-reject: if this chunk has no overlap with [-half, half) in both X and Z → empty.
    if chunk_max.x <= -half || chunk_min.x >= half || chunk_max.z <= -half || chunk_min.z >= half {
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

    fn generate_grid(seed: u32) -> VoxelGrid {
        let config = TerrainConfig {
            seed,
            world_size: 32, // small enough to run quickly
            ..TerrainConfig::default()
        };
        let mut grid = VoxelGrid::new(16);
        generate_terrain(&mut grid, &config);
        grid
    }

    fn collect_blocks(grid: &VoxelGrid) -> Vec<(BlockPos, u32, Option<u32>)> {
        let mut blocks: Vec<_> = grid
            .iter_block_data()
            .map(|b| (b.position, b.material_id, b.resource_id))
            .collect();
        blocks.sort_by_key(|(p, _, _)| (p.x, p.y, p.z));
        blocks
    }

    #[test]
    fn same_seed_produces_same_grid() {
        let a = collect_blocks(&generate_grid(42));
        let b = collect_blocks(&generate_grid(42));
        assert_eq!(a, b, "same seed must produce identical terrain");
    }

    #[test]
    fn different_seeds_produce_different_grids() {
        let a = collect_blocks(&generate_grid(42));
        let b = collect_blocks(&generate_grid(43));
        assert_ne!(a, b, "different seeds must produce different terrain");
    }

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
        let a = generate_chunk(&cfg, glam::IVec3::ZERO);
        let b = generate_chunk(&cfg, glam::IVec3::ZERO);
        assert_eq!(a, b, "same seed + chunk_pos must yield identical blocks");
    }

    #[test]
    fn generate_chunk_outside_world_returns_empty() {
        let cfg = small_config(42);
        let far = glam::IVec3::new(5, 0, 0);
        assert!(
            generate_chunk(&cfg, far).is_empty(),
            "chunk outside world bounds must be empty"
        );
    }

    #[test]
    fn generate_chunk_blocks_within_chunk_bounds() {
        let cfg = small_config(1);
        let cp = glam::IVec3::new(0, 0, 0);
        let chunk_min = cp * 16;
        let chunk_max = chunk_min + glam::IVec3::splat(16);
        for (pos, _, _) in generate_chunk(&cfg, cp) {
            assert!(
                pos.x >= chunk_min.x
                    && pos.x < chunk_max.x
                    && pos.y >= chunk_min.y
                    && pos.y < chunk_max.y
                    && pos.z >= chunk_min.z
                    && pos.z < chunk_max.z,
                "block {:?} outside chunk bounds {:?}..{:?}",
                pos,
                chunk_min,
                chunk_max
            );
        }
    }

    #[test]
    fn generate_chunk_matches_full_terrain_for_origin_chunk() {
        let cfg = small_config(42);

        // Collect blocks from generate_chunk for chunk (0,0,0)
        let chunk_blocks: std::collections::HashSet<(i32, i32, i32)> =
            generate_chunk(&cfg, glam::IVec3::ZERO)
                .into_iter()
                .map(|(p, _, _)| (p.x, p.y, p.z))
                .collect();

        // Generate via full terrain, filter to chunk (0,0,0)
        let mut world = legion::World::default();
        let grid = voxel_terrain_scene_with_config(&mut world, &cfg);
        let full_blocks: std::collections::HashSet<(i32, i32, i32)> = grid
            .iter_block_data()
            .filter(|b| {
                b.position.x >= 0
                    && b.position.x < 16
                    && b.position.y >= 0
                    && b.position.y < 16
                    && b.position.z >= 0
                    && b.position.z < 16
            })
            .map(|b| (b.position.x, b.position.y, b.position.z))
            .collect();

        assert_eq!(
            chunk_blocks, full_blocks,
            "generate_chunk must match full terrain for origin chunk"
        );
    }
}
