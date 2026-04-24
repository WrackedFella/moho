use legion::World;

use crate::voxel::{
    BiomeMap, BiomeParams, BiomeType, BlockPos, LightChannel, LightPropagator, MeshGenerator,
    OreLayout, VoxelBlock, VoxelChunk, VoxelGrid,
};
use bincode::{Decode, Encode};
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

    // Initialize light propagation system
    log::info!("Initializing light propagation...");
    let mut light_propagator = LightPropagator::new(grid.chunk_size());

    // Flood-fill sky light from the top down
    light_propagator.flood_fill(&mut grid, LightChannel::Sky);
    log::info!("Sky light propagation complete");

    // TODO: Consider making grid size configurable via the config struct
    // (e.g., grid_size: u32) so callers can control world extents.

    // Initialize all blocks with cube mesh since smoothing is disabled
    for block in grid.iter_blocks_mut() {
        block.mesh_data = MeshGenerator::cube_mesh();
    }

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
fn pos_hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = seed as u64;
    h ^= (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= (y as u64).wrapping_mul(0x6c62_272e_07bb_0142);
    h ^= (z as u64).wrapping_mul(0x5177_2d3d_ec2c_0b05);
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

/// Generate terrain blocks based on noise
fn generate_terrain(grid: &mut VoxelGrid, config: &TerrainConfig) {
    let noise = Perlin::new(config.seed);
    let biome_map = BiomeMap::new(config.seed);
    // Compute half-size for the -size..size looping used by the original impl.
    // `config.world_size` is the full width in blocks (e.g., 128 -> loop -64..64)
    let size = (config.world_size / 2) as i32;

    // Pass 1: Generate vertical columns of blocks based on per-column biome.
    for x in -size..size {
        for z in -size..size {
            let biome = biome_map.biome_at(x, z, &config.enabled_biomes);
            let params = biome.params();
            let height = sample_height(&noise, x, z, &biome, &params);

            for y in 0..=height {
                let pos = BlockPos::new(x, y, z);
                let material_id = determine_material_id(height, y, &params);
                let resource_id = determine_resource_id(height, y, x, z, config.seed);

                let mut block = VoxelBlock::new(pos, material_id);
                block.resource_id = resource_id;

                grid.set_block(pos, block);
            }
        }
    }
}

/// Sample Perlin noise to determine height at a position.
fn sample_height(noise: &Perlin, x: i32, z: i32, biome: &BiomeType, params: &BiomeParams) -> i32 {
    let mut value = 0.0;
    let mut amplitude = params.surface_amplitude;
    let mut frequency = params.surface_frequency;

    for _ in 0..params.octaves {
        value += noise.get([x as f64 * frequency, z as f64 * frequency]) * amplitude as f64;
        amplitude *= 0.5;
        frequency *= 2.0;
    }

    let shaped = biome.shape(value);
    (shaped.max(0.0) as i32).clamp(0, 32) // Height range [0, 32]
}

/// Determine material ID based on depth within the column, using the biome's materials.
fn determine_material_id(column_height: i32, y: i32, params: &BiomeParams) -> u32 {
    if y == column_height {
        params.surface_material
    } else if y > column_height - 3 {
        params.subsurface_material
    } else {
        params.base_material
    }
}

/// Determine resource ID based on depth (optional resources)
fn determine_resource_id(column_height: i32, y: i32, x: i32, z: i32, seed: u32) -> Option<u32> {
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
        let chunk_pos = VoxelGrid::get_chunk_pos(*block_pos, grid.chunk_size());
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
            .block_positions()
            .map(|p| {
                let b = grid.get_block(p).unwrap();
                (*p, b.material_id, b.resource_id)
            })
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
}
