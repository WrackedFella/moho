//! Light system manager integrating light propagation with event system.
//!
//! This module provides a high-level manager for light updates that:
//! - Listens to block modification events
//! - Automatically enqueues light update jobs
//! - Processes light updates within frame budget
//! - Emits chunk mesh dirty events for affected chunks
//!
//! # Architecture
//!
//! The LightSystem acts as the bridge between block modifications and light
//! propagation. It subscribes to WorldEvents (BlockPlaced, BlockRemoved) and
//! translates them into light update jobs that are processed incrementally.
//!
//! # Example
//!
//! ```ignore
//! use moho_core::voxel::{LightSystem, VoxelGrid, LightFrameBudget};
//! use moho_core::events::EventBus;
//!
//! let grid = VoxelGrid::new(16);
//! let event_bus = EventBus::new();
//! let mut light_system = LightSystem::new(grid, event_bus, LightFrameBudget::balanced());
//!
//! // Process light updates each frame
//! light_system.process_frame();
//!
//! // Check affected chunks and trigger mesh regeneration
//! let affected = light_system.collect_affected_chunks();
//! ```

use super::grid::VoxelGrid;
use super::light_jobs::{
    LightFrameBudget, LightJobQueue, LightJobStats, LightUpdateJob, LightUpdateResult,
};
use crate::events::{BlockChangeReason, EventBus, WorldEvent};
use glam::{IVec3, Vec3};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// Light system manager coordinating light propagation with events
pub struct LightSystem {
    /// The voxel grid (shared reference for light propagation)
    grid: VoxelGrid,

    /// Light job queue for async processing
    job_queue: LightJobQueue,

    /// Event bus for listening to modifications and emitting dirty events
    event_bus: Arc<EventBus>,

    /// Player position for priority calculation (updated externally)
    player_pos: Vec3,

    /// Chunks affected by recent light updates (pending mesh dirty events)
    affected_chunks: HashSet<IVec3>,

    /// Recently completed light update results
    completed_results: VecDeque<LightUpdateResult>,

    /// Statistics
    total_jobs_submitted: u64,
    total_chunks_affected: u64,
}

impl LightSystem {
    /// Create a new light system
    ///
    /// # Arguments
    /// * `grid` - The voxel grid to propagate light through
    /// * `event_bus` - Event bus for listening and publishing events
    /// * `budget` - Frame budget configuration
    pub fn new(grid: VoxelGrid, event_bus: Arc<EventBus>, budget: LightFrameBudget) -> Self {
        let chunk_size = grid.chunk_size();
        let job_queue = LightJobQueue::new(chunk_size, budget);

        Self {
            grid,
            job_queue,
            event_bus,
            player_pos: Vec3::ZERO,
            affected_chunks: HashSet::new(),
            completed_results: VecDeque::new(),
            total_jobs_submitted: 0,
            total_chunks_affected: 0,
        }
    }

    /// Create with default balanced budget
    pub fn with_default_budget(grid: VoxelGrid, event_bus: Arc<EventBus>) -> Self {
        Self::new(grid, event_bus, LightFrameBudget::balanced())
    }

    /// Update player position for priority calculation
    pub fn set_player_position(&mut self, pos: Vec3) {
        self.player_pos = pos;
    }

    /// Get current player position
    pub fn player_position(&self) -> Vec3 {
        self.player_pos
    }

    /// Set frame budget configuration
    pub fn set_budget(&mut self, budget: LightFrameBudget) {
        self.job_queue.set_budget(budget);
    }

    /// Get current frame budget
    pub fn budget(&self) -> LightFrameBudget {
        self.job_queue.budget()
    }

    /// Handle a block placed event.
    pub fn on_block_placed(&mut self, position: IVec3, material_id: u32) {
        let emission = self.grid.material_registry.emission(material_id);
        if emission != [0u8, 0, 0] {
            let job = LightUpdateJob::add_light(position, emission, 0)
                .with_player_distance(self.player_pos);
            self.job_queue.submit(job);
            self.total_jobs_submitted += 1;
        } else {
            // Opaque block placed — any existing light at this position must be removed.
            let opacity = self.grid.material_registry.opacity_cost(material_id);
            if opacity >= 15 {
                let job = LightUpdateJob::remove_light(position, 0)
                    .with_player_distance(self.player_pos);
                self.job_queue.submit(job);
                self.total_jobs_submitted += 1;
            }
        }
    }

    /// Handle a block removed event.
    pub fn on_block_removed(&mut self, position: IVec3, old_material_id: u32) {
        // Remove any block-light the removed block was contributing or blocking.
        let job = LightUpdateJob::remove_light(position, 0).with_player_distance(self.player_pos);
        self.job_queue.submit(job);
        self.total_jobs_submitted += 1;

        log::debug!(
            "Block removed at {:?} (material {}), enqueued light removal job",
            position,
            old_material_id
        );
    }

    /// Handle a batch block modification event
    ///
    /// Enqueues light update jobs for all affected positions
    pub fn on_blocks_batch_modified(&mut self, positions: &[IVec3], reason: &BlockChangeReason) {
        log::debug!(
            "Batch modification of {} blocks (reason: {:?})",
            positions.len(),
            reason
        );

        // For batch operations, we can optimize by:
        // 1. Collecting all affected chunks
        // 2. Submitting fewer, larger jobs

        // For now, simple approach: submit individual jobs for each position
        for &position in positions {
            if let Some(block) = self.grid.get_block(&position) {
                let mat_id = block.material_id;
                let emission = self.grid.material_registry.emission(mat_id);
                let job = if emission != [0u8, 0, 0] {
                    LightUpdateJob::add_light(position, emission, 0)
                } else {
                    LightUpdateJob::remove_light(position, 0)
                };
                self.job_queue.submit(job.with_player_distance(self.player_pos));
                self.total_jobs_submitted += 1;
            }
        }
    }

    /// Process light update jobs for this frame
    ///
    /// Returns the number of jobs completed this frame
    pub fn process_frame(&mut self) -> usize {
        let completed = self.job_queue.process_frame(&mut self.grid);

        // Collect results and track affected chunks
        let results = self.job_queue.collect_results();

        for result in results {
            // Track affected chunks
            for &chunk_pos in &result.affected_chunks {
                self.affected_chunks.insert(chunk_pos);
            }

            self.total_chunks_affected += result.affected_chunks.len() as u64;

            // Store result for external access
            self.completed_results.push_back(result);

            // Limit stored results to prevent unbounded growth
            if self.completed_results.len() > 100 {
                self.completed_results.pop_front();
            }
        }

        completed
    }

    /// Emit ChunkMeshDirty events for all affected chunks
    ///
    /// This should be called after process_frame() to notify the mesh system
    /// that chunks need regeneration due to light changes.
    ///
    /// Returns the number of chunks marked dirty
    pub fn emit_dirty_events(&mut self) -> usize {
        let count = self.affected_chunks.len();

        for &chunk_pos in &self.affected_chunks {
            // Emit event for both terrain and structure meshes
            // (light affects both geometry types)
            self.event_bus.publish(WorldEvent::ChunkMeshDirty {
                chunk_pos,
                terrain_dirty: true,
                structure_dirty: true,
            });
        }

        // Clear affected chunks after emitting events
        self.affected_chunks.clear();

        if count > 0 {
            log::debug!("Emitted {} ChunkMeshDirty events for light updates", count);
        }

        count
    }

    /// Collect affected chunks without emitting events
    ///
    /// Returns a list of chunks affected by recent light updates
    pub fn collect_affected_chunks(&mut self) -> Vec<IVec3> {
        let chunks: Vec<_> = self.affected_chunks.iter().copied().collect();
        self.affected_chunks.clear();
        chunks
    }

    /// Get recently completed light update results
    pub fn recent_results(&self) -> &VecDeque<LightUpdateResult> {
        &self.completed_results
    }

    /// Get number of pending jobs
    pub fn pending_jobs(&self) -> usize {
        self.job_queue.pending_jobs()
    }

    /// Get light job queue statistics
    pub fn stats(&self) -> LightJobStats {
        self.job_queue.stats()
    }

    /// Get system-wide statistics
    pub fn system_stats(&self) -> LightSystemStats {
        LightSystemStats {
            total_jobs_submitted: self.total_jobs_submitted,
            total_chunks_affected: self.total_chunks_affected,
            pending_jobs: self.pending_jobs(),
            queue_stats: self.stats(),
        }
    }

    /// Subscribe to world events
    ///
    /// This sets up event listeners for BlockPlaced and BlockRemoved events.
    /// Should be called once during initialization.
    pub fn subscribe_to_events(&mut self) {
        // Note: Event subscription would typically use callbacks or channels
        // For now, events should be forwarded to on_block_placed/on_block_removed
        // by the caller (e.g., main game loop or BlockModifier)

        log::info!("LightSystem ready to process block modification events");
    }

    /// Process a WorldEvent
    ///
    /// Helper method to handle events polymorphically
    pub fn handle_event(&mut self, event: &WorldEvent) {
        match event {
            WorldEvent::BlockPlaced {
                position,
                material_id,
                ..
            } => {
                self.on_block_placed(*position, *material_id);
            }
            WorldEvent::BlockRemoved {
                position,
                old_material_id,
                ..
            } => {
                self.on_block_removed(*position, *old_material_id);
            }
            WorldEvent::BlocksBatchModified {
                positions, reason, ..
            } => {
                self.on_blocks_batch_modified(positions, reason);
            }
            _ => {
                // Ignore other events
            }
        }
    }

    /// Get reference to the underlying voxel grid
    pub fn grid(&self) -> &VoxelGrid {
        &self.grid
    }

    /// Get mutable reference to the underlying voxel grid
    pub fn grid_mut(&mut self) -> &mut VoxelGrid {
        &mut self.grid
    }
}

/// System-wide statistics for the light system
#[derive(Debug, Clone, Copy)]
pub struct LightSystemStats {
    /// Total jobs submitted since creation
    pub total_jobs_submitted: u64,

    /// Total chunks affected by light updates
    pub total_chunks_affected: u64,

    /// Current number of pending jobs
    pub pending_jobs: usize,

    /// Light job queue statistics
    pub queue_stats: LightJobStats,
}

impl LightSystemStats {
    /// Get average chunks affected per job
    pub fn average_chunks_per_job(&self) -> f64 {
        if self.queue_stats.total_jobs_processed > 0 {
            self.total_chunks_affected as f64 / self.queue_stats.total_jobs_processed as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_light_system_creation() {
        let grid = VoxelGrid::new(16);
        let event_bus = Arc::new(EventBus::new());
        let light_system = LightSystem::with_default_budget(grid, event_bus);

        assert_eq!(light_system.pending_jobs(), 0);
        assert_eq!(light_system.player_position(), Vec3::ZERO);
    }

    #[test]
    fn test_player_position_update() {
        let grid = VoxelGrid::new(16);
        let event_bus = Arc::new(EventBus::new());
        let mut light_system = LightSystem::with_default_budget(grid, event_bus);

        let new_pos = Vec3::new(10.0, 5.0, 10.0);
        light_system.set_player_position(new_pos);

        assert_eq!(light_system.player_position(), new_pos);
    }

    #[test]
    fn test_block_placed_light_source() {
        use crate::voxel::grid::MaterialLighting;
        let mut grid = VoxelGrid::new(16);
        // Material 1: warm torch emission.
        grid.material_registry.set_lighting(
            1,
            MaterialLighting { emission: [15, 8, 2], opacity_cost: 0 },
        );

        let event_bus = Arc::new(EventBus::new());
        let mut light_system = LightSystem::with_default_budget(grid, event_bus);

        // Placing an emissive block should enqueue an add-light job.
        light_system.on_block_placed(IVec3::new(2, 2, 2), 1);
        assert_eq!(light_system.pending_jobs(), 1);
    }

    #[test]
    fn test_process_frame_with_no_jobs() {
        let grid = VoxelGrid::new(16);
        let event_bus = Arc::new(EventBus::new());
        let mut light_system = LightSystem::with_default_budget(grid, event_bus);

        let completed = light_system.process_frame();
        assert_eq!(completed, 0);

        let affected = light_system.emit_dirty_events();
        assert_eq!(affected, 0);
    }

    #[test]
    fn test_budget_configuration() {
        let grid = VoxelGrid::new(16);
        let event_bus = Arc::new(EventBus::new());
        let mut light_system = LightSystem::with_default_budget(grid, event_bus);

        let aggressive = LightFrameBudget::aggressive();
        light_system.set_budget(aggressive);

        assert_eq!(
            light_system.budget().max_blocks_per_frame,
            aggressive.max_blocks_per_frame
        );
    }
}
