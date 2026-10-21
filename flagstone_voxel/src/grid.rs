//! Voxel grid data structure and query methods.
//!
//! The grid uses a `HashMap<ChunkPos, PalettedChunk>` for sparse storage.
//! Air chunks are absent from the map (zero memory cost). Block accessors
//! return primitive values rather than references so the storage type is an
//! internal detail callers do not depend on.

mod paletted;

pub use self::paletted::CHUNK_FILE;
use self::paletted::PalettedChunk;
use super::light_storage::{self, CHUNK_SIZE as LIGHT_CHUNK_SIZE, ChunkLight, idx_to_local};
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

    pub fn register(&mut self, material: MaterialType) -> u32 {
        let id = self.materials.len() as u32;
        self.materials.push(material);
        self.lighting.push(MaterialLighting::NON_EMISSIVE_OPAQUE);
        id
    }

    /// The material registered under `id`.
    pub fn material(&self, id: u32) -> Option<&MaterialType> {
        self.materials.get(id as usize)
    }

    /// How many materials are registered.
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    /// True when no material is registered.
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    /// Draw blocks that carry `resource_id` in `material_id` instead of the
    /// material they sit in.
    pub fn set_resource_material(&mut self, _resource_id: u32, _material_id: u32) {}

    /// The material a face of a block shows: the material mapped to the
    /// block's resource when there is one, else the block's own `material_id`.
    pub fn face_material(&self, material_id: u32, _resource_id: Option<u32>) -> u32 {
        material_id
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

/// 3D grid storing voxel blocks using paletted chunk storage.
///
/// Blocks are organized into 16³ chunks. Air chunks are absent from the map
/// (zero memory overhead). The public accessor API is stable; internal storage
/// is an implementation detail.
///
/// # Examples
///
/// ```ignore
/// use flagstone_voxel::{VoxelGrid, BlockPos};
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
    /// Per-chunk lighting state (Stage 4C). Only populated for chunk_size == 16.
    chunk_lights: HashMap<IVec3, ChunkLight>,
}

impl VoxelGrid {
    /// Create a new empty voxel grid.
    ///
    /// `chunk_size` must be 16; the paletted storage and lighting system are
    /// both hardcoded to 16³ chunks.
    pub fn new(chunk_size: i32) -> Self {
        debug_assert_eq!(
            chunk_size, CHUNK_SIZE_I32,
            "VoxelGrid requires chunk_size == 16"
        );
        VoxelGrid {
            chunks: HashMap::new(),
            chunk_size,
            material_registry: MaterialRegistry::new(),
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
    /// - No event publishing.
    /// - No neighbor-chunk invalidation for edge blocks.
    ///
    /// This is the **lowest-level** mutation primitive. External callers must go through
    /// [`VoxelGrid::mutator`].
    pub(crate) fn place_block(
        &mut self,
        pos: BlockPos,
        material_id: u32,
        resource_id: Option<u32>,
    ) {
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

    /// Total number of non-air blocks across all chunks.
    pub fn block_count(&self) -> usize {
        self.chunks
            .values()
            .map(paletted::PalettedChunk::block_count)
            .sum()
    }

    /// Iterator over world positions of all non-air blocks.
    pub fn block_positions(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.chunks.iter().flat_map(|(&chunk_pos, chunk)| {
            let base = chunk_pos * CHUNK_SIZE_I32;
            chunk
                .iter_blocks()
                .map(move |(idx, _mat, _res)| base + idx_to_local(idx))
        })
    }

    /// Iterate over snapshots of every stored block.
    pub fn iter_block_data(&self) -> impl Iterator<Item = BlockData> + '_ {
        self.chunks.iter().flat_map(|(&chunk_pos, chunk)| {
            let base = chunk_pos * CHUNK_SIZE_I32;
            chunk
                .iter_blocks()
                .map(move |(idx, material_id, resource_id)| BlockData {
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
    /// Fails if `data` is malformed, leaving the grid untouched.
    pub fn deserialize_chunk_into(
        &mut self,
        pos: IVec3,
        data: &[u8],
    ) -> Result<(), moho_core::persist::PersistError> {
        let chunk = PalettedChunk::from_bytes(data)?;
        self.chunks.insert(pos, chunk);
        // Ensure a ChunkLight entry exists for the lighting pipeline.
        self.chunk_lights.entry(pos).or_default().light_dirty = true;
        Ok(())
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
            let cl = self.chunk_lights.entry(chunk_pos).or_default();
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
        let cl = self.chunk_lights.entry(chunk_pos).or_default();
        cl.set_block_light_rgb(idx, rgb);
    }

    /// Set the sky-exposed bit at `pos`.
    pub fn set_sky_exposed(&mut self, pos: BlockPos, value: bool) {
        if !self.supports_lighting() {
            return;
        }
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        let cl = self.chunk_lights.entry(chunk_pos).or_default();
        cl.set_sky_exposed(idx, value);
    }

    /// Borrow a chunk's lighting state, if any.
    pub fn chunk_light(&self, chunk_pos: IVec3) -> Option<&ChunkLight> {
        self.chunk_lights.get(&chunk_pos)
    }

    /// Mutable borrow of a chunk's lighting state, allocating if needed.
    pub fn chunk_light_mut(&mut self, chunk_pos: IVec3) -> &mut ChunkLight {
        self.chunk_lights.entry(chunk_pos).or_default()
    }

    /// Iterate all chunk positions with allocated light state.
    pub fn lit_chunk_positions(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.chunk_lights.keys().copied()
    }
}

// -------------------------------------------------------------------------
// Private helpers
// -------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;
    use moho_render_api::RenderMaterial;
    use std::collections::HashSet;

    #[test]
    fn corrupt_chunk_leaves_grid_unchanged() {
        let mut source = VoxelGrid::new(16);
        source.place_block(BlockPos::new(1, 1, 1), 2, Some(4));
        let mut bytes = source
            .serialize_chunk(IVec3::ZERO)
            .expect("modified chunk serializes");
        assert_eq!(&bytes[..5], b"MOHO\x03", "chunk envelope magic and kind");
        bytes[15] ^= 0xFF; // inside the stored CRC
        let mut grid = VoxelGrid::new(16);
        grid.place_block(BlockPos::new(0, 0, 0), 7, None);

        let err = grid
            .deserialize_chunk_into(IVec3::ZERO, &bytes)
            .expect_err("must reject");
        let other = grid.deserialize_chunk_into(IVec3::new(5, 0, 0), &bytes);

        assert!(
            matches!(err, moho_core::persist::PersistError::Corrupt),
            "{err:?}"
        );
        assert!(other.is_err());
        assert_eq!(grid.block_count(), 1);
        assert_eq!(grid.material_at(BlockPos::new(0, 0, 0)), Some(7));
        assert!(!grid.has_chunk(IVec3::new(5, 0, 0)));
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
    fn test_is_smooth_at() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);

        assert_eq!(grid.is_smooth_at(pos), None);
        for (material, smooth) in [(0, true), (99, true), (100, false)] {
            grid.place_block(pos, material, None);

            assert_eq!(grid.is_smooth_at(pos), Some(smooth), "material {material}");
        }
    }

    #[test]
    fn block_positions_reconstruct_world_coords() {
        let mut grid = VoxelGrid::new(16);
        assert_eq!(grid.block_positions().count(), 0);
        let placed = [
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(-1, 17, 3),
        ];
        for pos in placed {
            grid.place_block(pos, 0, None);
        }

        let found: HashSet<IVec3> = grid.block_positions().collect();

        assert_eq!(found, HashSet::from(placed));
    }

    #[test]
    fn test_get_height() {
        // HashMap iteration order is randomly seeded per grid; repeat so an
        // order-dependent answer cannot pass by luck.
        for _ in 0..16 {
            let mut grid = VoxelGrid::new(16);
            assert_eq!(grid.get_height(0, 0), None);

            for y in [17, 20, -3] {
                grid.place_block(IVec3::new(0, y, 0), 0, None);
            }
            grid.place_block(IVec3::new(0, 30, 16), 0, None);

            assert_eq!(grid.get_height(0, 0), Some(20));
            assert_eq!(grid.get_height(1, 0), None);
        }
    }

    #[test]
    fn test_iter_block_data() {
        let mut grid = VoxelGrid::new(16);
        grid.place_block(IVec3::new(0, 0, 0), 1, None);
        grid.place_block(IVec3::new(1, 0, 0), 2, Some(9));
        grid.place_block(IVec3::new(-3, 20, 5), 4, None);

        let data: HashSet<_> = grid
            .iter_block_data()
            .map(|b| (b.position, b.material_id, b.resource_id))
            .collect();

        assert_eq!(
            data,
            HashSet::from([
                (IVec3::new(0, 0, 0), 1, None),
                (IVec3::new(1, 0, 0), 2, Some(9)),
                (IVec3::new(-3, 20, 5), 4, None),
            ])
        );
    }

    #[test]
    fn test_chunk_block_data() {
        let mut grid = VoxelGrid::new(16);
        grid.place_block(IVec3::new(0, 0, 0), 1, None);
        grid.place_block(IVec3::new(20, 0, 0), 2, None);

        let in_chunk = grid.chunk_block_data(IVec3::new(1, 0, 0));

        assert_eq!(
            in_chunk,
            vec![BlockData {
                position: IVec3::new(20, 0, 0),
                material_id: 2,
                resource_id: None,
            }]
        );
        assert!(grid.chunk_block_data(IVec3::new(5, 5, 5)).is_empty());
    }

    #[test]
    fn negative_positions_round_trip() {
        let mut grid = VoxelGrid::new(16);
        let positions = [
            IVec3::new(-1, -1, -1),
            IVec3::new(-16, 0, 0),
            IVec3::new(-17, 5, 31),
        ];

        for (material, pos) in (10..).zip(positions) {
            grid.place_block(pos, material, None);
        }

        for (material, pos) in (10..).zip(positions) {
            assert_eq!(grid.material_at(pos), Some(material), "{pos}");
        }
        assert_eq!(grid.block_count(), 3);
        assert_eq!(grid.material_at(IVec3::new(15, 15, 15)), None);
        assert_eq!(grid.material_at(IVec3::new(0, 0, 0)), None);
    }

    #[test]
    fn overwrite_without_resource_clears_resource() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(3, 4, 5);
        grid.place_block(pos, 1, Some(7));
        assert_eq!(grid.resource_at(pos), Some(7));

        grid.place_block(pos, 1, None);

        assert_eq!(grid.resource_at(pos), None);
        assert_eq!(grid.material_at(pos), Some(1));
    }

    #[test]
    fn chunk_persists_only_when_modified() {
        let mut grid = VoxelGrid::new(16);
        let chunk = IVec3::ZERO;
        assert!(grid.serialize_chunk(chunk).is_none(), "absent chunk");

        grid.place_block(BlockPos::new(1, 1, 1), 2, None);
        let bytes = grid.serialize_chunk(chunk).expect("edited chunk");
        let mut restored = VoxelGrid::new(16);
        restored
            .deserialize_chunk_into(chunk, &bytes)
            .expect("decode");
        assert_eq!(restored.material_at(BlockPos::new(1, 1, 1)), Some(2));
        assert_eq!(restored.block_count(), 1);

        grid.clear_chunk_modified(chunk);

        assert!(grid.serialize_chunk(chunk).is_none(), "unmodified chunk");
        assert_eq!(grid.material_at(BlockPos::new(1, 1, 1)), Some(2));
    }

    #[test]
    fn chunk_round_trips_into_other_chunk_pos() {
        let mut source = VoxelGrid::new(16);
        source.place_block(BlockPos::new(1, 2, 3), 4, Some(6));
        source.place_block(BlockPos::new(15, 15, 15), 5, None);
        let bytes = source.serialize_chunk(IVec3::ZERO).expect("modified");
        let target = IVec3::new(2, 0, -1);
        let mut grid = VoxelGrid::new(16);
        grid.chunk_light_mut(target).light_dirty = false;

        grid.deserialize_chunk_into(target, &bytes).expect("decode");

        let base = IVec3::new(32, 0, -16);
        let data: HashSet<_> = grid
            .iter_block_data()
            .map(|b| (b.position, b.material_id, b.resource_id))
            .collect();
        assert_eq!(
            data,
            HashSet::from([
                (base + IVec3::new(1, 2, 3), 4, Some(6)),
                (base + IVec3::new(15, 15, 15), 5, None),
            ])
        );
        assert_eq!(grid.material_at(IVec3::new(1, 2, 3)), None);
        assert!(grid.chunk_light(target).expect("light entry").light_dirty);
    }

    const COLUMN: IVec3 = IVec3::new(1, 0, -1);

    fn grid_with_clean_lights(lit: &[IVec3]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for &pos in lit {
            grid.chunk_light_mut(pos).sky_dirty = false;
        }
        grid
    }

    fn dirty_sky_chunks(grid: &VoxelGrid) -> HashSet<IVec3> {
        grid.lit_chunk_positions()
            .filter(|&pos| grid.chunk_light(pos).is_some_and(|cl| cl.sky_dirty))
            .collect()
    }

    fn column_neighbourhood() -> Vec<IVec3> {
        vec![
            COLUMN,
            COLUMN + IVec3::new(0, -1, 0),
            COLUMN + IVec3::new(0, -3, 0),
            COLUMN + IVec3::new(0, 1, 0),
            COLUMN + IVec3::new(1, -1, 0),
            COLUMN + IVec3::new(-1, -1, 0),
            COLUMN + IVec3::new(0, -1, 1),
            COLUMN + IVec3::new(0, -1, -1),
            COLUMN + IVec3::new(1, -1, 1),
            COLUMN + IVec3::new(1, 0, 0),
            COLUMN + IVec3::new(0, 0, 1),
        ]
    }

    fn lower_in_column() -> HashSet<IVec3> {
        HashSet::from([
            COLUMN,
            COLUMN + IVec3::new(0, -1, 0),
            COLUMN + IVec3::new(0, -3, 0),
        ])
    }

    fn world_pos(chunk: IVec3, local: IVec3) -> BlockPos {
        chunk * 16 + local
    }

    #[test]
    fn block_change_marks_only_lower_chunks_in_its_column() {
        let mut grid = grid_with_clean_lights(&column_neighbourhood());

        grid.place_block(world_pos(COLUMN, IVec3::new(3, 5, 7)), 2, None);

        assert_eq!(dirty_sky_chunks(&grid), lower_in_column());
    }

    #[test]
    fn removing_a_column_top_block_marks_lower_chunks() {
        let mut grid = VoxelGrid::new(16);
        let top = world_pos(COLUMN, IVec3::new(3, 5, 7));
        grid.place_block(top, 2, None);
        for pos in column_neighbourhood() {
            grid.chunk_light_mut(pos).sky_dirty = false;
        }

        let removed = grid.clear_block(top);

        assert!(removed);
        assert_eq!(dirty_sky_chunks(&grid), lower_in_column());
    }

    #[test]
    fn a_change_below_the_column_top_leaves_sky_clean() {
        let mut grid = VoxelGrid::new(16);
        grid.place_block(world_pos(COLUMN, IVec3::new(3, 9, 7)), 2, None);
        let low = world_pos(COLUMN, IVec3::new(3, 2, 7));
        for pos in column_neighbourhood() {
            grid.chunk_light_mut(pos).sky_dirty = false;
        }

        grid.place_block(low, 2, None);

        assert!(dirty_sky_chunks(&grid).is_empty());

        let removed = grid.clear_block(low);

        assert!(removed);
        assert!(dirty_sky_chunks(&grid).is_empty());
    }

    #[test]
    fn block_change_in_an_already_dirty_chunk_does_not_mark_lower_chunks() {
        let mut grid = grid_with_clean_lights(&column_neighbourhood());
        grid.chunk_light_mut(COLUMN).sky_dirty = true;

        grid.place_block(world_pos(COLUMN, IVec3::new(3, 5, 7)), 2, None);

        assert_eq!(dirty_sky_chunks(&grid), HashSet::from([COLUMN]));
    }

    #[test]
    fn set_lighting_overrides_a_material() {
        let mut registry = MaterialRegistry::new();
        let torch = MaterialLighting {
            emission: [15, 8, 2],
            opacity_cost: 0,
        };
        for id in 0..3 {
            assert_eq!(registry.lighting(id), MaterialLighting::NON_EMISSIVE_OPAQUE);
            assert!(!registry.lighting(id).is_emissive());
        }

        registry.set_lighting(1, torch);

        assert_eq!(registry.lighting(1), torch);
        assert_eq!(registry.emission(1), [15, 8, 2]);
        assert_eq!(registry.opacity_cost(1), 0);
        assert!(registry.lighting(1).is_emissive());
        for id in [0, 2] {
            assert_eq!(registry.lighting(id), MaterialLighting::NON_EMISSIVE_OPAQUE);
            assert_eq!(registry.emission(id), [0, 0, 0]);
            assert_eq!(registry.opacity_cost(id), 15);
        }
    }

    #[test]
    fn any_single_emission_channel_is_emissive() {
        for emission in [[1, 0, 0], [0, 1, 0], [0, 0, 1]] {
            let lighting = MaterialLighting {
                emission,
                opacity_cost: 15,
            };

            assert!(lighting.is_emissive(), "{emission:?}");
        }
    }

    #[test]
    fn chunk_set_is_read_back() {
        let mut grid = VoxelGrid::new(16);
        let origin = IVec3::ZERO;
        let far = IVec3::new(2, -1, 0);
        grid.place_block(BlockPos::new(1, 1, 1), 2, None);
        grid.place_block(world_pos(far, IVec3::new(4, 4, 4)), 3, None);

        let positions: HashSet<IVec3> = grid.chunk_positions().collect();

        assert_eq!(positions, HashSet::from([origin, far]));
        assert!(grid.has_chunk(origin));
        assert!(grid.has_chunk(far));
        assert!(!grid.has_chunk(IVec3::new(0, 1, 0)));
        assert!(!grid.has_chunk(IVec3::new(-2, 1, 0)));
    }

    #[test]
    fn remove_chunk_drops_blocks_and_light_but_keeps_others() {
        let mut grid = VoxelGrid::new(16);
        let origin = IVec3::ZERO;
        let far = IVec3::new(2, -1, 0);
        let far_block = world_pos(far, IVec3::new(4, 4, 4));
        grid.place_block(BlockPos::new(1, 1, 1), 2, None);
        grid.place_block(far_block, 3, None);
        assert!(grid.chunk_light(far).is_some());

        grid.remove_chunk(far);

        assert!(!grid.has_chunk(far));
        assert_eq!(grid.material_at(far_block), None);
        assert!(grid.chunk_light(far).is_none());
        assert!(grid.has_chunk(origin));
        assert_eq!(grid.material_at(BlockPos::new(1, 1, 1)), Some(2));
        assert!(grid.chunk_light(origin).is_some());
        assert_eq!(grid.block_count(), 1);
    }

    #[test]
    fn set_sky_exposed_is_read_back() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(-3, 20, 5);
        let neighbour = BlockPos::new(-2, 20, 5);
        grid.set_sky_exposed(neighbour, true);
        assert!(!grid.sky_exposed_at(pos));

        grid.set_sky_exposed(pos, true);

        assert!(grid.sky_exposed_at(pos));
        assert!(grid.sky_exposed_at(neighbour));

        grid.set_sky_exposed(pos, false);

        assert!(!grid.sky_exposed_at(pos));
        assert!(grid.sky_exposed_at(neighbour));
    }

    #[test]
    fn grass_is_grass_on_top_and_dirt_on_its_sides() {
        let registry = MaterialRegistry::new();

        let grass = registry.material(0).expect("id 0 is registered").to_gpu();

        let top = |c: Vec3| [c.x, c.y, c.z];
        assert_eq!(grass.albedo[..3], top(crate::GRASS));
        assert_eq!(grass.params[..3], top(crate::DIRT));
    }

    #[test]
    fn default_registry_holds_grass_dirt_stone_and_iron_ore_as_terrain_materials() {
        let registry = MaterialRegistry::new();

        assert_eq!(registry.len(), 4);
        let expected = [
            (Vec3::new(0.3, 0.6, 0.3), Vec3::new(0.6, 0.5, 0.4)),
            (Vec3::new(0.6, 0.5, 0.4), Vec3::new(0.6, 0.5, 0.4)),
            (Vec3::new(0.5, 0.5, 0.5), Vec3::new(0.5, 0.5, 0.5)),
            (Vec3::new(0.62, 0.30, 0.16), Vec3::new(0.62, 0.30, 0.16)),
        ];
        for (id, (top, side)) in expected.into_iter().enumerate() {
            assert_eq!(
                registry.material(id as u32),
                Some(&MaterialType::VoxelTerrain {
                    top_albedo: top,
                    side_albedo: side
                }),
                "material {id}"
            );
        }
        assert_eq!(registry.material(4), None);
    }

    #[test]
    fn face_material_prefers_the_resources_material() {
        let registry = MaterialRegistry::new();

        assert_eq!(registry.face_material(2, Some(1)), 3, "iron ore in stone");
        assert_eq!(registry.face_material(1, Some(1)), 3, "iron ore in soil");
    }

    #[test]
    fn face_material_falls_back_without_a_mapping() {
        let mut registry = MaterialRegistry::new();

        assert_eq!(registry.face_material(2, None), 2, "no resource");
        assert_eq!(registry.face_material(1, Some(9)), 1, "unmapped resource");

        registry.set_resource_material(9, 0);

        assert_eq!(registry.face_material(1, Some(9)), 0, "now mapped");
        assert_eq!(registry.face_material(1, None), 1);
    }
}
