//! Voxel grid data structure and query methods.
//!
//! This module provides the core `VoxelGrid` data structure for storing voxel blocks
//! in a 3D grid. It handles:
//! - Block storage and retrieval
//! - Chunk-based organization
//! - Height queries
//! - Neighbor lookups
//!
//! The grid uses a HashMap for sparse storage, only allocating memory for solid blocks.

mod chunks;
mod queries;

use super::light_storage::{self, CHUNK_SIZE as LIGHT_CHUNK_SIZE, ChunkLight};
use crate::materials::MaterialType;
use glam::{IVec3, Vec3};
use std::collections::HashMap;

/// Integer vector for grid coordinates
pub type BlockPos = IVec3;

/// Resource data for mining/gathering
#[derive(Debug, Clone)]
pub struct ResourceData {
    pub resource_type: String, // "stone", "ore", "dirt", etc.
    pub quantity: u32,
}

/// Per-material lighting properties.
///
/// `emission` is the per-channel emission level `0..=15`. Each channel propagates
/// independently through the BFS with decay 1 per step, so a warm torch
/// `[15, 8, 2]` lights its R channel out to ~15 blocks, G to ~8, B to ~2 — color
/// decay falls out of the propagation for free.
///
/// `opacity_cost` is the additional decay cost when light traverses *into* this
/// material. `0` = transparent (no extra cost; air-like), `>=15` = opaque
/// (light dies at the boundary). Stage 4C only ships air-vs-opaque; finite
/// translucent costs are deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialLighting {
    pub emission: [u8; 3],
    pub opacity_cost: u8,
}

impl MaterialLighting {
    pub const NON_EMISSIVE_OPAQUE: Self = Self {
        emission: [0, 0, 0],
        opacity_cost: 15,
    };

    #[inline]
    pub fn is_emissive(&self) -> bool {
        self.emission != [0, 0, 0]
    }
}

/// Material registry - maps material IDs to actual material data
#[derive(Debug)]
pub struct MaterialRegistry {
    materials: Vec<MaterialType>,
    /// Per-material lighting properties, indexed by material ID. Parallel to
    /// `materials`. New entries default to `MaterialLighting::NON_EMISSIVE_OPAQUE`.
    lighting: Vec<MaterialLighting>,
}

impl MaterialRegistry {
    pub fn new() -> Self {
        let mut registry = MaterialRegistry {
            materials: Vec::new(),
            lighting: Vec::new(),
        };

        // Register default materials (all non-emissive opaque).
        // ID 0: Grass (green)
        registry.register(MaterialType::Lambertian {
            albedo: Vec3::new(0.3, 0.6, 0.3),
        });

        // ID 1: Dirt (brown)
        registry.register(MaterialType::Lambertian {
            albedo: Vec3::new(0.5, 0.4, 0.3),
        });

        // ID 2: Stone (gray)
        registry.register(MaterialType::Lambertian {
            albedo: Vec3::new(0.5, 0.5, 0.5),
        });

        registry
    }

    pub fn get(&self, id: u32) -> Option<&MaterialType> {
        self.materials.get(id as usize)
    }

    pub fn register(&mut self, material: MaterialType) -> u32 {
        let id = self.materials.len() as u32;
        self.materials.push(material);
        self.lighting.push(MaterialLighting::NON_EMISSIVE_OPAQUE);
        id
    }

    /// Lighting properties for a material. Falls back to opaque non-emissive
    /// for unknown ids — keeps callers branch-free at the cost of darkening
    /// stray materials, which is the safer default.
    #[inline]
    pub fn lighting(&self, id: u32) -> MaterialLighting {
        self.lighting
            .get(id as usize)
            .copied()
            .unwrap_or(MaterialLighting::NON_EMISSIVE_OPAQUE)
    }

    /// Per-channel emission level for a material id.
    #[inline]
    pub fn emission(&self, id: u32) -> [u8; 3] {
        self.lighting(id).emission
    }

    /// Opacity cost for light entering this material.
    #[inline]
    pub fn opacity_cost(&self, id: u32) -> u8 {
        self.lighting(id).opacity_cost
    }

    /// Override the lighting properties of an existing material id.
    pub fn set_lighting(&mut self, id: u32, lighting: MaterialLighting) {
        if let Some(slot) = self.lighting.get_mut(id as usize) {
            *slot = lighting;
        }
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource registry - maps resource IDs to resource data
#[derive(Debug)]
pub struct ResourceRegistry {
    resources: Vec<ResourceData>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        let mut registry = ResourceRegistry {
            resources: Vec::new(),
        };

        // Register default resources
        // ID 0: Stone
        registry.resources.push(ResourceData {
            resource_type: "stone".to_string(),
            quantity: 1,
        });

        // ID 1: Iron ore
        registry.resources.push(ResourceData {
            resource_type: "iron_ore".to_string(),
            quantity: 2,
        });

        registry
    }

    pub fn get(&self, id: u32) -> Option<&ResourceData> {
        self.resources.get(id as usize)
    }

    pub fn register(&mut self, resource: ResourceData) -> u32 {
        let id = self.resources.len() as u32;
        self.resources.push(resource);
        id
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Individual voxel block in the world
#[derive(Debug, Clone)]
pub struct VoxelBlock {
    pub position: BlockPos,
    pub material_id: u32,
    pub resource_id: Option<u32>,
}

/// Block geometry category for mesh generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockCategory {
    /// Natural terrain - uses Marching Cubes for smooth surfaces
    Smooth,
    /// Crafted/structure blocks - uses greedy meshing with sharp edges
    Blocky,
}

impl VoxelBlock {
    pub fn new(position: BlockPos, material_id: u32) -> Self {
        VoxelBlock { position, material_id, resource_id: None }
    }

    /// Determine if this block should use smooth (Marching Cubes) mesh generation.
    ///
    /// Natural materials like grass, dirt, and stone produce smooth terrain.
    /// Crafted materials like planks and bricks produce blocky structures.
    ///
    /// # Material ID Ranges
    /// - 0-99: Natural/terrain materials (smooth)
    /// - 100+: Crafted/structure materials (blocky)
    #[inline]
    pub fn is_smooth(&self) -> bool {
        self.category() == BlockCategory::Smooth
    }

    /// Get the geometry category for this block.
    ///
    /// Used to determine mesh generation strategy and AO computation method.
    #[inline]
    pub fn category(&self) -> BlockCategory {
        // Natural materials: IDs 0-99
        // Crafted materials: IDs 100+
        if self.material_id < 100 {
            BlockCategory::Smooth
        } else {
            BlockCategory::Blocky
        }
    }

    /// Convert grid position to world position (center of block)
    pub fn world_position(&self) -> Vec3 {
        Vec3::new(
            self.position.x as f32,
            self.position.y as f32,
            self.position.z as f32,
        )
    }

}

/// 3D grid storing voxel blocks
///
/// The grid uses a HashMap for sparse storage - only solid blocks are stored.
/// This is memory-efficient for worlds with lots of air/empty space.
///
/// # Examples
///
/// ```ignore
/// use moho_core::voxel::{VoxelGrid, VoxelBlock, BlockPos};
///
/// let mut grid = VoxelGrid::new(16); // 16x16x16 chunks
/// let pos = BlockPos::new(0, 0, 0);
/// let block = VoxelBlock::new(pos, 0); // material_id = 0 (grass)
/// grid.set_block(pos, block);
///
/// assert!(grid.get_block(&pos).is_some());
/// ```
#[derive(Debug)]
pub struct VoxelGrid {
    blocks: HashMap<BlockPos, VoxelBlock>,
    chunk_size: i32,
    pub material_registry: MaterialRegistry,
    pub resource_registry: ResourceRegistry,
    /// Per-chunk lighting state. Only populated for `chunk_size == 16` grids; the
    /// light system is hardcoded around 16³ chunks.
    chunk_lights: HashMap<IVec3, ChunkLight>,
}

impl VoxelGrid {
    /// Create a new empty voxel grid
    ///
    /// # Arguments
    /// * `chunk_size` - Number of blocks per chunk dimension (typically 16 or 32)
    pub fn new(chunk_size: i32) -> Self {
        VoxelGrid {
            blocks: HashMap::new(),
            chunk_size,
            material_registry: MaterialRegistry::new(),
            resource_registry: ResourceRegistry::new(),
            chunk_lights: HashMap::new(),
        }
    }

    /// Whether this grid uses the lighting system's required 16³ chunk layout.
    #[inline]
    pub fn supports_lighting(&self) -> bool {
        self.chunk_size == LIGHT_CHUNK_SIZE
    }

    /// Get the chunk size
    pub fn chunk_size(&self) -> i32 {
        self.chunk_size
    }

    /// Get iterator over all block positions
    pub fn block_positions(&self) -> impl Iterator<Item = &BlockPos> {
        self.blocks.keys()
    }

    /// Set a block at the specified position
    pub fn set_block(&mut self, pos: BlockPos, block: VoxelBlock) {
        self.blocks.insert(pos, block);
    }

    /// Get an immutable reference to a block
    pub fn get_block(&self, pos: &BlockPos) -> Option<&VoxelBlock> {
        self.blocks.get(pos)
    }

    /// Get a mutable reference to a block
    pub fn get_block_mut(&mut self, pos: &BlockPos) -> Option<&mut VoxelBlock> {
        self.blocks.get_mut(pos)
    }

    /// Remove a block from the grid
    pub fn remove_block(&mut self, pos: &BlockPos) -> Option<VoxelBlock> {
        self.blocks.remove(pos)
    }

    /// Iterate over all blocks (immutable)
    pub fn iter_blocks(&self) -> impl Iterator<Item = &VoxelBlock> {
        self.blocks.values()
    }

    /// Iterate over all blocks (mutable)
    pub fn iter_blocks_mut(&mut self) -> impl Iterator<Item = &mut VoxelBlock> {
        self.blocks.values_mut()
    }

    /// Get chunk coordinate from block position (static helper)
    ///
    /// Uses `div_euclid` for correct negative coordinate handling.
    pub fn get_chunk_pos(pos: BlockPos, chunk_size: i32) -> IVec3 {
        chunks::get_chunk_pos(pos, chunk_size)
    }

    /// Get chunk coordinate from block position (instance method)
    pub fn chunk_pos_of(&self, pos: BlockPos) -> IVec3 {
        chunks::get_chunk_pos(pos, self.chunk_size)
    }

    /// Get all blocks in a chunk
    ///
    /// Returns references to all blocks within the specified chunk boundaries.
    pub fn get_chunk_blocks(&self, chunk_pos: IVec3) -> Vec<&VoxelBlock> {
        chunks::get_chunk_blocks(&self.blocks, chunk_pos, self.chunk_size)
    }

    /// Get height at a position (highest Y with a block)
    ///
    /// Scans all blocks at the given (x, z) coordinates and returns the highest Y value.
    pub fn get_height(&self, x: i32, z: i32) -> Option<i32> {
        queries::get_height(&self.blocks, x, z)
    }

    /// Get neighbor heights for smoothing algorithm
    ///
    /// Returns heights in cardinal directions: [North, South, East, West]
    /// Used for terrain smoothing to create ramps.
    pub fn get_neighbor_heights(&self, pos: BlockPos) -> [Option<i32>; 4] {
        queries::get_neighbor_heights(&self.blocks, pos)
    }

    /// Check if a block exists at the given position
    ///
    /// Returns true if there's a solid block, false if air/empty.
    pub fn has_block_at(&self, pos: &BlockPos) -> bool {
        self.blocks.contains_key(pos)
    }

    /// Check if a block position is occupied (has a block)
    ///
    /// Convenience method for ambient occlusion calculations.
    pub fn is_block_occupied(&self, pos: BlockPos) -> bool {
        self.blocks.contains_key(&pos)
    }

    /// Get the number of blocks in the grid
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    // ---------------------------------------------------------------------
    // Accessor API (Stage 4B)
    //
    // These methods return primitive values rather than `&VoxelBlock`, so
    // callers do not depend on `VoxelBlock` being the storage type. Stage 4D
    // can swap the underlying storage to a paletted layout without API churn.
    // ---------------------------------------------------------------------

    /// Material id at `pos`, or `None` for air.
    #[inline]
    pub fn material_at(&self, pos: BlockPos) -> Option<u32> {
        self.blocks.get(&pos).map(|b| b.material_id)
    }

    /// Resource id at `pos`, or `None` if the block has no resource (or is air).
    #[inline]
    pub fn resource_at(&self, pos: BlockPos) -> Option<u32> {
        self.blocks.get(&pos).and_then(|b| b.resource_id)
    }

    /// Whether `pos` contains a solid (stored) block.
    #[inline]
    pub fn is_solid_at(&self, pos: BlockPos) -> bool {
        self.blocks.contains_key(&pos)
    }

    /// Geometry category at `pos`, or `None` for air.
    #[inline]
    pub fn is_smooth_at(&self, pos: BlockPos) -> Option<bool> {
        self.blocks.get(&pos).map(|b| b.is_smooth())
    }

    /// Snapshot of all block state at `pos`, or `None` for air.
    #[inline]
    pub fn block_data_at(&self, pos: BlockPos) -> Option<BlockData> {
        self.blocks.get(&pos).map(BlockData::from_block)
    }

    /// Place a block at `pos`, replacing any existing block. Light levels are
    /// initialized to 0; lighting is computed separately by `LightSystem`.
    pub fn place_block(&mut self, pos: BlockPos, material_id: u32, resource_id: Option<u32>) {
        self.blocks
            .insert(pos, VoxelBlock { position: pos, material_id, resource_id });
        self.note_block_change(pos, true);
    }

    /// Remove the block at `pos`. Returns `true` if a block was removed.
    pub fn clear_block(&mut self, pos: BlockPos) -> bool {
        let removed = self.blocks.remove(&pos).is_some();
        if removed {
            self.note_block_change(pos, false);
        }
        removed
    }

    /// Mark lighting state dirty in the chunk(s) containing `pos`. Called from
    /// `place_block`/`clear_block`. The 4C light system consumes these flags to
    /// schedule recomputes.
    fn note_block_change(&mut self, pos: BlockPos, placed: bool) {
        if !self.supports_lighting() {
            return;
        }
        let (chunk_pos, _idx, local) = light_storage::world_to_chunk_local(pos);
        let sky_became_dirty = {
            let cl = self
                .chunk_lights
                .entry(chunk_pos)
                .or_insert_with(ChunkLight::new);
            let before = cl.sky_dirty;
            if placed {
                // Conservative: any opaque placement could raise the column. Material
                // opacity is consulted by the rebuild pass; for the dirty trigger we
                // only need an upper bound.
                cl.note_opaque_placed(local.x, local.y, local.z);
            } else {
                // A removal at the column max requires a full column rescan, which the
                // sky-exposure pass owns. Just flag dirty here.
                let cur = cl.column_max_y(local.x, local.z);
                if (local.y as i8) >= cur {
                    cl.sky_dirty = true;
                }
            }
            cl.light_dirty = true;
            cl.sky_dirty && !before
        };

        // A column-height change in this chunk invalidates sky exposure for every
        // loaded chunk below it in the same (cx, cz) stack.
        if sky_became_dirty {
            let lower: Vec<IVec3> = self
                .chunk_lights
                .keys()
                .filter(|cp| cp.x == chunk_pos.x && cp.z == chunk_pos.z && cp.y < chunk_pos.y)
                .copied()
                .collect();
            for cp in lower {
                self.chunk_lights.get_mut(&cp).unwrap().sky_dirty = true;
            }
        }
    }

    /// Iterate over snapshots of every stored block.
    pub fn iter_block_data(&self) -> impl Iterator<Item = BlockData> + '_ {
        self.blocks.values().map(BlockData::from_block)
    }

    /// Snapshots of every stored block within a chunk.
    pub fn chunk_block_data(&self, chunk_pos: IVec3) -> Vec<BlockData> {
        chunks::get_chunk_blocks(&self.blocks, chunk_pos, self.chunk_size)
            .into_iter()
            .map(BlockData::from_block)
            .collect()
    }

    // ---------------------------------------------------------------------
    // Stage 4C: per-chunk lighting accessors.
    //
    // `sky_exposed_at` reports whether a voxel can see the sky (binary;
    // direct-sun shadow is CSM's job). `block_light_rgb_at` returns RGB
    // block-light, with each channel `0..=15`. Air outside any allocated
    // chunk reports as sky-exposed = true and block-light = [0, 0, 0].
    // ---------------------------------------------------------------------

    /// Whether `pos` is exposed to sky (no opaque voxels at or above in its column).
    pub fn sky_exposed_at(&self, pos: BlockPos) -> bool {
        if !self.supports_lighting() {
            return true;
        }
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        match self.chunk_lights.get(&chunk_pos) {
            Some(cl) => cl.sky_exposed_at(idx),
            None => true, // unallocated chunk → treat as open air
        }
    }

    /// RGB block-light at `pos`. Each channel `0..=15`. Defaults to `[0, 0, 0]`.
    pub fn block_light_rgb_at(&self, pos: BlockPos) -> [u8; 3] {
        if !self.supports_lighting() {
            return [0, 0, 0];
        }
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        match self.chunk_lights.get(&chunk_pos) {
            Some(cl) => cl.block_light_rgb(idx),
            None => [0, 0, 0],
        }
    }

    /// Set the RGB block-light at `pos`. Used by the propagator. Allocates a
    /// chunk-light entry if needed.
    pub fn set_block_light_rgb(&mut self, pos: BlockPos, rgb: [u8; 3]) {
        if !self.supports_lighting() {
            return;
        }
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        let cl = self
            .chunk_lights
            .entry(chunk_pos)
            .or_insert_with(ChunkLight::new);
        cl.set_block_light_rgb(idx, rgb);
    }

    /// Set the sky-exposed bit at `pos`. Used by the sky-exposure pass.
    pub fn set_sky_exposed(&mut self, pos: BlockPos, value: bool) {
        if !self.supports_lighting() {
            return;
        }
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        let cl = self
            .chunk_lights
            .entry(chunk_pos)
            .or_insert_with(ChunkLight::new);
        cl.set_sky_exposed(idx, value);
    }

    /// Borrow a chunk's lighting state, if any. Returns `None` if the chunk has
    /// never been touched by lighting writes.
    pub fn chunk_light(&self, chunk_pos: IVec3) -> Option<&ChunkLight> {
        self.chunk_lights.get(&chunk_pos)
    }

    /// Mutable borrow of a chunk's lighting state, allocating an empty entry if
    /// needed.
    pub fn chunk_light_mut(&mut self, chunk_pos: IVec3) -> &mut ChunkLight {
        self.chunk_lights
            .entry(chunk_pos)
            .or_insert_with(ChunkLight::new)
    }

    /// Iterate all chunk positions with allocated light state.
    pub fn lit_chunk_positions(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.chunk_lights.keys().copied()
    }
}

/// Compact, copyable snapshot of a block's stored state.
///
/// Returned by `VoxelGrid` accessors. Decouples callers from the underlying
/// storage type so paletted storage (Stage 4D) can drop in without API churn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockData {
    pub position: BlockPos,
    pub material_id: u32,
    pub resource_id: Option<u32>,
}

impl BlockData {
    fn from_block(block: &VoxelBlock) -> Self {
        Self {
            position: block.position,
            material_id: block.material_id,
            resource_id: block.resource_id,
        }
    }

    /// Geometry category — same threshold as `VoxelBlock::is_smooth`.
    #[inline]
    pub fn is_smooth(&self) -> bool {
        self.material_id < 100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_grid() {
        let grid = VoxelGrid::new(16);
        assert_eq!(grid.chunk_size(), 16);
        assert_eq!(grid.block_count(), 0);
    }

    #[test]
    fn test_set_and_get_block() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);
        let block = VoxelBlock::new(pos, 0);

        grid.set_block(pos, block);

        assert!(grid.get_block(&pos).is_some());
        assert_eq!(grid.get_block(&pos).unwrap().material_id, 0);
        assert_eq!(grid.block_count(), 1);
    }

    #[test]
    fn test_remove_block() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(1, 2, 3);
        let block = VoxelBlock::new(pos, 1);

        grid.set_block(pos, block);
        assert_eq!(grid.block_count(), 1);

        let removed = grid.remove_block(&pos);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().material_id, 1);
        assert_eq!(grid.block_count(), 0);
    }

    #[test]
    fn test_has_block_at() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(5, 5, 5);

        assert!(!grid.has_block_at(&pos));

        grid.set_block(pos, VoxelBlock::new(pos, 0));

        assert!(grid.has_block_at(&pos));
    }

    #[test]
    fn test_block_world_position() {
        let block = VoxelBlock::new(BlockPos::new(5, 10, 15), 0);
        let world_pos = block.world_position();

        assert_eq!(world_pos, Vec3::new(5.0, 10.0, 15.0));
    }

    #[test]
    fn test_material_registry() {
        let mut registry = MaterialRegistry::new();

        // Default materials should exist
        assert!(registry.get(0).is_some()); // Grass
        assert!(registry.get(1).is_some()); // Dirt
        assert!(registry.get(2).is_some()); // Stone

        // Register new material
        let new_id = registry.register(MaterialType::Lambertian {
            albedo: Vec3::new(1.0, 0.0, 0.0),
        });
        assert_eq!(new_id, 3);
        assert!(registry.get(3).is_some());
    }

    #[test]
    fn test_resource_registry() {
        let mut registry = ResourceRegistry::new();

        // Default resources should exist
        assert!(registry.get(0).is_some()); // Stone
        assert!(registry.get(1).is_some()); // Iron ore

        // Register new resource
        let new_id = registry.register(ResourceData {
            resource_type: "gold_ore".to_string(),
            quantity: 3,
        });
        assert_eq!(new_id, 2);
        assert!(registry.get(2).is_some());
    }

    #[test]
    fn test_block_category_natural_materials() {
        // Natural materials (IDs 0-99) should be smooth
        let grass_block = VoxelBlock::new(BlockPos::new(0, 0, 0), 0);
        assert!(grass_block.is_smooth());
        assert_eq!(grass_block.category(), BlockCategory::Smooth);

        let dirt_block = VoxelBlock::new(BlockPos::new(0, 0, 0), 1);
        assert!(dirt_block.is_smooth());
        assert_eq!(dirt_block.category(), BlockCategory::Smooth);

        let stone_block = VoxelBlock::new(BlockPos::new(0, 0, 0), 2);
        assert!(stone_block.is_smooth());
        assert_eq!(stone_block.category(), BlockCategory::Smooth);

        // Edge of natural range
        let block_99 = VoxelBlock::new(BlockPos::new(0, 0, 0), 99);
        assert!(block_99.is_smooth());
        assert_eq!(block_99.category(), BlockCategory::Smooth);
    }

    #[test]
    fn test_block_category_crafted_materials() {
        // Crafted materials (IDs 100+) should be blocky
        let planks_block = VoxelBlock::new(BlockPos::new(0, 0, 0), 100);
        assert!(!planks_block.is_smooth());
        assert_eq!(planks_block.category(), BlockCategory::Blocky);

        let bricks_block = VoxelBlock::new(BlockPos::new(0, 0, 0), 101);
        assert!(!bricks_block.is_smooth());
        assert_eq!(bricks_block.category(), BlockCategory::Blocky);

        // High ID crafted material
        let metal_block = VoxelBlock::new(BlockPos::new(0, 0, 0), 500);
        assert!(!metal_block.is_smooth());
        assert_eq!(metal_block.category(), BlockCategory::Blocky);
    }

    #[test]
    fn test_block_category_boundary() {
        // Test the boundary between smooth and blocky (99 vs 100)
        let last_smooth = VoxelBlock::new(BlockPos::new(0, 0, 0), 99);
        let first_blocky = VoxelBlock::new(BlockPos::new(0, 0, 0), 100);

        assert!(last_smooth.is_smooth());
        assert!(!first_blocky.is_smooth());
        assert_eq!(last_smooth.category(), BlockCategory::Smooth);
        assert_eq!(first_blocky.category(), BlockCategory::Blocky);
    }
}
