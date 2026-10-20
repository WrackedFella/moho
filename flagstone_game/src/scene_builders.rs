use crate::biome::{BiomeMap, BiomeParams, BiomeType, OreLayout};
use crate::perlin::Perlin;
use flagstone_voxel::{BlockPos, ChunkStore, LightPropagator, VoxelChunk, VoxelGrid};
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
            noise.sample(f64::from(x) * frequency, f64::from(z) * frequency) * f64::from(amplitude);
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

    type Block = (BlockPos, u32, Option<u32>);

    fn small_config(seed: u32) -> TerrainConfig {
        TerrainConfig {
            seed,
            world_size: 32,
            ..TerrainConfig::default()
        }
    }

    fn all_biome_config() -> TerrainConfig {
        TerrainConfig {
            seed: 7,
            world_size: 64,
            enabled_biomes: vec![
                BiomeType::GentleHills,
                BiomeType::Mountains,
                BiomeType::Plains,
                BiomeType::Cliffs,
                BiomeType::Canyon,
            ],
            ..TerrainConfig::default()
        }
    }

    /// Every chunk position that can hold terrain, with the blocks `generate_chunk` gives it.
    fn world_chunks(config: &TerrainConfig) -> Vec<(glam::IVec3, Vec<Block>)> {
        let half = (config.world_size / 2) as i32;
        let lo = (-half).div_euclid(CHUNK_SIZE);
        let hi = (half - 1).div_euclid(CHUNK_SIZE);
        let mut chunks = Vec::new();
        for cx in lo..=hi {
            for cy in 0..=MAX_TERRAIN_HEIGHT / CHUNK_SIZE {
                for cz in lo..=hi {
                    let pos = glam::IVec3::new(cx, cy, cz);
                    chunks.push((pos, generate_chunk(config, pos)));
                }
            }
        }
        chunks
    }

    fn world_blocks(config: &TerrainConfig) -> Vec<Block> {
        let mut blocks: Vec<Block> = world_chunks(config)
            .into_iter()
            .flat_map(|(_, blocks)| blocks)
            .collect();
        blocks.sort_by_key(|(p, _, _)| (p.x, p.y, p.z));
        blocks
    }

    /// 64-bit FNV-1a over the sorted blocks; `None` resources get a distinct tag byte.
    fn fingerprint(blocks: &[Block]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for &b in bytes {
                hash ^= u64::from(b);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for (p, material, resource) in blocks {
            feed(&p.x.to_le_bytes());
            feed(&p.y.to_le_bytes());
            feed(&p.z.to_le_bytes());
            feed(&material.to_le_bytes());
            match resource {
                None => feed(&[0]),
                Some(n) => {
                    feed(&[1]);
                    feed(&n.to_le_bytes());
                }
            }
        }
        hash
    }

    fn ore_count(blocks: &[Block]) -> usize {
        blocks.iter().filter(|(_, _, r)| *r == Some(1)).count()
    }

    #[test]
    fn default_world_matches_its_pinned_fingerprint() {
        let blocks = world_blocks(&small_config(42));

        let measured = (fingerprint(&blocks), blocks.len(), ore_count(&blocks));

        assert_eq!(
            measured,
            (16_667_745_297_482_914_019, 67_114, 5_792),
            "world generation output changed; update these pins only when the change is deliberate"
        );
    }

    #[test]
    fn all_biome_world_matches_its_pinned_fingerprint() {
        let blocks = world_blocks(&all_biome_config());

        let measured = (fingerprint(&blocks), blocks.len(), ore_count(&blocks));

        assert_eq!(
            measured,
            (3_431_315_624_261_164_992, 265_476, 22_876),
            "world generation output changed; update these pins only when the change is deliberate"
        );
    }

    #[test]
    fn different_seeds_produce_different_grids() {
        let a = world_blocks(&small_config(42));
        let b = world_blocks(&small_config(43));

        assert_ne!(a, b, "different seeds must produce different terrain");
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
    fn chunk_past_each_world_edge_is_empty() {
        let cfg = small_config(42);

        // (outside chunk, in-world neighbour), one edge at a time.
        let edges = [
            ((1, 4, 0), (0, 4, 0)),
            ((-2, 4, 0), (-1, 4, 0)),
            ((0, 4, 1), (0, 4, 0)),
            ((0, 4, -2), (0, 4, -1)),
        ];

        for (outside, inside) in edges {
            let outside = glam::IVec3::from(outside);
            let inside = glam::IVec3::from(inside);
            assert!(
                generate_chunk(&cfg, outside).is_empty(),
                "chunk {outside:?} lies past one edge and must be empty"
            );
            assert!(
                !generate_chunk(&cfg, inside).is_empty(),
                "chunk {inside:?} is in the world and must have blocks"
            );
        }
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
    fn chunks_tile_the_world_without_overlap() {
        use std::collections::HashSet;
        let cfg = all_biome_config();
        let half = (cfg.world_size / 2) as i32;
        let mut seen = HashSet::new();
        let mut columns = HashSet::new();

        for (chunk, blocks) in world_chunks(&cfg) {
            let min = chunk * CHUNK_SIZE;
            let max = min + glam::IVec3::splat(CHUNK_SIZE);
            for (p, _, _) in blocks {
                assert!(
                    (min.x..max.x).contains(&p.x)
                        && (min.y..max.y).contains(&p.y)
                        && (min.z..max.z).contains(&p.z),
                    "block {p:?} lies outside its chunk {chunk:?}"
                );
                assert!(seen.insert((p.x, p.y, p.z)), "block {p:?} is in two chunks");
                columns.insert((p.x, p.z));
            }
        }

        for x in -half..half {
            for z in -half..half {
                assert!(columns.contains(&(x, z)), "column ({x}, {z}) has no blocks");
            }
        }
    }

    #[test]
    fn block_at_zero_density_is_air() {
        // Perlin noise is 0 at lattice point (0, 0), so the surface sits exactly at BASE_ELEVATION.
        let noise = Perlin::new(42);
        let biome = BiomeType::GentleHills;
        let params = biome.params();
        let surface_y = BASE_ELEVATION as i32;

        let at_surface = density(&noise, 0, surface_y, 0, &biome, &params);

        assert_eq!(
            at_surface.to_bits(),
            0.0_f32.to_bits(),
            "density must be +0.0"
        );
        assert!(!is_solid(&noise, 0, surface_y, 0, &biome, &params));
        assert!(is_solid(&noise, 0, surface_y - 1, 0, &biome, &params));
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

    #[test]
    fn golden_surface_heights_are_pinned() {
        let noise = Perlin::new(42);
        let biome = BiomeType::GentleHills;
        let params = biome.params();
        let pinned = [
            ((-33, 19), 1.372_482_3),
            ((-19, 12), 3.875_477_3),
            ((-12, 9), 5.168_122_3),
            ((23, -8), 1.547_490_2),
        ];

        for ((x, z), expected) in pinned {
            let h = sample_surface_height(&noise, x, z, &biome, &params);
            assert!(
                (h - expected).abs() < 1e-6,
                "({x}, {z}) = {h}, pinned {expected}"
            );
        }
    }
}
