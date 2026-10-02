//! Chunk state management for terrain modification infrastructure.
//!
//! This module provides state tracking for voxel chunks, enabling:
//! - Dirty flags for mesh and lighting regeneration
//! - Generation state machine for async mesh building
//! - Double-buffering support for seamless mesh swaps
//!
//! # State Machine
//!
//! ```text
//!      ┌─────────┐
//!      │  Idle   │◄─────────────────────────────┐
//!      └────┬────┘                              │
//!           │ Block modified                    │
//!           ▼                                   │
//!      ┌─────────┐                              │
//!      │ Dirty   │                              │
//!      └────┬────┘                              │
//!           │ Job scheduled                     │
//!           ▼                                   │
//!      ┌─────────┐                              │
//!      │ Queued  │──── Cancelled ───────────────┤
//!      └────┬────┘     (player moved away)      │
//!           │ Worker picks up                   │
//!           ▼                                   │
//!      ┌────────────┐                           │
//!      │ Generating │──── Cancelled ────────────┤
//!      └─────┬──────┘                           │
//!            │ Complete                         │
//!            ▼                                  │
//!      ┌─────────┐                              │
//!      │  Ready  │──── Swap mesh, return to ────┘
//!      └─────────┘     Idle
//! ```

use glam::IVec3;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for mesh generation jobs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

impl JobId {
    /// Generate a new unique job ID
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        JobId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

/// Generation state for async mesh building
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GenerationState {
    /// Chunk mesh is up-to-date, no work needed
    #[default]
    Idle,

    /// Chunk has pending changes, needs regeneration
    Dirty,

    /// Job is queued in the mesh generation queue
    Queued { job_id: JobId },

    /// Worker thread is actively generating mesh
    Generating { job_id: JobId },

    /// New mesh is ready, waiting for swap to GPU
    Ready { job_id: JobId },
}

impl GenerationState {
    /// Check if the chunk needs mesh regeneration
    pub fn needs_rebuild(&self) -> bool {
        matches!(self, GenerationState::Dirty)
    }

    /// Check if a job is currently active (queued or generating)
    pub fn has_active_job(&self) -> bool {
        matches!(
            self,
            GenerationState::Queued { .. } | GenerationState::Generating { .. }
        )
    }

    /// Check if new mesh is ready for swap
    pub fn is_ready(&self) -> bool {
        matches!(self, GenerationState::Ready { .. })
    }

    /// Get the active job ID if any
    pub fn job_id(&self) -> Option<JobId> {
        match self {
            GenerationState::Queued { job_id }
            | GenerationState::Generating { job_id }
            | GenerationState::Ready { job_id } => Some(*job_id),
            _ => None,
        }
    }
}

/// Dirty flags for different chunk data types
#[derive(Debug, Clone, Default)]
pub struct DirtyFlags {
    /// Terrain (smooth) mesh needs regeneration
    pub terrain_mesh: bool,

    /// Structure (blocky) mesh needs regeneration
    pub structure_mesh: bool,

    /// Light levels need recalculation
    pub light: bool,
}

impl DirtyFlags {
    /// Create with all flags clean
    pub fn clean() -> Self {
        Self::default()
    }

    /// Create with all flags dirty
    pub fn all_dirty() -> Self {
        Self {
            terrain_mesh: true,
            structure_mesh: true,
            light: true,
        }
    }

    /// Check if any mesh needs rebuilding
    pub fn any_mesh_dirty(&self) -> bool {
        self.terrain_mesh || self.structure_mesh
    }

    /// Check if anything is dirty
    pub fn any_dirty(&self) -> bool {
        self.terrain_mesh || self.structure_mesh || self.light
    }

    /// Clear all dirty flags
    pub fn clear_all(&mut self) {
        self.terrain_mesh = false;
        self.structure_mesh = false;
        self.light = false;
    }

    /// Clear only mesh-related flags
    pub fn clear_mesh(&mut self) {
        self.terrain_mesh = false;
        self.structure_mesh = false;
    }
}

/// Complete state tracking for a voxel chunk
#[derive(Debug, Clone)]
pub struct ChunkState {
    /// Chunk position in chunk coordinates
    pub chunk_pos: IVec3,

    /// Dirty flags indicating what needs regeneration
    pub dirty: DirtyFlags,

    /// Terrain mesh generation state
    pub terrain_state: GenerationState,

    /// Structure mesh generation state
    pub structure_state: GenerationState,

    /// Priority for mesh generation (lower = higher priority)
    /// Typically based on distance to player
    pub priority: i32,

    /// Whether this chunk is visible to the camera
    pub visible: bool,

    /// Timestamp of last modification (for debugging/metrics)
    pub last_modified: std::time::Instant,
}

impl ChunkState {
    /// Create a new chunk state for the given position
    pub fn new(chunk_pos: IVec3) -> Self {
        Self {
            chunk_pos,
            dirty: DirtyFlags::all_dirty(), // New chunks need initial build
            terrain_state: GenerationState::Dirty,
            structure_state: GenerationState::Dirty,
            priority: i32::MAX, // Start with lowest priority
            visible: false,
            last_modified: std::time::Instant::now(),
        }
    }

    /// Mark terrain mesh as dirty and reset state
    pub fn dirty_terrain(&mut self) {
        self.dirty.terrain_mesh = true;
        // Only reset to Dirty if not already processing
        if matches!(self.terrain_state, GenerationState::Idle) {
            self.terrain_state = GenerationState::Dirty;
        }
        self.last_modified = std::time::Instant::now();
    }

    /// Mark structure mesh as dirty and reset state
    pub fn dirty_structure(&mut self) {
        self.dirty.structure_mesh = true;
        if matches!(self.structure_state, GenerationState::Idle) {
            self.structure_state = GenerationState::Dirty;
        }
        self.last_modified = std::time::Instant::now();
    }

    /// Mark light as dirty
    pub fn dirty_light(&mut self) {
        self.dirty.light = true;
        self.last_modified = std::time::Instant::now();
    }

    /// Mark all aspects as dirty (for full rebuild)
    pub fn dirty_all(&mut self) {
        self.dirty = DirtyFlags::all_dirty();
        self.terrain_state = GenerationState::Dirty;
        self.structure_state = GenerationState::Dirty;
        self.last_modified = std::time::Instant::now();
    }

    /// Update priority based on distance to a position (in chunk coords)
    pub fn update_priority(&mut self, player_chunk_pos: IVec3) {
        let delta = self.chunk_pos - player_chunk_pos;
        // Manhattan distance for simple priority
        self.priority = delta.x.abs() + delta.y.abs() + delta.z.abs();
    }

    /// Check if any work is needed for this chunk
    pub fn needs_work(&self) -> bool {
        self.dirty.any_dirty()
            || self.terrain_state.needs_rebuild()
            || self.structure_state.needs_rebuild()
    }

    /// Transition terrain state to queued
    pub fn queue_terrain(&mut self) -> Option<JobId> {
        if self.terrain_state.needs_rebuild() {
            let job_id = JobId::new();
            self.terrain_state = GenerationState::Queued { job_id };
            Some(job_id)
        } else {
            None
        }
    }

    /// Transition structure state to queued
    pub fn queue_structure(&mut self) -> Option<JobId> {
        if self.structure_state.needs_rebuild() {
            let job_id = JobId::new();
            self.structure_state = GenerationState::Queued { job_id };
            Some(job_id)
        } else {
            None
        }
    }

    /// Mark terrain generation as started
    pub fn start_terrain_generation(&mut self, job_id: JobId) {
        if let GenerationState::Queued { job_id: qid } = self.terrain_state
            && qid == job_id
        {
            self.terrain_state = GenerationState::Generating { job_id };
        }
    }

    /// Mark structure generation as started
    pub fn start_structure_generation(&mut self, job_id: JobId) {
        if let GenerationState::Queued { job_id: qid } = self.structure_state
            && qid == job_id
        {
            self.structure_state = GenerationState::Generating { job_id };
        }
    }

    /// Mark terrain generation as complete (ready for swap)
    pub fn complete_terrain_generation(&mut self, job_id: JobId) {
        if let GenerationState::Generating { job_id: gid } = self.terrain_state
            && gid == job_id
        {
            self.terrain_state = GenerationState::Ready { job_id };
            self.dirty.terrain_mesh = false;
        }
    }

    /// Mark structure generation as complete (ready for swap)
    pub fn complete_structure_generation(&mut self, job_id: JobId) {
        if let GenerationState::Generating { job_id: gid } = self.structure_state
            && gid == job_id
        {
            self.structure_state = GenerationState::Ready { job_id };
            self.dirty.structure_mesh = false;
        }
    }

    /// Finalize terrain swap and return to idle
    pub fn finalize_terrain_swap(&mut self) {
        if matches!(self.terrain_state, GenerationState::Ready { .. }) {
            self.terrain_state = GenerationState::Idle;
        }
    }

    /// Finalize structure swap and return to idle
    pub fn finalize_structure_swap(&mut self) {
        if matches!(self.structure_state, GenerationState::Ready { .. }) {
            self.structure_state = GenerationState::Idle;
        }
    }

    /// Cancel any active terrain job (player moved away)
    pub fn cancel_terrain_job(&mut self) {
        if self.terrain_state.has_active_job() {
            // Return to dirty so it can be re-queued if needed
            self.terrain_state = if self.dirty.terrain_mesh {
                GenerationState::Dirty
            } else {
                GenerationState::Idle
            };
        }
    }

    /// Cancel any active structure job
    pub fn cancel_structure_job(&mut self) {
        if self.structure_state.has_active_job() {
            self.structure_state = if self.dirty.structure_mesh {
                GenerationState::Dirty
            } else {
                GenerationState::Idle
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_id_uniqueness() {
        let id1 = JobId::new();
        let id2 = JobId::new();
        let id3 = JobId::new();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_generation_state_transitions() {
        let mut state = ChunkState::new(IVec3::ZERO);

        // Initial state is dirty
        assert!(state.terrain_state.needs_rebuild());

        // Queue the job
        let job_id = state.queue_terrain().unwrap();
        assert!(state.terrain_state.has_active_job());
        assert!(!state.terrain_state.needs_rebuild());

        // Start generation
        state.start_terrain_generation(job_id);
        assert!(matches!(
            state.terrain_state,
            GenerationState::Generating { .. }
        ));

        // Complete generation
        state.complete_terrain_generation(job_id);
        assert!(state.terrain_state.is_ready());
        assert!(!state.dirty.terrain_mesh);

        // Finalize swap
        state.finalize_terrain_swap();
        assert!(matches!(state.terrain_state, GenerationState::Idle));
    }

    #[test]
    fn test_dirty_flags() {
        let mut flags = DirtyFlags::clean();
        assert!(!flags.any_dirty());

        flags.terrain_mesh = true;
        assert!(flags.any_dirty());
        assert!(flags.any_mesh_dirty());

        flags.clear_mesh();
        assert!(!flags.any_mesh_dirty());

        flags.light = true;
        assert!(flags.any_dirty());

        flags.clear_all();
        assert!(!flags.any_dirty());
    }

    #[test]
    fn test_priority_update() {
        let mut state = ChunkState::new(IVec3::new(5, 0, 5));

        state.update_priority(IVec3::ZERO);
        assert_eq!(state.priority, 10); // |5| + |0| + |5|

        state.update_priority(IVec3::new(5, 0, 5));
        assert_eq!(state.priority, 0); // Same position

        state.update_priority(IVec3::new(-5, 0, -5));
        assert_eq!(state.priority, 20); // |10| + |0| + |10|
    }

    #[test]
    fn test_cancel_returns_to_correct_state() {
        let mut state = ChunkState::new(IVec3::ZERO);

        // Queue and start terrain job
        let job_id = state.queue_terrain().unwrap();
        state.start_terrain_generation(job_id);

        // Cancel while dirty flag is still set
        state.dirty.terrain_mesh = true;
        state.cancel_terrain_job();
        assert!(matches!(state.terrain_state, GenerationState::Dirty));

        // Queue again, this time clear dirty before cancel
        let job_id = state.queue_terrain().unwrap();
        state.start_terrain_generation(job_id);
        state.dirty.terrain_mesh = false;
        state.cancel_terrain_job();
        assert!(matches!(state.terrain_state, GenerationState::Idle));
    }
}
