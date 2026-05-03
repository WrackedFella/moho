//! Block modification API with automatic chunk state management.
//!
//! This module provides a high-level API for modifying voxel blocks that
//! automatically handles:
//! - Chunk dirty flag management
//! - Neighbor chunk invalidation for edge blocks
//! - Event publishing for modification notifications
//! - Batch modifications for bulk operations
//!
//! # Example
//!
//! ```ignore
//! use moho_core::voxel::{VoxelGrid, BlockModifier, BlockPos};
//! use moho_core::events::EventBus;
//!
//! let grid = VoxelGrid::new(16);
//! let event_bus = EventBus::new();
//! let mut modifier = BlockModifier::new(grid, event_bus);
//!
//! // Single block modification
//! let pos = BlockPos::new(5, 10, 5);
//! modifier.set_block(pos, 1, None);
//!
//! // Batch modification (e.g., explosion)
//! let positions = vec![BlockPos::new(0, 0, 0), BlockPos::new(1, 0, 0)];
//! modifier.remove_blocks_batch(&positions, BlockChangeReason::Unknown);
//! ```

use super::grid::{BlockData, BlockPos, VoxelGrid};
use super::state::{ChunkState, JobId};
use crate::events::{BlockChangeReason, EventBus, WorldEvent};
use glam::IVec3;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Result of a block modification operation
#[derive(Debug, Clone)]
pub struct ModificationResult {
    /// Chunks that were dirtied by this modification
    pub dirty_chunks: Vec<IVec3>,

    /// Whether any neighbor chunks were affected
    pub neighbors_affected: bool,

    /// Number of blocks actually modified
    pub blocks_modified: u32,
}

impl ModificationResult {
    fn empty() -> Self {
        Self {
            dirty_chunks: Vec::new(),
            neighbors_affected: false,
            blocks_modified: 0,
        }
    }

    fn single(chunk_pos: IVec3, neighbors: bool) -> Self {
        Self {
            dirty_chunks: vec![chunk_pos],
            neighbors_affected: neighbors,
            blocks_modified: 1,
        }
    }
}

/// High-level block modification API with automatic state management.
///
/// Wraps a `VoxelGrid` and `ChunkState` map, automatically handling dirty
/// flags, neighbor invalidation, and event publishing when blocks are modified.
pub struct BlockModifier {
    /// The underlying voxel grid
    grid: VoxelGrid,

    /// Chunk state tracking
    chunk_states: HashMap<IVec3, ChunkState>,

    /// Event bus for publishing modification events
    event_bus: Arc<EventBus>,

    /// Chunk size (cached from grid)
    chunk_size: i32,

    /// Whether to publish events (can be disabled for bulk operations)
    events_enabled: bool,
}

impl BlockModifier {
    /// Create a new BlockModifier wrapping the given grid.
    pub fn new(grid: VoxelGrid, event_bus: Arc<EventBus>) -> Self {
        let chunk_size = grid.chunk_size();
        Self {
            grid,
            chunk_states: HashMap::new(),
            event_bus,
            chunk_size,
            events_enabled: true,
        }
    }

    /// Get the underlying VoxelGrid reference
    pub fn grid(&self) -> &VoxelGrid {
        &self.grid
    }

    /// Get a mutable reference to the underlying VoxelGrid
    ///
    /// **Warning**: Direct grid modifications bypass state tracking.
    /// Prefer using the BlockModifier methods for proper state management.
    pub fn grid_mut(&mut self) -> &mut VoxelGrid {
        &mut self.grid
    }

    /// Get the chunk state for a position, creating it if needed
    pub fn get_or_create_chunk_state(&mut self, chunk_pos: IVec3) -> &mut ChunkState {
        self.chunk_states
            .entry(chunk_pos)
            .or_insert_with(|| ChunkState::new(chunk_pos))
    }

    /// Get the chunk state for a position (immutable)
    pub fn get_chunk_state(&self, chunk_pos: IVec3) -> Option<&ChunkState> {
        self.chunk_states.get(&chunk_pos)
    }

    /// Get all chunk states
    pub fn chunk_states(&self) -> &HashMap<IVec3, ChunkState> {
        &self.chunk_states
    }

    /// Get mutable access to all chunk states
    pub fn chunk_states_mut(&mut self) -> &mut HashMap<IVec3, ChunkState> {
        &mut self.chunk_states
    }

    /// Set a block at the given position with automatic state management.
    ///
    /// This will:
    /// 1. Update the block in the grid
    /// 2. Mark the containing chunk as dirty
    /// 3. Mark neighbor chunks dirty if this is an edge block
    /// 4. Publish a `BlockPlaced` event
    pub fn set_block(
        &mut self,
        pos: BlockPos,
        material_id: u32,
        resource_id: Option<u32>,
    ) -> ModificationResult {
        self.set_block_with_reason(pos, material_id, resource_id, BlockChangeReason::Unknown)
    }

    /// Set a block with a specific change reason.
    pub fn set_block_with_reason(
        &mut self,
        pos: BlockPos,
        material_id: u32,
        resource_id: Option<u32>,
        reason: BlockChangeReason,
    ) -> ModificationResult {
        let chunk_pos = VoxelGrid::get_chunk_pos(pos, self.chunk_size);

        // Update the grid
        self.grid.place_block(pos, material_id, resource_id);

        // Mark chunk dirty
        let state = self.get_or_create_chunk_state(chunk_pos);
        state.dirty_all(); // For now, dirty everything; Phase 1 will differentiate terrain/structure

        // Check and dirty neighbor chunks
        let neighbors_affected = self.dirty_neighbors_if_edge(pos, chunk_pos);

        // Publish event
        if self.events_enabled {
            self.event_bus.publish(WorldEvent::BlockPlaced {
                position: pos,
                material_id,
                reason,
            });

            self.event_bus.publish(WorldEvent::ChunkMeshDirty {
                chunk_pos,
                terrain_dirty: true,
                structure_dirty: true,
            });
        }

        ModificationResult::single(chunk_pos, neighbors_affected)
    }

    /// Remove a block at the given position.
    ///
    /// Returns a snapshot of the removed block if one existed.
    pub fn remove_block(&mut self, pos: BlockPos) -> Option<BlockData> {
        self.remove_block_with_reason(pos, BlockChangeReason::Unknown)
    }

    /// Remove a block with a specific change reason.
    pub fn remove_block_with_reason(
        &mut self,
        pos: BlockPos,
        reason: BlockChangeReason,
    ) -> Option<BlockData> {
        let chunk_pos = VoxelGrid::get_chunk_pos(pos, self.chunk_size);

        // Snapshot before removing
        let removed = self.grid.block_data_at(pos);
        if removed.is_some() {
            self.grid.clear_block(pos);
        }

        if let Some(block) = removed {
            // Mark chunk dirty
            let state = self.get_or_create_chunk_state(chunk_pos);
            state.dirty_all();

            // Check and dirty neighbor chunks
            self.dirty_neighbors_if_edge(pos, chunk_pos);

            // Publish event
            if self.events_enabled {
                self.event_bus.publish(WorldEvent::BlockRemoved {
                    position: pos,
                    old_material_id: block.material_id,
                    reason,
                });

                self.event_bus.publish(WorldEvent::ChunkMeshDirty {
                    chunk_pos,
                    terrain_dirty: true,
                    structure_dirty: true,
                });
            }
        }

        removed
    }

    /// Set multiple blocks in a batch operation.
    ///
    /// This is more efficient than individual `set_block` calls as it:
    /// - Groups chunks for single dirty marking
    /// - Publishes a single batch event
    pub fn set_blocks_batch(
        &mut self,
        blocks: Vec<(BlockPos, u32, Option<u32>)>,
        reason: BlockChangeReason,
    ) -> ModificationResult {
        if blocks.is_empty() {
            return ModificationResult::empty();
        }

        // Temporarily disable individual events
        let was_enabled = self.events_enabled;
        self.events_enabled = false;

        let mut dirty_chunks: HashSet<IVec3> = HashSet::new();
        let mut all_positions: Vec<IVec3> = Vec::with_capacity(blocks.len());
        let mut neighbors_affected = false;

        for (pos, material_id, resource_id) in blocks {
            let chunk_pos = VoxelGrid::get_chunk_pos(pos, self.chunk_size);
            dirty_chunks.insert(chunk_pos);
            all_positions.push(pos);

            self.grid.place_block(pos, material_id, resource_id);

            if self.is_edge_block(pos, chunk_pos) {
                neighbors_affected = true;
                self.dirty_neighbor_chunks(pos, chunk_pos);
            }
        }

        // Mark all affected chunks dirty
        for &chunk_pos in &dirty_chunks {
            let state = self.get_or_create_chunk_state(chunk_pos);
            state.dirty_all();
        }

        // Restore event publishing and send batch event
        self.events_enabled = was_enabled;

        if self.events_enabled && !all_positions.is_empty() {
            // Use the first chunk for the batch event (could be improved)
            let primary_chunk = *dirty_chunks.iter().next().unwrap();
            self.event_bus.publish(WorldEvent::BlocksBatchModified {
                chunk_pos: primary_chunk,
                positions: all_positions.clone(),
                reason,
            });

            // Notify each dirty chunk
            for &chunk_pos in &dirty_chunks {
                self.event_bus.publish(WorldEvent::ChunkMeshDirty {
                    chunk_pos,
                    terrain_dirty: true,
                    structure_dirty: true,
                });
            }
        }

        ModificationResult {
            dirty_chunks: dirty_chunks.into_iter().collect(),
            neighbors_affected,
            blocks_modified: all_positions.len() as u32,
        }
    }

    /// Remove multiple blocks in a batch operation.
    pub fn remove_blocks_batch(
        &mut self,
        positions: &[BlockPos],
        reason: BlockChangeReason,
    ) -> ModificationResult {
        if positions.is_empty() {
            return ModificationResult::empty();
        }

        let was_enabled = self.events_enabled;
        self.events_enabled = false;

        let mut dirty_chunks: HashSet<IVec3> = HashSet::new();
        let mut removed_positions: Vec<IVec3> = Vec::new();
        let mut neighbors_affected = false;

        for &pos in positions {
            let chunk_pos = VoxelGrid::get_chunk_pos(pos, self.chunk_size);

            if self.grid.clear_block(pos) {
                dirty_chunks.insert(chunk_pos);
                removed_positions.push(pos);

                if self.is_edge_block(pos, chunk_pos) {
                    neighbors_affected = true;
                    self.dirty_neighbor_chunks(pos, chunk_pos);
                }
            }
        }

        for &chunk_pos in &dirty_chunks {
            let state = self.get_or_create_chunk_state(chunk_pos);
            state.dirty_all();
        }

        self.events_enabled = was_enabled;

        if self.events_enabled && !removed_positions.is_empty() {
            let primary_chunk = *dirty_chunks.iter().next().unwrap();
            self.event_bus.publish(WorldEvent::BlocksBatchModified {
                chunk_pos: primary_chunk,
                positions: removed_positions.clone(),
                reason,
            });

            for &chunk_pos in &dirty_chunks {
                self.event_bus.publish(WorldEvent::ChunkMeshDirty {
                    chunk_pos,
                    terrain_dirty: true,
                    structure_dirty: true,
                });
            }
        }

        ModificationResult {
            dirty_chunks: dirty_chunks.into_iter().collect(),
            neighbors_affected,
            blocks_modified: removed_positions.len() as u32,
        }
    }

    /// Check if a block position is on the edge of its chunk.
    fn is_edge_block(&self, pos: BlockPos, chunk_pos: IVec3) -> bool {
        let local = self.to_local_pos(pos, chunk_pos);

        local.x == 0
            || local.x == self.chunk_size - 1
            || local.y == 0
            || local.y == self.chunk_size - 1
            || local.z == 0
            || local.z == self.chunk_size - 1
    }

    /// Convert world position to chunk-local position.
    fn to_local_pos(&self, pos: BlockPos, chunk_pos: IVec3) -> IVec3 {
        let chunk_origin = chunk_pos * self.chunk_size;
        pos - chunk_origin
    }

    /// Dirty neighbor chunks if this is an edge block.
    ///
    /// Returns true if any neighbors were affected.
    fn dirty_neighbors_if_edge(&mut self, pos: BlockPos, chunk_pos: IVec3) -> bool {
        if self.is_edge_block(pos, chunk_pos) {
            self.dirty_neighbor_chunks(pos, chunk_pos);
            true
        } else {
            false
        }
    }

    /// Mark neighbor chunks as dirty based on which edge the block is on.
    fn dirty_neighbor_chunks(&mut self, pos: BlockPos, chunk_pos: IVec3) {
        let local = self.to_local_pos(pos, chunk_pos);

        // Check each axis for edge conditions
        if local.x == 0 {
            self.dirty_chunk_at(chunk_pos + IVec3::new(-1, 0, 0));
        }
        if local.x == self.chunk_size - 1 {
            self.dirty_chunk_at(chunk_pos + IVec3::new(1, 0, 0));
        }
        if local.y == 0 {
            self.dirty_chunk_at(chunk_pos + IVec3::new(0, -1, 0));
        }
        if local.y == self.chunk_size - 1 {
            self.dirty_chunk_at(chunk_pos + IVec3::new(0, 1, 0));
        }
        if local.z == 0 {
            self.dirty_chunk_at(chunk_pos + IVec3::new(0, 0, -1));
        }
        if local.z == self.chunk_size - 1 {
            self.dirty_chunk_at(chunk_pos + IVec3::new(0, 0, 1));
        }
    }

    /// Mark a specific chunk as dirty (if it exists in our state map).
    fn dirty_chunk_at(&mut self, chunk_pos: IVec3) {
        // Only dirty if we're tracking this chunk
        // This avoids creating state for chunks we haven't loaded yet
        if let Some(state) = self.chunk_states.get_mut(&chunk_pos) {
            state.dirty_all();

            if self.events_enabled {
                self.event_bus.publish(WorldEvent::ChunkMeshDirty {
                    chunk_pos,
                    terrain_dirty: true,
                    structure_dirty: true,
                });
            }
        }
    }

    /// Get all chunks that need mesh regeneration.
    pub fn get_dirty_chunks(&self) -> Vec<IVec3> {
        self.chunk_states
            .iter()
            .filter(|(_, state)| state.needs_work())
            .map(|(pos, _)| *pos)
            .collect()
    }

    /// Get dirty chunks sorted by priority (closest first).
    pub fn get_dirty_chunks_by_priority(&self) -> Vec<IVec3> {
        let mut chunks: Vec<_> = self
            .chunk_states
            .iter()
            .filter(|(_, state)| state.needs_work())
            .collect();

        chunks.sort_by_key(|(_, state)| state.priority);

        chunks.into_iter().map(|(pos, _)| *pos).collect()
    }

    /// Update priorities for all chunks based on player position.
    pub fn update_priorities(&mut self, player_chunk_pos: IVec3) {
        for state in self.chunk_states.values_mut() {
            state.update_priority(player_chunk_pos);
        }
    }

    /// Queue a chunk for terrain mesh generation.
    pub fn queue_terrain_generation(&mut self, chunk_pos: IVec3) -> Option<JobId> {
        self.chunk_states
            .get_mut(&chunk_pos)
            .and_then(|state| state.queue_terrain())
    }

    /// Queue a chunk for structure mesh generation.
    pub fn queue_structure_generation(&mut self, chunk_pos: IVec3) -> Option<JobId> {
        self.chunk_states
            .get_mut(&chunk_pos)
            .and_then(|state| state.queue_structure())
    }

    /// Consume the VoxelGrid out of this modifier (for when you need ownership back).
    pub fn into_grid(self) -> VoxelGrid {
        self.grid
    }

    /// Initialize chunk states for existing blocks in the grid.
    ///
    /// Call this after loading a world to set up state tracking for all chunks.
    pub fn initialize_from_grid(&mut self) {
        let chunk_size = self.chunk_size;

        // Find all unique chunk positions
        let chunk_positions: HashSet<IVec3> = self
            .grid
            .block_positions()
            .map(|pos| VoxelGrid::get_chunk_pos(pos, chunk_size))
            .collect();

        // Create state for each chunk
        for chunk_pos in chunk_positions {
            self.chunk_states
                .entry(chunk_pos)
                .or_insert_with(|| ChunkState::new(chunk_pos));
        }

        log::info!(
            "Initialized {} chunk states from grid",
            self.chunk_states.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_modifier() -> BlockModifier {
        let grid = VoxelGrid::new(16);
        let event_bus = Arc::new(EventBus::new());
        BlockModifier::new(grid, event_bus)
    }

    #[test]
    fn test_set_block_creates_chunk_state() {
        let mut modifier = test_modifier();
        let pos = BlockPos::new(5, 5, 5);

        modifier.set_block(pos, 1, None);

        let chunk_pos = IVec3::ZERO;
        assert!(modifier.get_chunk_state(chunk_pos).is_some());
        assert!(
            modifier
                .get_chunk_state(chunk_pos)
                .unwrap()
                .dirty
                .terrain_mesh
        );
    }

    #[test]
    fn test_edge_block_detection() {
        let modifier = test_modifier();

        // Interior block (5,5,5) in chunk (0,0,0)
        let interior = BlockPos::new(5, 5, 5);
        let chunk_pos = IVec3::ZERO;
        assert!(!modifier.is_edge_block(interior, chunk_pos));

        // Edge block (0,5,5) - on negative X edge
        let edge_x = BlockPos::new(0, 5, 5);
        assert!(modifier.is_edge_block(edge_x, chunk_pos));

        // Edge block (15,5,5) - on positive X edge
        let edge_x_pos = BlockPos::new(15, 5, 5);
        assert!(modifier.is_edge_block(edge_x_pos, chunk_pos));

        // Corner block (0,0,0)
        let corner = BlockPos::new(0, 0, 0);
        assert!(modifier.is_edge_block(corner, chunk_pos));
    }

    #[test]
    fn test_remove_block() {
        let mut modifier = test_modifier();
        let pos = BlockPos::new(5, 5, 5);

        modifier.set_block(pos, 2, None);
        assert!(modifier.grid().is_solid_at(pos));

        let removed = modifier.remove_block(pos);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().material_id, 2);
        assert!(!modifier.grid().is_solid_at(pos));
    }

    #[test]
    fn test_batch_modification() {
        let mut modifier = test_modifier();

        let blocks: Vec<_> = (0..5)
            .map(|i| (BlockPos::new(i, 0, 0), 1u32, None))
            .collect();

        let result = modifier.set_blocks_batch(blocks, BlockChangeReason::Player);

        assert_eq!(result.blocks_modified, 5);
        assert!(!result.dirty_chunks.is_empty());
    }

    #[test]
    fn test_neighbor_chunk_dirtying() {
        let mut modifier = test_modifier();

        // First, create state for neighboring chunk
        let neighbor_chunk = IVec3::new(-1, 0, 0);
        modifier.get_or_create_chunk_state(neighbor_chunk);

        // Clear the dirty flag
        modifier
            .chunk_states
            .get_mut(&neighbor_chunk)
            .unwrap()
            .dirty
            .clear_all();

        // Place block at edge (0,5,5) which borders chunk (-1,0,0)
        let edge_pos = BlockPos::new(0, 5, 5);
        let result = modifier.set_block(edge_pos, 1, None);

        assert!(result.neighbors_affected);

        // Neighbor should now be dirty
        let neighbor_state = modifier.get_chunk_state(neighbor_chunk).unwrap();
        assert!(neighbor_state.dirty.any_dirty());
    }

    #[test]
    fn test_priority_update() {
        let mut modifier = test_modifier();

        // Create chunks at various distances
        modifier.get_or_create_chunk_state(IVec3::new(0, 0, 0));
        modifier.get_or_create_chunk_state(IVec3::new(5, 0, 0));
        modifier.get_or_create_chunk_state(IVec3::new(2, 2, 2));

        // Update priorities with player at origin
        modifier.update_priorities(IVec3::ZERO);

        let state_0 = modifier.get_chunk_state(IVec3::ZERO).unwrap();
        let state_5 = modifier.get_chunk_state(IVec3::new(5, 0, 0)).unwrap();
        let state_222 = modifier.get_chunk_state(IVec3::new(2, 2, 2)).unwrap();

        assert_eq!(state_0.priority, 0);
        assert_eq!(state_5.priority, 5);
        assert_eq!(state_222.priority, 6);
    }

    #[test]
    fn remove_absent_block_returns_none() {
        // Regression guard: remove_block on an empty position must not panic.
        let mut modifier = test_modifier();
        let pos = BlockPos::new(3, 3, 3);
        let result = modifier.remove_block(pos);
        assert!(result.is_none());
    }

    #[test]
    fn set_then_remove_restores_empty_grid() {
        let mut modifier = test_modifier();
        let pos = BlockPos::new(4, 4, 4);

        assert!(!modifier.grid().is_solid_at(pos));
        modifier.set_block(pos, 7, None);
        assert!(modifier.grid().is_solid_at(pos));
        modifier.remove_block(pos);
        assert!(!modifier.grid().is_solid_at(pos));
    }
}
