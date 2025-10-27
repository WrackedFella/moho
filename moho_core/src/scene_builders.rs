use glam::Vec3;
use legion::World;

use crate::actors::{Cube, Sphere};
use crate::materials::MaterialType;
use crate::vector_length;
use crate::voxel::{BlockPos, MeshGenerator, VoxelBlock, VoxelChunk, VoxelGrid};
use bincode::{Decode, Encode};
use noise::{NoiseFn, Perlin};
use rand::{Rng, rng};
use serde::{Deserialize, Serialize};

/// Simple random scene generator used for testing and demos.
/// Moved out of `main.rs` to keep application code minimal.
pub fn random_scene(world: &mut World) {
    let mut rng_local = rng();
    let sphere = Sphere::new(
        Vec3::new(0f32, -1000f32, 0f32),
        1000f32,
        MaterialType::Lambertian {
            albedo: Vec3::new(
                rng_local.random::<f32>() * rng_local.random::<f32>(),
                rng_local.random::<f32>() * rng_local.random::<f32>(),
                rng_local.random::<f32>() * rng_local.random::<f32>(),
            ),
        },
    );
    world.push((sphere,));
    for a in -11..11 {
        for b in -11..11 {
            let choose_mat = rng_local.random::<f32>();

            let center = Vec3::new(
                a as f32 + 0.9f32 * rng_local.random::<f32>(),
                0.2f32,
                b as f32 + 0.9f32 * rng_local.random::<f32>(),
            );
            if vector_length(center - Vec3::new(4f32, 0.2f32, 0f32)) > 0.9f32 {
                if choose_mat < 0.8f32 {
                    // diffuse
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Lambertian {
                            albedo: Vec3::new(
                                rng_local.random::<f32>() * rng_local.random::<f32>(),
                                rng_local.random::<f32>() * rng_local.random::<f32>(),
                                rng_local.random::<f32>() * rng_local.random::<f32>(),
                            ),
                        },
                    );
                    world.push((sphere,));
                } else if choose_mat < 0.95f32 {
                    // metal
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Metal {
                            albedo: Vec3::new(
                                0.5f32 * (1f32 + rng_local.random::<f32>()),
                                0.5f32 * (1f32 + rng_local.random::<f32>()),
                                0.5f32 * (1f32 + rng_local.random::<f32>()),
                            ),
                            fuzz: 0.5f32 * rng_local.random::<f32>(),
                        },
                    );
                    world.push((sphere,));
                } else {
                    // glass
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Dielectric { ref_indx: 1.5f32 },
                    );
                    world.push((sphere,));
                }
            }
        }
    }
    world.push((Cube::new(
        Vec3::new(0f32, 1f32, 0f32),
        1f32,
        1f32,
        1f32,
        MaterialType::Lambertian {
            albedo: Vec3::new(
                rng_local.random::<f32>() * rng_local.random::<f32>(),
                rng_local.random::<f32>() * rng_local.random::<f32>(),
                rng_local.random::<f32>() * rng_local.random::<f32>(),
            ),
        },
    ),));
    world.push((Sphere::new(
        Vec3::new(4f32, 1f32, 0f32),
        1f32,
        MaterialType::Dielectric { ref_indx: 1.5f32 },
    ),));
    world.push((Sphere::new(
        Vec3::new(8f32, 1f32, 0f32),
        1f32,
        MaterialType::Dielectric { ref_indx: 1.5f32 },
    ),));

    log::info!("World Generated");
}

/// Parameters describing a new world request coming from the UI or other
/// front-ends. Defined here so the generator and caller share a single type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode)]
pub struct WorldSpec {
    pub name: String,
    pub seed: Option<u64>,
    pub size_xz: u32,
}

/// Terrain configuration for procedural generation
#[derive(Clone, Serialize, Deserialize)]
pub struct TerrainConfig {
    pub frequency: f64,
    pub amplitude: f32,
    pub octaves: u32,
    pub seed: u32,
    /// World size in blocks along the X/Z axes (full width). The generator
    /// treats this as the total side length; internal code uses half this
    /// value as the +/- loop bound when iterating from -size..size.
    pub world_size: u32,
    pub terrain_type: TerrainType,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum TerrainType {
    GentleHills, // Smooth, rolling terrain
    Mountains,   // Dramatic height variation
    Plains,      // Mostly flat with small bumps
    Cliffs,      // Stepped terrain with vertical faces
    Canyon,      // Deep valleys
}

impl Default for TerrainConfig {
    fn default() -> Self {
        TerrainConfig {
            frequency: 0.05,
            amplitude: 8.0,
            octaves: 3,
            seed: 42,
            world_size: 64,
            terrain_type: TerrainType::GentleHills,
        }
    }
}

/// Generate voxel-based terrain using Perlin noise
/// Two-pass algorithm: 1) Place blocks, 2) Smooth transitions
pub fn voxel_terrain_scene(world: &mut World) {
    let config = TerrainConfig::default();
    voxel_terrain_scene_with_config(world, &config);
}

/// Variant that accepts a custom `TerrainConfig`. This allows callers to
/// control the PRNG seed (and later other parameters) when generating a
/// terrain for new-world generation.
pub fn voxel_terrain_scene_with_config(world: &mut World, config: &TerrainConfig) {
    let mut grid = VoxelGrid::new(64); // 64×64×64 chunks

    log::info!("Generating voxel terrain (seed={})...", config.seed);
    generate_terrain(&mut grid, config);

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
}

/// Generate terrain blocks based on noise
fn generate_terrain(grid: &mut VoxelGrid, config: &TerrainConfig) {
    let noise = Perlin::new(config.seed);
    // Compute half-size for the -size..size looping used by the original impl.
    // `config.world_size` is the full width in blocks (e.g., 128 -> loop -64..64)
    let size = (config.world_size / 2) as i32;

    // Pass 1: Generate vertical columns of blocks based on noise
    for x in -size..size {
        for z in -size..size {
            // Sample noise for height
            let height = sample_height(&noise, x, z, config);

            // Generate vertical column of blocks
            for y in 0..=height {
                let pos = BlockPos::new(x, y, z);
                let material_id = determine_material_id(height, y);
                let resource_id = determine_resource_id(height, y);

                let mut block = VoxelBlock::new(pos, material_id);
                block.resource_id = resource_id;

                grid.set_block(pos, block);
            }
        }
    }
}

/// Sample Perlin noise to determine height at a position
fn sample_height(noise: &Perlin, x: i32, z: i32, config: &TerrainConfig) -> i32 {
    let mut value = 0.0;
    let mut amplitude = config.amplitude;
    let mut frequency = config.frequency;

    // Multi-octave Perlin noise
    for _ in 0..config.octaves {
        value += noise.get([x as f64 * frequency, z as f64 * frequency]) * amplitude as f64;

        amplitude *= 0.5;
        frequency *= 2.0;
    }

    // Terrain type modifiers
    value = match config.terrain_type {
        TerrainType::GentleHills => value,
        TerrainType::Mountains => value * 2.0,
        TerrainType::Plains => value * 0.3,
        TerrainType::Cliffs => (value * 4.0).floor() / 4.0, // Stepped
        TerrainType::Canyon => {
            // Negative in valleys, positive on ridges
            if value < 0.0 {
                value * 2.0
            } else {
                value * 0.5
            }
        }
    };

    (value.max(0.0) as i32).clamp(0, 32) // Height range [0, 32]
}

/// Determine material ID based on height and depth
fn determine_material_id(column_height: i32, y: i32) -> u32 {
    // Material variation by height
    if y == column_height {
        0 // Grass material (top layer)
    } else if y > column_height - 3 {
        1 // Dirt material (sub-surface)
    } else {
        2 // Stone material (deep)
    }
}

/// Determine resource ID based on depth (optional resources)
fn determine_resource_id(column_height: i32, y: i32) -> Option<u32> {
    // Distribute resources based on depth
    // 10% chance of iron ore in mid-levels
    if y > 5 && y < column_height - 3 {
        let mut rng_local = rng();
        if rng_local.random::<f32>() < 0.1 {
            Some(1) // Iron ore resource ID
        } else {
            None
        }
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
        let chunk = VoxelChunk::from_grid(grid, chunk_pos);

        // Only include non-empty chunks
        if !chunk.is_empty() {
            log::debug!(
                "Chunk at {:?}: {} vertices, {} indices",
                chunk_pos,
                chunk.vertices.len(),
                chunk.indices.len()
            );
            chunks.push(chunk);
        }
    }

    log::info!("Generated {} non-empty chunks", chunks.len());
    chunks
}
