//! Biome system for terrain generation.
//!
//! A `BiomeType` names a region's terrain character. `BiomeParams` holds the
//! numerical parameters (noise amplitude/frequency, material IDs, etc.) used
//! to realize that character at a given position.
//!
//! `BiomeMap` picks a biome per XZ column from a low-frequency noise field,
//! so worlds can contain multiple biomes arranged in regions. Generation is a
//! pure function of `(x, z, seed, enabled_biomes)`.

use bincode::{Decode, Encode};
use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

/// Named biome. Each variant has a distinct terrain shape realized by
/// `BiomeType::shape(raw_noise)` in addition to its numerical `params()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub enum BiomeType {
    GentleHills,
    Mountains,
    Plains,
    Cliffs,
    Canyon,
}

/// Numerical parameters describing a biome's terrain response.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiomeParams {
    pub surface_amplitude: f32,
    pub surface_frequency: f64,
    pub octaves: u32,
    /// Multiplier for cave-noise contribution. 0.0 disables caves.
    pub cave_density: f32,
    /// Top-layer material (grass, sand, ...).
    pub surface_material: u32,
    /// Just below surface (dirt, packed sand, ...).
    pub subsurface_material: u32,
    /// Deep rock (stone).
    pub base_material: u32,
}

impl BiomeType {
    /// Per-biome numerical parameters.
    pub fn params(&self) -> BiomeParams {
        match self {
            BiomeType::GentleHills => BiomeParams {
                surface_amplitude: 8.0,
                surface_frequency: 0.05,
                octaves: 3,
                cave_density: 0.0,
                surface_material: 0,    // grass
                subsurface_material: 1, // dirt
                base_material: 2,       // stone
            },
            BiomeType::Mountains => BiomeParams {
                surface_amplitude: 16.0,
                surface_frequency: 0.04,
                octaves: 4,
                cave_density: 0.0,
                surface_material: 0,
                subsurface_material: 1,
                base_material: 2,
            },
            BiomeType::Plains => BiomeParams {
                surface_amplitude: 2.4,
                surface_frequency: 0.06,
                octaves: 2,
                cave_density: 0.0,
                surface_material: 0,
                subsurface_material: 1,
                base_material: 2,
            },
            BiomeType::Cliffs => BiomeParams {
                surface_amplitude: 12.0,
                surface_frequency: 0.05,
                octaves: 3,
                cave_density: 0.0,
                surface_material: 0,
                subsurface_material: 1,
                base_material: 2,
            },
            BiomeType::Canyon => BiomeParams {
                surface_amplitude: 10.0,
                surface_frequency: 0.04,
                octaves: 3,
                cave_density: 0.0,
                surface_material: 0,
                subsurface_material: 1,
                base_material: 2,
            },
        }
    }

    /// Apply the biome's distinctive shape to a raw multi-octave noise sample.
    /// `raw` already includes this biome's amplitude/frequency/octaves.
    pub fn shape(&self, raw: f64) -> f64 {
        match self {
            BiomeType::GentleHills | BiomeType::Mountains | BiomeType::Plains => raw,
            BiomeType::Cliffs => (raw * 4.0).floor() / 4.0,
            BiomeType::Canyon => {
                if raw < 0.0 {
                    raw * 2.0
                } else {
                    raw * 0.5
                }
            }
        }
    }
}

/// Sub-voxel material strategy when a block is converted to a `MicroChunk`.
/// `Geological` (the default) samples sub-voxels individually from an ore
/// noise field; `BlockAligned` makes every sub-voxel inherit the parent block.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum OreLayout {
    BlockAligned,
    #[default]
    Geological,
}

/// Salt mixed into the biome-map noise seed so it is uncorrelated with the
/// terrain surface noise at the same seed.
const BIOME_NOISE_SALT: u32 = 0x9E37_79B9;

/// 2D biome map. Given `(x, z)` it picks a biome from the list of enabled
/// biomes using a low-frequency noise field.
pub struct BiomeMap {
    noise: Perlin,
    frequency: f64,
}

impl std::fmt::Debug for BiomeMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiomeMap")
            .field("frequency", &self.frequency)
            .finish()
    }
}

impl BiomeMap {
    /// Create a biome map deterministic in `seed`. The biome map frequency is
    /// much lower than the surface frequency so each biome covers many chunks.
    pub fn new(seed: u32) -> Self {
        Self {
            noise: Perlin::new(seed.wrapping_add(BIOME_NOISE_SALT)),
            frequency: 0.005,
        }
    }

    /// Pick a biome for world position `(x, z)`. The result is a pure function
    /// of `(x, z, seed, enabled)`. `enabled` must be non-empty; if empty, this
    /// returns `BiomeType::GentleHills` as a safe fallback.
    pub fn biome_at(&self, x: i32, z: i32, enabled: &[BiomeType]) -> BiomeType {
        if enabled.is_empty() {
            return BiomeType::GentleHills;
        }
        let n = self
            .noise
            .get([x as f64 * self.frequency, z as f64 * self.frequency]);
        // Perlin returns roughly [-1.0, 1.0]; map into [0.0, 1.0).
        let t = ((n + 1.0) * 0.5).clamp(0.0, 0.999_999);
        let idx = (t * enabled.len() as f64) as usize;
        enabled[idx.min(enabled.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_biome_always_returns_that_biome() {
        let map = BiomeMap::new(42);
        let enabled = [BiomeType::Mountains];
        for x in -64..64 {
            for z in -64..64 {
                assert_eq!(map.biome_at(x, z, &enabled), BiomeType::Mountains);
            }
        }
    }

    #[test]
    fn biome_at_is_deterministic() {
        let a = BiomeMap::new(123);
        let b = BiomeMap::new(123);
        let enabled = [BiomeType::Plains, BiomeType::Mountains, BiomeType::Canyon];
        for x in (-32..32).step_by(4) {
            for z in (-32..32).step_by(4) {
                assert_eq!(
                    a.biome_at(x, z, &enabled),
                    b.biome_at(x, z, &enabled),
                    "biome at ({x}, {z}) must match across BiomeMap instances with same seed"
                );
            }
        }
    }

    #[test]
    fn empty_enabled_list_falls_back_to_gentle_hills() {
        let map = BiomeMap::new(1);
        assert_eq!(map.biome_at(0, 0, &[]), BiomeType::GentleHills);
    }

    #[test]
    fn multi_biome_covers_all_enabled_over_a_region() {
        // With a non-trivial region size and low-frequency noise, all enabled
        // biomes should appear somewhere. This guards against bugs where the
        // mapping collapses to a single index.
        let map = BiomeMap::new(7);
        let enabled = [BiomeType::Plains, BiomeType::Mountains];
        let mut saw_plains = false;
        let mut saw_mountains = false;
        for x in (-400..400).step_by(8) {
            for z in (-400..400).step_by(8) {
                match map.biome_at(x, z, &enabled) {
                    BiomeType::Plains => saw_plains = true,
                    BiomeType::Mountains => saw_mountains = true,
                    _ => {}
                }
            }
        }
        assert!(
            saw_plains && saw_mountains,
            "expected both biomes to appear in a 800x800 region"
        );
    }
}
