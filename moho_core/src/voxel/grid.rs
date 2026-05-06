//! Voxel grid data structure and query methods.
//!
//! The grid uses a `HashMap<ChunkPos, PalettedChunk>` for sparse storage.
//! Air chunks are absent from the map (zero memory cost). Block accessors
//! return primitive values rather than `&VoxelBlock` references so the storage
//! type is an internal detail callers do not depend on.

mod paletted;

use self::paletted::PalettedChunk;
use super::light_storage::{self, CHUNK_SIZE as LIGHT_CHUNK_SIZE, CHUNK_USIZE, ChunkLight};
use crate::materials::MaterialType;
use glam::{IVec3, Vec3};
use std::collections::HashMap;

/// Integer vector for grid coordinates
pub type BlockPos = IVec3;

/// Chunk size constant used throughout this module.
const CHUNK_SIZE_I32: i32 = LIGHT_CHUNK_SIZE;

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

/// Individual voxel block — kept as a standalone value type for callers that
/// construct block descriptors outside the grid. The grid itself stores
/// paletted chunks rather than `VoxelBlock` instances.
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

    /// Whether this block uses smooth (Marching Cubes) mesh generation.
    ///
    /// Material IDs 0–99 = natural/terrain (smooth).
    /// Material IDs 100+ = crafted/structure (blocky).
    #[inline]
    pub fn is_smooth(&self) -> bool {
        self.category() == BlockCategory::Smooth
    }

    #[inline]
    pub fn category(&self) -> BlockCategory {
        if self.material_id < 100 {
            BlockCategory::Smooth
        } else {
            BlockCategory::Blocky
        }
    }

    pub fn world_position(&self) -> Vec3 {
        Vec3::new(
            self.position.x as f32,
            self.position.y as f32,
            self.position.z as f32,
        )
    }
}

/// 3D grid storing voxel blocks using paletted chunk storage.
///
/// Blocks are organized into 16³ chunks. Air chunks are absent from the map
/// (zero memory overhead). The public accessor API is stable; internal storage
/// is an implementation detail.
///
/// # Examples
///
/// ```ignore
/// use moho_core::voxel::{VoxelGrid, BlockPos};
///
/// let mut grid = VoxelGrid::new(16);
/// let pos = BlockPos::new(0, 0, 0);
/// grid.place_block(pos, 0, None); // material_id = 0 (grass)
///
/// assert!(grid.is_solid_at(pos));
/// ```
#[derive(Debug)]
pub struct VoxelGrid {
    /// Paletted block storage, keyed by chunk coordinate.
    chunks: HashMap<IVec3, PalettedChunk>,
    chunk_size: i32,
    pub material_registry: MaterialRegistry,
    pub resource_registry: ResourceRegistry,
    /// Per-chunk lighting state (Stage 4C). Only populated for chunk_size == 16.
    chunk_lights: HashMap<IVec3, ChunkLight>,
}

impl VoxelGrid {
    /// Create a new empty voxel grid.
    ///
    /// `chunk_size` must be 16; the paletted storage and lighting system are
    /// both hardcoded to 16³ chunks.
    pub fn new(chunk_size: i32) -> Self {
        debug_assert_eq!(chunk_size, CHUNK_SIZE_I32, "VoxelGrid requires chunk_size == 16");
        VoxelGrid {
            chunks: HashMap::new(),
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

    /// Get the chunk size (always 16 after Stage 4D).
    pub fn chunk_size(&self) -> i32 {
        self.chunk_size
    }

    /// Static helper: chunk coordinate from a block position and an arbitrary chunk size.
    pub fn get_chunk_pos(pos: BlockPos, chunk_size: i32) -> IVec3 {
        IVec3::new(
            pos.x.div_euclid(chunk_size),
            pos.y.div_euclid(chunk_size),
            pos.z.div_euclid(chunk_size),
        )
    }

    /// Chunk coordinate containing `pos`.
    pub fn chunk_pos_of(&self, pos: BlockPos) -> IVec3 {
        Self::get_chunk_pos(pos, self.chunk_size)
    }

    // -------------------------------------------------------------------------
    // Accessor API (Stage 4B / 4D)
    // -------------------------------------------------------------------------

    /// Material id at `pos`, or `None` for air.
    #[inline]
    pub fn material_at(&self, pos: BlockPos) -> Option<u32> {
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        self.chunks.get(&chunk_pos)?.material_at(idx)
    }

    /// Resource id at `pos`, or `None` if the block has no resource (or is air).
    #[inline]
    pub fn resource_at(&self, pos: BlockPos) -> Option<u32> {
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        self.chunks.get(&chunk_pos)?.resource_at(idx)
    }

    /// Whether `pos` contains a solid (non-air) block.
    #[inline]
    pub fn is_solid_at(&self, pos: BlockPos) -> bool {
        self.material_at(pos).is_some()
    }

    /// Geometry category at `pos`: `Some(true)` = smooth terrain,
    /// `Some(false)` = blocky structure, `None` = air.
    #[inline]
    pub fn is_smooth_at(&self, pos: BlockPos) -> Option<bool> {
        self.material_at(pos).map(|m| m < 100)
    }

    /// Snapshot of all block state at `pos`, or `None` for air.
    #[inline]
    pub fn block_data_at(&self, pos: BlockPos) -> Option<BlockData> {
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        let chunk = self.chunks.get(&chunk_pos)?;
        let material_id = chunk.material_at(idx)?;
        Some(BlockData { position: pos, material_id, resource_id: chunk.resource_at(idx) })
    }

    /// Place a block at world position `pos`, replacing any existing block.
    ///
    /// Creates the containing 16³ chunk on first use (absent key == all-air). The
    /// block is written directly into the paletted storage; no validation or bounds
    /// check is performed — all integer coordinates are valid.
    ///
    /// # Side effects
    ///
    /// - Sets `chunk.mesh_dirty = true` on the chunk that contains `pos`.
    /// - Sets `chunk.light_dirty = true` on that same chunk.
    /// - Calls `note_block_change(pos, true)`, which updates `ChunkLight` sky-column
    ///   tracking and may propagate `sky_dirty = true` downward to chunks below.
    ///
    /// The dirty flags are consumed asynchronously:
    /// - `mesh_dirty` is read by the meshing pipeline during chunk mesh generation.
    /// - `light_dirty` / `sky_dirty` are read by `LightSystem::emit_dirty_events()`,
    ///   which publishes `WorldEvent::LightChunkDirty` events for `EventProcessor`.
    ///
    /// # What this does NOT do
    ///
    /// - No physics, collision, or gameplay validation.
    /// - No event publishing — callers that need `BlockPlaced` events must use
    ///   [`BlockModifier::set_block`] (or the forthcoming [`VoxelMutator`]).
    /// - No neighbor-chunk invalidation for edge blocks — `BlockModifier` handles that.
    ///
    /// This is the **lowest-level** mutation primitive. Prefer [`BlockModifier`] for
    /// gameplay code, and [`VoxelMutator`] (via [`VoxelGrid::mutator`]) for bulk
    /// terrain generation where direct grid access is intentional.
    pub fn place_block(&mut self, pos: BlockPos, material_id: u32, resource_id: Option<u32>) {
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        chunk.set_block(idx, material_id, resource_id);
        chunk.mesh_dirty = true;
        chunk.light_dirty = true;
        self.note_block_change(pos, true);
    }

    /// Remove the block at `pos`. Returns `true` if a block was removed.
    pub fn clear_block(&mut self, pos: BlockPos) -> bool {
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        let removed = if let Some(chunk) = self.chunks.get_mut(&chunk_pos) {
            let r = chunk.clear_block(idx);
            if r {
                chunk.mesh_dirty = true;
                chunk.light_dirty = true;
            }
            r
        } else {
            false
        };
        if removed {
            self.note_block_change(pos, false);
        }
        removed
    }

    /// Whether a block exists at `pos`.
    #[inline]
    pub fn has_block_at(&self, pos: &BlockPos) -> bool {
        self.is_solid_at(*pos)
    }

    /// Whether a block position is occupied (alias for `is_solid_at`).
    #[inline]
    pub fn is_block_occupied(&self, pos: BlockPos) -> bool {
        self.is_solid_at(pos)
    }

    /// Total number of non-air blocks across all chunks.
    pub fn block_count(&self) -> usize {
        self.chunks.values().map(|c| c.block_count()).sum()
    }

    /// Iterator over world positions of all non-air blocks.
    pub fn block_positions(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.chunks.iter().flat_map(|(&chunk_pos, chunk)| {
            let base = chunk_pos * CHUNK_SIZE_I32;
            chunk.iter_blocks().map(move |(idx, _mat, _res)| base + idx_to_local(idx))
        })
    }

    /// Iterate over snapshots of every stored block.
    pub fn iter_block_data(&self) -> impl Iterator<Item = BlockData> + '_ {
        self.chunks.iter().flat_map(|(&chunk_pos, chunk)| {
            let base = chunk_pos * CHUNK_SIZE_I32;
            chunk.iter_blocks().map(move |(idx, material_id, resource_id)| BlockData {
                position: base + idx_to_local(idx),
                material_id,
                resource_id,
            })
        })
    }

    /// Snapshots of every stored block within a chunk.
    pub fn chunk_block_data(&self, chunk_pos: IVec3) -> Vec<BlockData> {
        let Some(chunk) = self.chunks.get(&chunk_pos) else {
            return vec![];
        };
        let base = chunk_pos * CHUNK_SIZE_I32;
        chunk
            .iter_blocks()
            .map(|(idx, material_id, resource_id)| BlockData {
                position: base + idx_to_local(idx),
                material_id,
                resource_id,
            })
            .collect()
    }

    /// Remove a chunk and its associated lighting state from the grid.
    ///
    /// Called by the streaming system when a chunk moves beyond the unload radius.
    pub fn remove_chunk(&mut self, pos: IVec3) {
        self.chunks.remove(&pos);
        self.chunk_lights.remove(&pos);
    }

    /// Iterate over the positions of all currently-loaded chunks.
    ///
    /// Includes any chunk that has at least one block; all-air chunks are never
    /// stored (absent key == all air).
    pub fn chunk_positions(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.chunks.keys().copied()
    }

    /// Returns `true` if a non-empty chunk exists at `pos`.
    pub fn has_chunk(&self, pos: IVec3) -> bool {
        self.chunks.contains_key(&pos)
    }

    /// Serialize the chunk at `pos` to bytes (palette + indices + resources).
    /// Returns `None` if the chunk is absent (all-air) or unmodified.
    ///
    /// Used by the streaming system to persist modified chunks on eviction.
    pub fn serialize_chunk(&self, pos: IVec3) -> Option<Vec<u8>> {
        let chunk = self.chunks.get(&pos)?;
        if !chunk.is_modified() {
            return None;
        }
        Some(chunk.to_bytes())
    }

    /// Deserialize a chunk from `data` (produced by `serialize_chunk`) and insert
    /// it at `pos`, replacing any existing chunk. Sets `mesh_dirty` and
    /// `light_dirty` so the lighting/meshing pipeline picks it up on the next frame.
    ///
    /// Returns `false` if `data` is malformed.
    pub fn deserialize_chunk_into(&mut self, pos: IVec3, data: &[u8]) -> bool {
        match PalettedChunk::from_bytes(data) {
            Some(chunk) => {
                self.chunks.insert(pos, chunk);
                // Ensure a ChunkLight entry exists for the lighting pipeline.
                self.chunk_lights.entry(pos).or_default().light_dirty = true;
                true
            }
            None => false,
        }
    }

    /// Whether the chunk at `pos` has been modified since the last save.
    pub fn chunk_is_modified(&self, pos: IVec3) -> bool {
        self.chunks.get(&pos).is_some_and(|c| c.is_modified())
    }

    /// Clear the modified flag on the chunk at `pos`.
    ///
    /// Called after the streaming system generates a chunk from seed (freshly
    /// generated chunks must not be re-saved on eviction — only player edits need
    /// to be persisted).
    pub fn clear_chunk_modified(&mut self, pos: IVec3) {
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.clear_modified();
        }
    }

    /// Highest Y containing a solid block at column `(x, z)`, or `None`.
    pub fn get_height(&self, x: i32, z: i32) -> Option<i32> {
        let cx = x.div_euclid(CHUNK_SIZE_I32);
        let cz = z.div_euclid(CHUNK_SIZE_I32);
        let lx = x.rem_euclid(CHUNK_SIZE_I32);
        let lz = z.rem_euclid(CHUNK_SIZE_I32);

        let mut max_y: Option<i32> = None;
        for (&chunk_pos, chunk) in &self.chunks {
            if chunk_pos.x != cx || chunk_pos.z != cz {
                continue;
            }
            let base_y = chunk_pos.y * CHUNK_SIZE_I32;
            // Scan the column top-to-bottom within this chunk.
            for ly in (0..CHUNK_SIZE_I32).rev() {
                let idx = light_storage::local_index(lx, ly, lz);
                if chunk.material_at(idx).is_some() {
                    let wy = base_y + ly;
                    max_y = Some(max_y.map_or(wy, |m: i32| m.max(wy)));
                    break;
                }
            }
        }
        max_y
    }

    /// Heights of the four cardinal neighbors of `pos`: [North(+Z), South(-Z), East(+X), West(-X)].
    pub fn get_neighbor_heights(&self, pos: BlockPos) -> [Option<i32>; 4] {
        [
            self.get_height(pos.x, pos.z + 1),
            self.get_height(pos.x, pos.z - 1),
            self.get_height(pos.x + 1, pos.z),
            self.get_height(pos.x - 1, pos.z),
        ]
    }

    /// Obtain a [`VoxelMutator`] for semantically-named block operations.
    ///
    /// Prefer this over calling `place_block` / `clear_block` directly when the
    /// intent should be expressed in domain terms (`place`, `remove`, `fill_region`)
    /// rather than raw grid plumbing.
    pub fn mutator(&mut self) -> super::modification::VoxelMutator<'_> {
        super::modification::VoxelMutator::new(self)
    }

    // -------------------------------------------------------------------------
    // Internal dirty-flag bookkeeping (called by place_block / clear_block).
    // -------------------------------------------------------------------------

    fn note_block_change(&mut self, pos: BlockPos, placed: bool) {
        if !self.supports_lighting() {
            return;
        }
        let (chunk_pos, _idx, local) = light_storage::world_to_chunk_local(pos);
        let sky_became_dirty = {
            let cl = self
                .chunk_lights
                .entry(chunk_pos)
                .or_default();
            let before = cl.sky_dirty;
            if placed {
                cl.note_opaque_placed(local.x, local.y, local.z);
            } else {
                let cur = cl.column_max_y(local.x, local.z);
                if (local.y as i8) >= cur {
                    cl.sky_dirty = true;
                }
            }
            cl.light_dirty = true;
            cl.sky_dirty && !before
        };

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

    // -------------------------------------------------------------------------
    // Stage 4C: per-chunk lighting accessors.
    // -------------------------------------------------------------------------

    /// Whether `pos` is exposed to sky.
    pub fn sky_exposed_at(&self, pos: BlockPos) -> bool {
        if !self.supports_lighting() {
            return true;
        }
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        match self.chunk_lights.get(&chunk_pos) {
            Some(cl) => cl.sky_exposed_at(idx),
            None => true,
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

    /// Set the RGB block-light at `pos`. Allocates a chunk-light entry if needed.
    pub fn set_block_light_rgb(&mut self, pos: BlockPos, rgb: [u8; 3]) {
        if !self.supports_lighting() {
            return;
        }
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        let cl = self
            .chunk_lights
            .entry(chunk_pos)
            .or_default();
        cl.set_block_light_rgb(idx, rgb);
    }

    /// Set the sky-exposed bit at `pos`.
    pub fn set_sky_exposed(&mut self, pos: BlockPos, value: bool) {
        if !self.supports_lighting() {
            return;
        }
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        let cl = self
            .chunk_lights
            .entry(chunk_pos)
            .or_default();
        cl.set_sky_exposed(idx, value);
    }

    /// Borrow a chunk's lighting state, if any.
    pub fn chunk_light(&self, chunk_pos: IVec3) -> Option<&ChunkLight> {
        self.chunk_lights.get(&chunk_pos)
    }

    /// Mutable borrow of a chunk's lighting state, allocating if needed.
    pub fn chunk_light_mut(&mut self, chunk_pos: IVec3) -> &mut ChunkLight {
        self.chunk_lights
            .entry(chunk_pos)
            .or_default()
    }

    /// Iterate all chunk positions with allocated light state.
    pub fn lit_chunk_positions(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.chunk_lights.keys().copied()
    }
}

// -------------------------------------------------------------------------
// Private helpers
// -------------------------------------------------------------------------

/// Reconstruct chunk-local (x, y, z) from a dense linear index.
/// Index layout: `idx = x + y*16 + z*256`.
#[inline]
fn idx_to_local(idx: usize) -> IVec3 {
    IVec3::new(
        (idx % CHUNK_USIZE) as i32,
        ((idx / CHUNK_USIZE) % CHUNK_USIZE) as i32,
        (idx / (CHUNK_USIZE * CHUNK_USIZE)) as i32,
    )
}

/// Compact, copyable snapshot of a block's stored state.
///
/// Returned by `VoxelGrid` accessors. Decouples callers from internal storage
/// so paletted storage swaps do not cause API churn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockData {
    pub position: BlockPos,
    pub material_id: u32,
    pub resource_id: Option<u32>,
}

impl BlockData {
    /// Geometry category — smooth iff `material_id < 100`.
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
    fn test_place_and_query() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);

        grid.place_block(pos, 0, None);

        assert!(grid.is_solid_at(pos));
        assert_eq!(grid.material_at(pos), Some(0));
        assert_eq!(grid.block_count(), 1);
    }

    #[test]
    fn test_clear_block() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(1, 2, 3);

        grid.place_block(pos, 1, None);
        assert_eq!(grid.block_count(), 1);

        assert!(grid.clear_block(pos));
        assert_eq!(grid.material_at(pos), None);
        assert_eq!(grid.block_count(), 0);
    }

    #[test]
    fn test_has_block_at() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(5, 5, 5);

        assert!(!grid.has_block_at(&pos));
        grid.place_block(pos, 0, None);
        assert!(grid.has_block_at(&pos));
    }

    #[test]
    fn test_material_registry() {
        let mut registry = MaterialRegistry::new();

        assert!(registry.get(0).is_some()); // Grass
        assert!(registry.get(1).is_some()); // Dirt
        assert!(registry.get(2).is_some()); // Stone

        let new_id = registry.register(MaterialType::Lambertian {
            albedo: Vec3::new(1.0, 0.0, 0.0),
        });
        assert_eq!(new_id, 3);
        assert!(registry.get(3).is_some());
    }

    #[test]
    fn test_resource_registry() {
        let mut registry = ResourceRegistry::new();

        assert!(registry.get(0).is_some()); // Stone
        assert!(registry.get(1).is_some()); // Iron ore

        let new_id = registry.register(ResourceData {
            resource_type: "gold_ore".to_string(),
            quantity: 3,
        });
        assert_eq!(new_id, 2);
        assert!(registry.get(2).is_some());
    }

    #[test]
    fn test_block_category_smooth() {
        let b = VoxelBlock::new(BlockPos::new(0, 0, 0), 0);
        assert!(b.is_smooth());
        assert_eq!(b.category(), BlockCategory::Smooth);

        let b99 = VoxelBlock::new(BlockPos::new(0, 0, 0), 99);
        assert!(b99.is_smooth());
    }

    #[test]
    fn test_block_category_blocky() {
        let b = VoxelBlock::new(BlockPos::new(0, 0, 0), 100);
        assert!(!b.is_smooth());
        assert_eq!(b.category(), BlockCategory::Blocky);
    }

    #[test]
    fn test_is_smooth_at() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);

        assert_eq!(grid.is_smooth_at(pos), None);
        grid.place_block(pos, 0, None); // smooth
        assert_eq!(grid.is_smooth_at(pos), Some(true));
        grid.place_block(pos, 100, None); // blocky
        assert_eq!(grid.is_smooth_at(pos), Some(false));
    }

    #[test]
    fn test_block_positions_count() {
        let mut grid = VoxelGrid::new(16);
        assert_eq!(grid.block_positions().count(), 0);

        grid.place_block(IVec3::new(0, 0, 0), 0, None);
        grid.place_block(IVec3::new(1, 0, 0), 0, None);
        assert_eq!(grid.block_positions().count(), 2);
    }

    #[test]
    fn test_block_data_at() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(3, 4, 5);
        assert!(grid.block_data_at(pos).is_none());

        grid.place_block(pos, 2, Some(1));
        let bd = grid.block_data_at(pos).unwrap();
        assert_eq!(bd.position, pos);
        assert_eq!(bd.material_id, 2);
        assert_eq!(bd.resource_id, Some(1));
    }

    #[test]
    fn test_get_height() {
        let mut grid = VoxelGrid::new(16);
        assert_eq!(grid.get_height(0, 0), None);

        grid.place_block(IVec3::new(0, 2, 0), 0, None);
        grid.place_block(IVec3::new(0, 5, 0), 0, None);
        assert_eq!(grid.get_height(0, 0), Some(5));
        assert_eq!(grid.get_height(1, 0), None);
    }

    #[test]
    fn test_iter_block_data() {
        let mut grid = VoxelGrid::new(16);
        grid.place_block(IVec3::new(0, 0, 0), 1, None);
        grid.place_block(IVec3::new(1, 0, 0), 2, None);

        let data: Vec<_> = grid.iter_block_data().collect();
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_chunk_block_data() {
        let mut grid = VoxelGrid::new(16);
        grid.place_block(IVec3::new(0, 0, 0), 1, None);
        grid.place_block(IVec3::new(20, 0, 0), 2, None); // different chunk

        let in_chunk = grid.chunk_block_data(IVec3::ZERO);
        assert_eq!(in_chunk.len(), 1);
        assert_eq!(in_chunk[0].material_id, 1);
    }
}
