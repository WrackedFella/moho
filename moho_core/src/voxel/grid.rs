//! Voxel grid data structure and query methods.
//!
//! This module provides the core `VoxelGrid` data structure for storing voxel blocks
//! in a 3D grid. It handles:
//! - Block storage and retrieval
//! - Chunk-based organization
//! - Height queries
//! - Neighbor lookups
//!
//! The grid uses a HashMap for sparse storage, only allocating memory for non-air blocks.

mod chunks;
mod queries;

use crate::materials::MaterialType;
use glam::{IVec3, Vec3};
use std::collections::HashMap;

/// Integer vector for grid coordinates
pub type BlockPos = IVec3;

/// Mesh data for a single voxel block
#[derive(Debug, Clone)]
pub struct VoxelMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Ambient occlusion values per vertex (0.0 = fully occluded, 1.0 = no occlusion)
    pub ambient_occlusion: Vec<f32>,
    /// Geometry type per vertex (0 = smooth terrain, 1 = blocky structure)
    pub geometry_type: Vec<u32>,
}

impl VoxelMesh {
    pub fn empty() -> Self {
        VoxelMesh {
            vertices: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            ambient_occlusion: Vec::new(),
            geometry_type: Vec::new(),
        }
    }
}

/// Resource data for mining/gathering
#[derive(Debug, Clone)]
pub struct ResourceData {
    pub resource_type: String, // "stone", "ore", "dirt", etc.
    pub quantity: u32,
}

/// Material registry - maps material IDs to actual material data
pub struct MaterialRegistry {
    materials: Vec<MaterialType>,
}

impl MaterialRegistry {
    pub fn new() -> Self {
        let mut registry = MaterialRegistry {
            materials: Vec::new(),
        };

        // Register default materials
        // ID 0: Grass (green)
        registry.materials.push(MaterialType::Lambertian {
            albedo: Vec3::new(0.3, 0.6, 0.3),
        });

        // ID 1: Dirt (brown)
        registry.materials.push(MaterialType::Lambertian {
            albedo: Vec3::new(0.5, 0.4, 0.3),
        });

        // ID 2: Stone (gray)
        registry.materials.push(MaterialType::Lambertian {
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
        id
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource registry - maps resource IDs to resource data
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
/// All blocks are treated uniformly - material/resource data stored separately
#[derive(Debug, Clone)]
pub struct VoxelBlock {
    pub position: BlockPos,
    pub mesh_data: VoxelMesh,
    pub material_id: u32,         // Index into MaterialRegistry
    pub resource_id: Option<u32>, // Index into ResourceRegistry
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
        VoxelBlock {
            position,
            mesh_data: VoxelMesh::empty(),
            material_id,
            resource_id: None,
        }
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
pub struct VoxelGrid {
    blocks: HashMap<BlockPos, VoxelBlock>,
    chunk_size: i32,
    pub material_registry: MaterialRegistry,
    pub resource_registry: ResourceRegistry,
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
        }
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
