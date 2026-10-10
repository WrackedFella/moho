use crate::biome::{BiomeMap, BiomeParams, BiomeType, OreLayout};
use moho_voxel::{BlockPos, ChunkStore, LightPropagator, VoxelChunk, VoxelGrid};
use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

/// Parameters describing a new world request coming from the UI or other
/// front-ends. Defined here so the generator and caller share a single type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
pub fn voxel_terrain_scene(chunks: &mut ChunkStore) -> VoxelGrid {
    let config = TerrainConfig::default();
    voxel_terrain_scene_with_config(chunks, &config)
}

/// Variant that accepts a custom `TerrainConfig`. This allows callers to
/// control the PRNG seed (and later other parameters) when generating a
/// terrain for new-world generation.
///
/// Returns the VoxelGrid for use with light propagation system.
pub fn voxel_terrain_scene_with_config(
    chunks: &mut ChunkStore,
    config: &TerrainConfig,
) -> VoxelGrid {
    // NOTE: Hybrid mesh generation currently requires chunk_size=16
    // due to hardcoded density field size in Marching Cubes
    let mut grid = VoxelGrid::new(16); // 16×16×16 chunks

    tracing::info!(seed = config.seed, "Generating voxel terrain");
    generate_terrain(&mut grid, config);

    // Initialize block light propagation from all emissive blocks.
    tracing::info!("Initializing light propagation...");
    let mut light_propagator = LightPropagator::new();
    light_propagator.flood_fill_block_lights(&mut grid);
    tracing::info!("Block light propagation complete");

    // TODO: Consider making grid size configurable via the config struct
    // (e.g., grid_size: u32) so callers can control world extents.

    tracing::info!("Converting grid to renderable chunks...");
    for chunk in grid_to_chunks(&grid) {
        chunks.insert(chunk);
    }

    tracing::info!(count = chunks.len(), "Voxel terrain scene ready");

    // Return the grid so it can be stored by the caller for light propagation
    grid
}

/// Deterministic positional hash for use during terrain generation.
/// Produces a value in [0.0, 1.0) that depends only on position and seed —
/// no entropy-seeded RNG, so generation is a pure function of its inputs.
pub(crate) fn pos_hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = u64::from(seed);
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
        value +=
            noise.get([f64::from(x) * frequency, f64::from(z) * frequency]) * f64::from(amplitude);
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

    tracing::info!(count = chunk_positions.len(), "Converting chunks from grid");

    // Generate a VoxelChunk for each chunk position
    let mut chunks = Vec::new();
    for chunk_pos in chunk_positions {
        let chunk = VoxelChunk::from_grid_hybrid(grid, chunk_pos);

        // Only include non-empty chunks
        if !chunk.is_empty() {
            tracing::debug!(
                chunk = ?chunk_pos,
                vertices = chunk.vertices().len(),
                indices = chunk.indices().len(),
                "Chunk generated"
            );
            chunks.push(chunk);
        }
    }

    tracing::info!(count = chunks.len(), "Generated non-empty chunks");
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

        for outside in [(1, 0, 0), (-2, 0, 0), (0, 0, 1), (0, 0, -2)] {
            let chunk = glam::IVec3::from(outside);
            assert!(
                generate_chunk(&cfg, chunk).is_empty(),
                "chunk {chunk:?} lies outside the world and must be empty"
            );
        }

        let edge = glam::IVec3::new(-1, 0, 0);
        assert!(
            !generate_chunk(&cfg, edge).is_empty(),
            "chunk {edge:?} touches the world's -X edge and must have blocks"
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
                "block {pos:?} outside chunk bounds {chunk_min:?}..{chunk_max:?}"
            );
        }
    }

    #[test]
    fn generate_chunk_matches_full_terrain() {
        let cfg = small_config(42);
        let full = collect_blocks(&generate_grid(42));

        for chunk_pos in [glam::IVec3::new(0, 4, 0), glam::IVec3::new(-1, 4, -1)] {
            let min = chunk_pos * CHUNK_SIZE;
            let max = min + glam::IVec3::splat(CHUNK_SIZE);
            let expected: Vec<_> = full
                .iter()
                .filter(|(p, _, _)| {
                    (min.x..max.x).contains(&p.x)
                        && (min.y..max.y).contains(&p.y)
                        && (min.z..max.z).contains(&p.z)
                })
                .copied()
                .collect();
            let mut actual = generate_chunk(&cfg, chunk_pos);
            actual.sort_by_key(|(p, _, _)| (p.x, p.y, p.z));

            assert!(!actual.is_empty(), "chunk {chunk_pos:?} must have blocks");
            assert_eq!(
                actual, expected,
                "generate_chunk must match full terrain for chunk {chunk_pos:?}"
            );
        }
    }

    #[test]
    fn terrain_column_layers_and_ore_band() {
        let params = BiomeParams {
            surface_material: 10,
            subsurface_material: 20,
            base_material: 30,
            ..BiomeType::GentleHills.params()
        };
        let column_height = 70;

        let materials: Vec<u32> = (66..=70)
            .rev()
            .map(|y| determine_material_id(column_height, y, &params))
            .collect();

        assert_eq!(materials, vec![10, 10, 20, 20, 30]);

        // Ore seen at each band edge, so a narrowed band fails too.
        let (mut ore_at_bottom, mut ore_at_top) = (false, false);
        for seed in 0..4 {
            for x in -8..8 {
                for z in -8..8 {
                    for y in 0..=column_height {
                        let ore = determine_resource_id(column_height, y, x, z, seed);
                        if (6..=66).contains(&y) {
                            ore_at_bottom |= y == 6 && ore == Some(1);
                            ore_at_top |= y == 66 && ore == Some(1);
                        } else {
                            assert_eq!(ore, None, "no ore expected at y={y}");
                        }
                    }
                }
            }
        }
        assert!(ore_at_bottom, "expected ore somewhere at y=6");
        assert!(ore_at_top, "expected ore somewhere at y=66");
    }
}
