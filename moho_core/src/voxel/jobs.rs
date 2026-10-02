//! Mesh generation job queue with priority scheduling.
//!
//! This module provides an async job queue for mesh generation that:
//! - Prioritizes chunks by distance to player
//! - Supports job cancellation when chunks move out of range
//! - Manages a thread pool for background mesh generation
//! - Provides job status tracking and completion callbacks
//!
//! # Architecture
//!
//! The system uses a priority queue where jobs are ordered by distance to the
//! player. When a worker thread becomes available, it picks the highest priority
//! (closest) job from the queue.
//!
//! Jobs can be cancelled at any time. If a job is in the queue, it's removed.
//! If a job is currently being processed, a cancellation flag is set that the
//! worker can check periodically.

use super::VoxelChunk;
use super::grid::VoxelGrid;
use super::state::JobId;
use glam::IVec3;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread::{self, JoinHandle};

/// Type of mesh generation job
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshJobType {
    /// Generate terrain (smooth) mesh
    Terrain,
    /// Generate structure (blocky) mesh
    Structure,
    /// Generate both mesh types (full rebuild)
    Full,
}

/// A mesh generation job
#[derive(Debug, Clone)]
pub struct MeshJob {
    /// Unique identifier for this job
    pub id: JobId,

    /// Chunk position to generate mesh for
    pub chunk_pos: IVec3,

    /// Type of mesh to generate
    pub job_type: MeshJobType,

    /// Priority (lower = higher priority, typically distance to player)
    pub priority: i32,
}

impl MeshJob {
    /// Create a new mesh job
    pub fn new(chunk_pos: IVec3, job_type: MeshJobType, priority: i32) -> Self {
        Self {
            id: JobId::new(),
            chunk_pos,
            job_type,
            priority,
        }
    }
}

// Implement ordering for priority queue (min-heap by priority)
impl PartialEq for MeshJob {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for MeshJob {}

impl PartialOrd for MeshJob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MeshJob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap (lower priority value = higher priority)
        other.priority.cmp(&self.priority)
    }
}

/// Result of a completed mesh generation job
#[derive(Debug)]
pub struct MeshJobResult {
    /// The job that was completed
    pub job: MeshJob,

    /// The generated chunk mesh (None if cancelled or failed)
    pub chunk: Option<VoxelChunk>,

    /// Whether the job was cancelled
    pub cancelled: bool,

    /// Error message if generation failed
    pub error: Option<String>,

    /// Time taken to generate (in milliseconds)
    pub generation_time_ms: u64,
}

/// Cancellation token for a job
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if this job has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Cancel this job
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Internal state shared between queue and workers
struct QueueState {
    /// Priority queue of pending jobs
    jobs: BinaryHeap<MeshJob>,

    /// Set of job IDs in the queue (for quick lookup)
    pending_ids: HashSet<JobId>,

    /// Cancellation tokens for active jobs
    cancellation_tokens: HashMap<JobId, CancellationToken>,

    /// Whether the queue is shutting down
    shutdown: bool,
}

impl QueueState {
    fn new() -> Self {
        Self {
            jobs: BinaryHeap::new(),
            pending_ids: HashSet::new(),
            cancellation_tokens: HashMap::new(),
            shutdown: false,
        }
    }
}

/// Function type for mesh generation
/// Takes grid reference, chunk position, job type, and cancellation token
pub type MeshGeneratorFn = Box<
    dyn Fn(&VoxelGrid, IVec3, MeshJobType, &CancellationToken) -> Option<VoxelChunk> + Send + Sync,
>;

/// Mesh generation job queue with thread pool
pub struct MeshJobQueue {
    /// Shared queue state
    state: Arc<Mutex<QueueState>>,

    /// Condition variable for worker notification
    condvar: Arc<Condvar>,

    /// Worker thread handles
    workers: Vec<JoinHandle<()>>,

    /// Completed job results waiting to be collected
    results: Arc<Mutex<Vec<MeshJobResult>>>,

    /// Reference to the voxel grid (for mesh generation)
    #[allow(dead_code)] // Used when integrating with actual mesh generation in Phase 1
    grid: Arc<RwLock<VoxelGrid>>,

    /// Mesh generation function
    #[allow(dead_code)] // Used when integrating with actual mesh generation in Phase 1
    generator: Arc<MeshGeneratorFn>,

    /// Maximum number of pending jobs
    max_pending: usize,
}

impl MeshJobQueue {
    /// Create a new mesh job queue with the specified number of worker threads.
    ///
    /// # Arguments
    /// * `num_workers` - Number of worker threads (typically num_cpus - 1)
    /// * `grid` - Shared reference to the voxel grid
    /// * `generator` - Function that generates a mesh for a chunk
    /// * `max_pending` - Maximum number of jobs in the queue
    pub fn new(
        num_workers: usize,
        grid: Arc<RwLock<VoxelGrid>>,
        generator: MeshGeneratorFn,
        max_pending: usize,
    ) -> Self {
        let state = Arc::new(Mutex::new(QueueState::new()));
        let condvar = Arc::new(Condvar::new());
        let results = Arc::new(Mutex::new(Vec::new()));
        let generator = Arc::new(generator);

        let mut workers = Vec::with_capacity(num_workers);

        for i in 0..num_workers {
            let state_clone = Arc::clone(&state);
            let condvar_clone = Arc::clone(&condvar);
            let results_clone = Arc::clone(&results);
            let grid_clone = Arc::clone(&grid);
            let generator_clone = Arc::clone(&generator);

            let handle = thread::Builder::new()
                .name(format!("mesh-worker-{}", i))
                .spawn(move || {
                    Self::worker_loop(
                        state_clone,
                        condvar_clone,
                        results_clone,
                        grid_clone,
                        generator_clone,
                    );
                })
                .expect("Failed to spawn mesh worker thread");

            workers.push(handle);
        }

        log::info!("MeshJobQueue started with {} workers", num_workers);

        Self {
            state,
            condvar,
            workers,
            results,
            grid,
            generator,
            max_pending,
        }
    }

    /// Submit a new mesh generation job.
    ///
    /// Returns the job ID if queued, or None if the queue is full.
    pub fn submit(&self, chunk_pos: IVec3, job_type: MeshJobType, priority: i32) -> Option<JobId> {
        let mut state = self.state.lock().unwrap();

        // Check queue capacity
        if state.jobs.len() >= self.max_pending {
            log::warn!("Mesh job queue full, rejecting job for {:?}", chunk_pos);
            return None;
        }

        let job = MeshJob::new(chunk_pos, job_type, priority);
        let job_id = job.id;

        // Create cancellation token
        let token = CancellationToken::new();
        state.cancellation_tokens.insert(job_id, token);

        state.pending_ids.insert(job_id);
        state.jobs.push(job);

        // Notify a worker
        self.condvar.notify_one();

        log::debug!(
            "Submitted mesh job {:?} for chunk {:?} (priority {})",
            job_id,
            chunk_pos,
            priority
        );

        Some(job_id)
    }

    /// Cancel a pending or in-progress job.
    ///
    /// Returns true if the job was found and cancelled.
    pub fn cancel(&self, job_id: JobId) -> bool {
        let mut state = self.state.lock().unwrap();

        // If job is pending, remove it from queue
        if state.pending_ids.remove(&job_id) {
            // Rebuild heap without the cancelled job
            let jobs: Vec<_> = state.jobs.drain().filter(|j| j.id != job_id).collect();
            state.jobs = jobs.into_iter().collect();
            state.cancellation_tokens.remove(&job_id);

            log::debug!("Cancelled pending job {:?}", job_id);
            return true;
        }

        // If job is in progress, set cancellation flag
        if let Some(token) = state.cancellation_tokens.get(&job_id) {
            token.cancel();
            log::debug!("Requested cancellation of in-progress job {:?}", job_id);
            return true;
        }

        false
    }

    /// Cancel all jobs for a specific chunk.
    pub fn cancel_chunk(&self, chunk_pos: IVec3) {
        let mut state = self.state.lock().unwrap();

        // Find and cancel jobs for this chunk
        let to_cancel: Vec<JobId> = state
            .jobs
            .iter()
            .filter(|j| j.chunk_pos == chunk_pos)
            .map(|j| j.id)
            .collect();

        for job_id in &to_cancel {
            state.pending_ids.remove(job_id);
            if let Some(token) = state.cancellation_tokens.get(job_id) {
                token.cancel();
            }
        }

        // Rebuild heap without cancelled jobs
        let jobs: Vec<_> = state
            .jobs
            .drain()
            .filter(|j| j.chunk_pos != chunk_pos)
            .collect();
        state.jobs = jobs.into_iter().collect();
    }

    /// Get the number of pending jobs.
    pub fn pending_count(&self) -> usize {
        self.state.lock().unwrap().jobs.len()
    }

    /// Get the number of active (in-progress) jobs.
    pub fn active_count(&self) -> usize {
        let state = self.state.lock().unwrap();
        state.cancellation_tokens.len() - state.pending_ids.len()
    }

    /// Collect completed job results.
    ///
    /// Returns all results that have been completed since the last call.
    pub fn collect_results(&self) -> Vec<MeshJobResult> {
        let mut results = self.results.lock().unwrap();
        std::mem::take(&mut *results)
    }

    /// Update job priorities based on a new player position.
    ///
    /// This rebuilds the priority queue with updated priorities.
    pub fn update_priorities(&self, player_chunk_pos: IVec3) {
        let mut state = self.state.lock().unwrap();

        // Drain and reprioritize all jobs
        let mut jobs: Vec<_> = state.jobs.drain().collect();

        for job in &mut jobs {
            let delta = job.chunk_pos - player_chunk_pos;
            job.priority = delta.x.abs() + delta.y.abs() + delta.z.abs();
        }

        state.jobs = jobs.into_iter().collect();
    }

    /// Worker thread main loop
    fn worker_loop(
        state: Arc<Mutex<QueueState>>,
        condvar: Arc<Condvar>,
        results: Arc<Mutex<Vec<MeshJobResult>>>,
        grid: Arc<RwLock<VoxelGrid>>,
        generator: Arc<MeshGeneratorFn>,
    ) {
        loop {
            // Wait for a job
            let (job, token) = {
                let mut state_guard = state.lock().unwrap();

                while state_guard.jobs.is_empty() && !state_guard.shutdown {
                    state_guard = condvar.wait(state_guard).unwrap();
                }

                if state_guard.shutdown {
                    break;
                }

                // Pop the highest priority job
                let job = match state_guard.jobs.pop() {
                    Some(j) => j,
                    None => continue,
                };

                state_guard.pending_ids.remove(&job.id);

                // Get the cancellation token (move to in-progress)
                let token = state_guard
                    .cancellation_tokens
                    .get(&job.id)
                    .cloned()
                    .unwrap_or_else(CancellationToken::new);

                (job, token)
            };

            // Check if already cancelled before starting
            if token.is_cancelled() {
                let result = MeshJobResult {
                    job: job.clone(),
                    chunk: None,
                    cancelled: true,
                    error: None,
                    generation_time_ms: 0,
                };
                results.lock().unwrap().push(result);

                // Clean up cancellation token
                state.lock().unwrap().cancellation_tokens.remove(&job.id);
                continue;
            }

            // Generate the mesh
            let start = std::time::Instant::now();
            let chunk = {
                let grid_guard = grid.read().unwrap();
                generator(&grid_guard, job.chunk_pos, job.job_type, &token)
            };
            let generation_time_ms = start.elapsed().as_millis() as u64;

            // Build result
            let cancelled = token.is_cancelled();
            let result = MeshJobResult {
                job: job.clone(),
                chunk: if cancelled { None } else { chunk },
                cancelled,
                error: None,
                generation_time_ms,
            };

            // Store result
            results.lock().unwrap().push(result);

            // Clean up cancellation token
            state.lock().unwrap().cancellation_tokens.remove(&job.id);

            log::debug!(
                "Completed mesh job {:?} for {:?} in {}ms (cancelled: {})",
                job.id,
                job.chunk_pos,
                generation_time_ms,
                cancelled
            );
        }
    }

    /// Shutdown the job queue and wait for all workers to finish.
    pub fn shutdown(self) {
        {
            let mut state = self.state.lock().unwrap();
            state.shutdown = true;

            // Cancel all pending jobs
            for token in state.cancellation_tokens.values() {
                token.cancel();
            }
        }

        // Wake up all workers
        self.condvar.notify_all();

        // Wait for workers to finish
        for worker in self.workers {
            let _ = worker.join();
        }

        log::info!("MeshJobQueue shutdown complete");
    }
}

/// Builder for MeshJobQueue with sensible defaults
pub struct MeshJobQueueBuilder {
    num_workers: Option<usize>,
    max_pending: usize,
}

impl MeshJobQueueBuilder {
    pub fn new() -> Self {
        Self {
            num_workers: None,
            max_pending: 256,
        }
    }

    /// Set the number of worker threads
    pub fn workers(mut self, count: usize) -> Self {
        self.num_workers = Some(count);
        self
    }

    /// Set the maximum number of pending jobs
    pub fn max_pending(mut self, max: usize) -> Self {
        self.max_pending = max;
        self
    }

    /// Build the queue with the given grid and generator
    pub fn build(self, grid: Arc<RwLock<VoxelGrid>>, generator: MeshGeneratorFn) -> MeshJobQueue {
        let num_workers = self.num_workers.unwrap_or_else(|| {
            // Default to num_cpus - 1, minimum 1
            let cpus = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(4);
            (cpus - 1).max(1)
        });

        MeshJobQueue::new(num_workers, grid, generator, self.max_pending)
    }
}

impl Default for MeshJobQueueBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a hybrid mesh generator function for use with MeshJobQueue.
///
/// This generator uses Phase 1's hybrid mesh generation system, automatically
/// selecting between smooth (Marching Cubes) and blocky (greedy meshing) based
/// on block material types.
///
/// # Returns
/// A `MeshGeneratorFn` that can be passed to `MeshJobQueue::new()`
///
/// # Example
/// ```ignore
/// let grid = Arc::new(RwLock::new(VoxelGrid::new(16)));
/// let generator = create_hybrid_generator();
/// let queue = MeshJobQueue::new(4, grid, generator, 1000);
/// ```
pub fn create_hybrid_generator() -> MeshGeneratorFn {
    Box::new(
        |grid: &VoxelGrid, chunk_pos: IVec3, _job_type: MeshJobType, token: &CancellationToken| {
            // Check cancellation before starting
            if token.is_cancelled() {
                return None;
            }

            // Generate mesh using hybrid system
            let chunk = VoxelChunk::from_grid_hybrid(grid, chunk_pos);

            // Check cancellation after generation
            if token.is_cancelled() {
                return None;
            }

            // Return None for empty chunks
            if chunk.is_empty() { None } else { Some(chunk) }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_priority_ordering() {
        let low_priority = MeshJob::new(IVec3::ZERO, MeshJobType::Full, 10);
        let high_priority = MeshJob::new(IVec3::ONE, MeshJobType::Full, 1);

        // Higher priority (lower number) should be "greater" for max-heap behavior
        // But we want min-heap, so we reverse it
        assert!(high_priority > low_priority);
    }

    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());

        // Cloned token shares state
        let token2 = token.clone();
        assert!(token2.is_cancelled());
    }

    #[test]
    fn test_job_id_uniqueness() {
        let job1 = MeshJob::new(IVec3::ZERO, MeshJobType::Full, 0);
        let job2 = MeshJob::new(IVec3::ZERO, MeshJobType::Full, 0);

        assert_ne!(job1.id, job2.id);
    }
}
