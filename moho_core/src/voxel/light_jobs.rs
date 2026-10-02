//! Async light update job queue with priority scheduling.
//!
//! This module provides an async job queue for light propagation that:
//! - Prioritizes light updates by distance to player
//! - Supports job cancellation when jobs become irrelevant
//! - Implements frame budget to avoid frame drops
//! - Integrates with block modification events
//!
//! # Architecture
//!
//! Light updates are queued and processed incrementally over multiple frames.
//! Small operations (torch placement) complete in a single frame, while large
//! operations (explosions affecting hundreds of blocks) are split across frames.
//!
//! Jobs are prioritized by distance to player - lights closer to the player
//! are updated first for better perceived responsiveness.
//!
//! # Performance Targets
//!
//! - Single torch place: <2ms (usually completes in one frame)
//! - Single torch remove: <5ms (usually completes in one frame)
//! - Block placement shadowing light: <3ms
//! - Large edit (explosion): Split over multiple frames, max 100 blocks/frame

use super::grid::{BlockPos, VoxelGrid};
use super::light_propagation::LightPropagator;
use super::state::JobId;
use glam::{IVec3, Vec3};
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Type of light update operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightUpdateOp {
    /// Add light from a new source (torch placement, emissive block)
    Add,
    /// Remove light from a removed source (torch removal, block placement blocking light)
    Remove,
    /// Full flood-fill (chunk generation, large rebuild)
    FloodFill,
}

/// A light update job
#[derive(Debug, Clone)]
pub struct LightUpdateJob {
    /// Unique identifier for this job
    pub id: JobId,

    /// Type of operation to perform
    pub op: LightUpdateOp,

    /// Source position for Add/Remove operations (ignored for FloodFill)
    pub source_pos: BlockPos,

    /// RGB emission for Add operations. [0,0,0] for Remove/FloodFill.
    pub rgb: [u8; 3],

    /// Priority (lower = higher priority, typically distance to player)
    pub priority: i32,

    /// Affected chunk coordinates (if known in advance)
    pub affected_chunks: Option<Vec<IVec3>>,
}

impl LightUpdateJob {
    /// Create a new light update job for adding RGB block-light.
    pub fn add_light(source_pos: BlockPos, rgb: [u8; 3], priority: i32) -> Self {
        Self {
            id: JobId::new(),
            op: LightUpdateOp::Add,
            source_pos,
            rgb,
            priority,
            affected_chunks: None,
        }
    }

    /// Create a new light update job for removing block-light at `source_pos`.
    pub fn remove_light(source_pos: BlockPos, priority: i32) -> Self {
        Self {
            id: JobId::new(),
            op: LightUpdateOp::Remove,
            source_pos,
            rgb: [0, 0, 0],
            priority,
            affected_chunks: None,
        }
    }

    /// Create a new light update job for full flood-fill (e.g. initial world load).
    pub fn flood_fill(priority: i32) -> Self {
        Self {
            id: JobId::new(),
            op: LightUpdateOp::FloodFill,
            source_pos: IVec3::ZERO,
            rgb: [0, 0, 0],
            priority,
            affected_chunks: None,
        }
    }

    /// Calculate priority based on distance to player position
    pub fn with_player_distance(mut self, player_pos: Vec3) -> Self {
        let distance = player_pos.distance(self.source_pos.as_vec3());
        self.priority = distance as i32;
        self
    }
}

// Implement ordering for priority queue (min-heap by priority)
impl PartialEq for LightUpdateJob {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for LightUpdateJob {}

impl PartialOrd for LightUpdateJob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LightUpdateJob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap (lower priority value = higher priority)
        other.priority.cmp(&self.priority)
    }
}

/// Result of a completed light update job
#[derive(Debug)]
pub struct LightUpdateResult {
    /// The job that was completed
    pub job: LightUpdateJob,

    /// List of chunk coordinates that were affected
    pub affected_chunks: Vec<IVec3>,

    /// Whether the job was cancelled
    pub cancelled: bool,

    /// Number of blocks processed
    pub blocks_processed: usize,

    /// Time taken to process (in microseconds)
    pub processing_time_us: u64,
}

/// Cancellation token for a light job
#[derive(Clone)]
pub struct LightCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl LightCancellationToken {
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

/// Frame budget configuration for light updates
#[derive(Debug, Clone, Copy)]
pub struct LightFrameBudget {
    /// Maximum number of blocks to process per frame
    pub max_blocks_per_frame: usize,

    /// Maximum time budget in microseconds (for monitoring)
    pub max_time_us: u64,
}

impl Default for LightFrameBudget {
    fn default() -> Self {
        Self {
            max_blocks_per_frame: 100, // Conservative default
            max_time_us: 2000,         // 2ms target
        }
    }
}

impl LightFrameBudget {
    /// Conservative budget for maintaining 60 FPS
    pub fn conservative() -> Self {
        Self {
            max_blocks_per_frame: 50,
            max_time_us: 1000,
        }
    }

    /// Balanced budget for typical gameplay
    pub fn balanced() -> Self {
        Self::default()
    }

    /// Aggressive budget for high-end hardware
    pub fn aggressive() -> Self {
        Self {
            max_blocks_per_frame: 200,
            max_time_us: 3000,
        }
    }
}

/// Light update job queue with frame budget
pub struct LightJobQueue {
    /// Pending jobs in priority order
    jobs: BinaryHeap<LightUpdateJob>,

    /// Cancellation tokens for jobs
    cancellation_tokens: HashMap<JobId, LightCancellationToken>,

    /// Completed results waiting to be collected
    results: VecDeque<LightUpdateResult>,

    /// Frame budget configuration
    budget: LightFrameBudget,

    /// Light propagator (reused across jobs)
    propagator: LightPropagator,

    /// Statistics
    total_jobs_processed: u64,
    total_blocks_processed: u64,
    total_time_us: u64,
}

impl LightJobQueue {
    /// Create a new light job queue
    pub fn new(chunk_size: i32, budget: LightFrameBudget) -> Self {
        Self {
            jobs: BinaryHeap::new(),
            cancellation_tokens: HashMap::new(),
            results: VecDeque::new(),
            budget,
            propagator: LightPropagator::new(chunk_size),
            total_jobs_processed: 0,
            total_blocks_processed: 0,
            total_time_us: 0,
        }
    }

    /// Create with default balanced budget
    pub fn with_default_budget(chunk_size: i32) -> Self {
        Self::new(chunk_size, LightFrameBudget::balanced())
    }

    /// Set the frame budget configuration
    pub fn set_budget(&mut self, budget: LightFrameBudget) {
        self.budget = budget;
    }

    /// Get current frame budget
    pub fn budget(&self) -> LightFrameBudget {
        self.budget
    }

    /// Submit a new light update job
    ///
    /// Returns the job ID for tracking
    pub fn submit(&mut self, job: LightUpdateJob) -> JobId {
        let job_id = job.id;
        let priority = job.priority;
        let source_pos = job.source_pos;
        let token = LightCancellationToken::new();

        self.cancellation_tokens.insert(job_id, token);
        self.jobs.push(job);

        log::debug!(
            "Submitted light job {:?} for position {:?} (priority {}, queue size: {})",
            job_id,
            source_pos,
            priority,
            self.jobs.len()
        );

        job_id
    }

    /// Cancel a pending job
    ///
    /// Returns true if the job was found and cancelled
    pub fn cancel(&mut self, job_id: JobId) -> bool {
        if let Some(token) = self.cancellation_tokens.get(&job_id) {
            token.cancel();
            log::debug!("Cancelled light job {:?}", job_id);
            true
        } else {
            false
        }
    }

    /// Process light updates for this frame within the budget
    ///
    /// Returns the number of jobs completed this frame
    pub fn process_frame(&mut self, grid: &mut VoxelGrid) -> usize {
        let mut jobs_completed = 0;
        let mut blocks_processed_this_frame = 0;

        let frame_start = std::time::Instant::now();

        // Process jobs until we hit the frame budget
        while blocks_processed_this_frame < self.budget.max_blocks_per_frame {
            // Get next job
            let job = match self.jobs.pop() {
                Some(job) => job,
                None => break, // No more jobs
            };

            // Check if cancelled
            let token = self.cancellation_tokens.get(&job.id);
            if token.map(|t| t.is_cancelled()).unwrap_or(false) {
                self.cancellation_tokens.remove(&job.id);
                self.results.push_back(LightUpdateResult {
                    job,
                    affected_chunks: Vec::new(),
                    cancelled: true,
                    blocks_processed: 0,
                    processing_time_us: 0,
                });
                jobs_completed += 1;
                continue;
            }

            // Process the job
            let job_start = std::time::Instant::now();
            let result = self.process_job(grid, job);
            let job_time_us = job_start.elapsed().as_micros() as u64;

            blocks_processed_this_frame += result.blocks_processed;

            // Update statistics
            self.total_jobs_processed += 1;
            self.total_blocks_processed += result.blocks_processed as u64;
            self.total_time_us += job_time_us;

            // Remove cancellation token
            self.cancellation_tokens.remove(&result.job.id);

            // Store result
            self.results.push_back(result);
            jobs_completed += 1;

            // Check if we should continue (budget check)
            if blocks_processed_this_frame >= self.budget.max_blocks_per_frame {
                break;
            }
        }

        let frame_time_us = frame_start.elapsed().as_micros() as u64;

        if jobs_completed > 0 {
            log::debug!(
                "Processed {} light jobs, {} blocks in {}µs (budget: {} blocks, {}µs)",
                jobs_completed,
                blocks_processed_this_frame,
                frame_time_us,
                self.budget.max_blocks_per_frame,
                self.budget.max_time_us
            );
        }

        jobs_completed
    }

    /// Process a single light update job
    fn process_job(&mut self, grid: &mut VoxelGrid, job: LightUpdateJob) -> LightUpdateResult {
        let start = std::time::Instant::now();

        let (affected_chunks, blocks_processed) = match job.op {
            LightUpdateOp::Add => {
                let chunks = self.propagator.add_light_rgb(grid, job.source_pos, job.rgb);
                let blocks = self.estimate_blocks_affected(&chunks);
                (chunks, blocks)
            }
            LightUpdateOp::Remove => {
                let chunks = self.propagator.remove_light(grid, job.source_pos);
                let blocks = self.estimate_blocks_affected(&chunks);
                (chunks, blocks)
            }
            LightUpdateOp::FloodFill => {
                let chunks = self.propagator.flood_fill_block_lights(grid);
                let blocks = self.estimate_blocks_affected(&chunks);
                (chunks, blocks)
            }
        };

        let processing_time_us = start.elapsed().as_micros() as u64;

        LightUpdateResult {
            job,
            affected_chunks,
            cancelled: false,
            blocks_processed,
            processing_time_us,
        }
    }

    /// Estimate number of blocks affected based on chunk count
    fn estimate_blocks_affected(&self, chunks: &[IVec3]) -> usize {
        // Rough estimate: each affected chunk has ~10-50 blocks updated
        // Use conservative estimate
        chunks.len() * 20
    }

    /// Collect completed results
    ///
    /// Returns all results that have completed since last collection
    pub fn collect_results(&mut self) -> Vec<LightUpdateResult> {
        self.results.drain(..).collect()
    }

    /// Get number of pending jobs
    pub fn pending_jobs(&self) -> usize {
        self.jobs.len()
    }

    /// Get statistics
    pub fn stats(&self) -> LightJobStats {
        LightJobStats {
            total_jobs_processed: self.total_jobs_processed,
            total_blocks_processed: self.total_blocks_processed,
            total_time_us: self.total_time_us,
            average_time_per_job_us: self
                .total_time_us
                .checked_div(self.total_jobs_processed)
                .unwrap_or(0),
            average_blocks_per_job: self
                .total_blocks_processed
                .checked_div(self.total_jobs_processed)
                .unwrap_or(0),
        }
    }
}

/// Statistics for light job processing
#[derive(Debug, Clone, Copy)]
pub struct LightJobStats {
    pub total_jobs_processed: u64,
    pub total_blocks_processed: u64,
    pub total_time_us: u64,
    pub average_time_per_job_us: u64,
    pub average_blocks_per_job: u64,
}

impl LightJobStats {
    /// Get average time per job in milliseconds
    pub fn average_time_ms(&self) -> f64 {
        self.average_time_per_job_us as f64 / 1000.0
    }

    /// Get total time in milliseconds
    pub fn total_time_ms(&self) -> f64 {
        self.total_time_us as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::grid::MaterialLighting;
    use crate::voxel::light_storage;

    fn transparent_grid() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        grid
    }

    #[test]
    fn test_job_priority_ordering() {
        let mut queue = BinaryHeap::new();

        let job1 = LightUpdateJob::add_light(IVec3::new(0, 0, 0), [15, 0, 0], 100);
        let job2 = LightUpdateJob::add_light(IVec3::new(10, 0, 0), [15, 0, 0], 50);
        let job3 = LightUpdateJob::add_light(IVec3::new(20, 0, 0), [15, 0, 0], 25);

        queue.push(job1);
        queue.push(job2);
        queue.push(job3);

        // Should pop in order: job3 (25), job2 (50), job1 (100)
        assert_eq!(queue.pop().unwrap().priority, 25);
        assert_eq!(queue.pop().unwrap().priority, 50);
        assert_eq!(queue.pop().unwrap().priority, 100);
    }

    #[test]
    fn test_job_cancellation() {
        let mut queue = LightJobQueue::with_default_budget(16);
        let job = LightUpdateJob::add_light(IVec3::new(0, 0, 0), [15, 0, 0], 10);
        let job_id = queue.submit(job);

        // First cancel should succeed
        assert!(queue.cancel(job_id));

        // Second cancel of same job should also return true (cancellation token still exists)
        assert!(queue.cancel(job_id));
    }

    #[test]
    fn test_add_light_job() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::with_default_budget(16);

        for x in 0..5i32 {
            for y in 0..5i32 {
                for z in 0..5i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        let light_pos = IVec3::new(2, 2, 2);
        let job = LightUpdateJob::add_light(light_pos, [15, 0, 0], 10);
        queue.submit(job);

        let completed = queue.process_frame(&mut grid);
        assert_eq!(completed, 1);

        // Verify R channel at source and neighbor.
        let (sc, si, _) = light_storage::world_to_chunk_local(light_pos);
        assert_eq!(grid.chunk_light(sc).unwrap().block_light_r[si], 15);

        let nb = IVec3::new(3, 2, 2);
        let (nc, ni, _) = light_storage::world_to_chunk_local(nb);
        assert!(grid.chunk_light(nc).unwrap().block_light_r[ni] > 0);

        let results = queue.collect_results();
        assert_eq!(results.len(), 1);
        assert!(!results[0].cancelled);
        assert!(!results[0].affected_chunks.is_empty());
    }

    #[test]
    fn test_remove_light_job() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::with_default_budget(16);

        for x in 0..5i32 {
            for y in 0..5i32 {
                for z in 0..5i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        // Add light via propagator directly.
        let light_pos = IVec3::new(2, 2, 2);
        let mut propagator = LightPropagator::new(16);
        propagator.add_light_rgb(&mut grid, light_pos, [15, 0, 0]);

        let (sc, si, _) = light_storage::world_to_chunk_local(light_pos);
        assert_eq!(grid.chunk_light(sc).unwrap().block_light_r[si], 15);

        let job = LightUpdateJob::remove_light(light_pos, 10);
        queue.submit(job);

        let completed = queue.process_frame(&mut grid);
        assert_eq!(completed, 1);

        assert_eq!(grid.chunk_light(sc).unwrap().block_light_r[si], 0);
    }

    #[test]
    fn test_frame_budget() {
        let mut grid = transparent_grid();

        let budget = LightFrameBudget::conservative();
        let mut queue = LightJobQueue::new(16, budget);

        for x in 0..10i32 {
            for y in 0..10i32 {
                for z in 0..10i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        for i in 0..5i32 {
            let job = LightUpdateJob::add_light(IVec3::new(i * 2, 5, 5), [15, 0, 0], i);
            queue.submit(job);
        }

        assert_eq!(queue.pending_jobs(), 5);

        let completed = queue.process_frame(&mut grid);
        assert!(completed <= 5);

        if queue.pending_jobs() > 0 {
            let completed2 = queue.process_frame(&mut grid);
            assert!(completed2 > 0);
        }
    }

    #[test]
    fn test_player_distance_priority() {
        let player_pos = Vec3::new(0.0, 0.0, 0.0);

        let job_close = LightUpdateJob::add_light(IVec3::new(5, 0, 0), [15, 0, 0], 100)
            .with_player_distance(player_pos);

        let job_far = LightUpdateJob::add_light(IVec3::new(50, 0, 0), [15, 0, 0], 100)
            .with_player_distance(player_pos);

        assert!(job_close.priority < job_far.priority);
    }

    #[test]
    fn test_stats_tracking() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::with_default_budget(16);

        for x in 0..5i32 {
            for y in 0..5i32 {
                for z in 0..5i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        for i in 0..3i32 {
            let job = LightUpdateJob::add_light(IVec3::new(i, 2, 2), [15, 0, 0], 10);
            queue.submit(job);
        }

        queue.process_frame(&mut grid);

        let stats = queue.stats();
        assert!(stats.total_jobs_processed > 0);
        assert!(stats.total_time_us > 0);
    }

    #[test]
    fn test_budget_presets() {
        let conservative = LightFrameBudget::conservative();
        let balanced = LightFrameBudget::balanced();
        let aggressive = LightFrameBudget::aggressive();

        assert!(conservative.max_blocks_per_frame < balanced.max_blocks_per_frame);
        assert!(balanced.max_blocks_per_frame < aggressive.max_blocks_per_frame);
    }
}
