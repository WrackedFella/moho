//! Async light update job queue with priority scheduling.
//!
//! This module provides an async job queue for light propagation that:
//! - Prioritizes light updates by distance to player
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
use glam::{IVec3, Vec3};
use std::collections::{BinaryHeap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for light update jobs
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

/// A light update job: reconcile the light at one position with the grid.
#[derive(Debug, Clone)]
pub struct LightUpdateJob {
    /// Unique identifier for this job
    pub id: JobId,

    /// The position whose light is reconciled with the grid.
    pub source_pos: BlockPos,

    /// Priority (lower = higher priority, typically distance to player)
    pub priority: i32,
}

impl LightUpdateJob {
    /// Create a job that reconciles the light at `source_pos` with the grid.
    pub fn at(source_pos: BlockPos) -> Self {
        Self {
            id: JobId::new(),
            source_pos,
            priority: 0,
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

    /// Number of blocks processed
    pub blocks_processed: usize,
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

/// Light update job queue with frame budget
pub struct LightJobQueue {
    /// Pending jobs in priority order
    jobs: BinaryHeap<LightUpdateJob>,

    /// Completed results waiting to be collected
    results: VecDeque<LightUpdateResult>,

    /// Frame budget configuration
    budget: LightFrameBudget,

    /// Light propagator (reused across jobs)
    propagator: LightPropagator,
}

impl LightJobQueue {
    /// Create a new light job queue
    pub fn new(budget: LightFrameBudget) -> Self {
        Self {
            jobs: BinaryHeap::new(),
            results: VecDeque::new(),
            budget,
            propagator: LightPropagator::new(),
        }
    }

    /// Submit a new light update job
    ///
    /// Returns the job ID for tracking
    pub fn submit(&mut self, job: LightUpdateJob) -> JobId {
        let job_id = job.id;
        let priority = job.priority;
        let source_pos = job.source_pos;

        self.jobs.push(job);

        tracing::debug!(
            job = ?job_id,
            pos = ?source_pos,
            priority,
            queue_size = self.jobs.len(),
            "Submitted light job"
        );

        job_id
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
            let Some(job) = self.jobs.pop() else { break };

            let result = self.process_job(grid, job);

            blocks_processed_this_frame += result.blocks_processed;

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
            tracing::debug!(
                jobs_completed = jobs_completed,
                blocks_processed = blocks_processed_this_frame,
                frame_time_us = frame_time_us,
                budget_blocks = self.budget.max_blocks_per_frame,
                budget_time_us = self.budget.max_time_us,
                "Processed light jobs"
            );
        }

        jobs_completed
    }

    /// Process a single light update job
    fn process_job(&mut self, grid: &mut VoxelGrid, job: LightUpdateJob) -> LightUpdateResult {
        let affected_chunks = self.propagator.update_at(grid, job.source_pos);
        let blocks_processed = self.estimate_blocks_affected(&affected_chunks);

        LightUpdateResult {
            job,
            affected_chunks,
            blocks_processed,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::MaterialLighting;
    use crate::light_storage;
    use crate::materials::MaterialType;

    #[test]
    fn test_job_id_uniqueness() {
        let id1 = JobId::new();
        let id2 = JobId::new();
        let id3 = JobId::new();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    fn transparent_grid() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        grid.material_registry.set_lighting(
            2,
            MaterialLighting {
                emission: [15, 0, 0],
                opacity_cost: 0,
            },
        );
        let dim = grid.material_registry.register(MaterialType::Lambertian {
            albedo: Vec3::splat(0.5),
        });
        assert_eq!(dim, DIM, "the registry starts with ids 0 to 2");
        grid.material_registry.set_lighting(
            dim,
            MaterialLighting {
                emission: [1, 0, 0],
                opacity_cost: 0,
            },
        );
        grid
    }

    /// Material 2 emits red at 15, material 3 at 1; both are transparent.
    const BRIGHT: u32 = 2;
    const DIM: u32 = 3;

    fn job(pos: IVec3, priority: i32) -> LightUpdateJob {
        LightUpdateJob {
            priority,
            ..LightUpdateJob::at(pos)
        }
    }

    fn unlimited_budget() -> LightFrameBudget {
        LightFrameBudget {
            max_blocks_per_frame: usize::MAX,
            max_time_us: u64::MAX,
        }
    }

    #[test]
    fn test_job_priority_ordering() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::new(unlimited_budget());

        for (x, priority) in [(0, 50), (10, 100), (20, 25)] {
            let pos = IVec3::new(x, 0, 0);
            grid.place_block(pos, BRIGHT, None);
            queue.submit(job(pos, priority));
        }
        queue.process_frame(&mut grid);

        let priorities: Vec<i32> = queue
            .collect_results()
            .iter()
            .map(|r| r.job.priority)
            .collect();
        assert_eq!(priorities, [25, 50, 100]);
    }

    #[test]
    fn test_add_light_job() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::new(LightFrameBudget::default());

        for x in 0..5i32 {
            for y in 0..5i32 {
                for z in 0..5i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        let light_pos = IVec3::new(2, 2, 2);
        grid.place_block(light_pos, BRIGHT, None);
        queue.submit(job(light_pos, 10));

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
    }

    #[test]
    fn test_remove_light_job() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::new(LightFrameBudget::default());

        for x in 0..5i32 {
            for y in 0..5i32 {
                for z in 0..5i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        let light_pos = IVec3::new(2, 2, 2);
        grid.place_block(light_pos, BRIGHT, None);
        let mut propagator = LightPropagator::new();
        propagator.add_light_rgb(&mut grid, light_pos, [15, 0, 0]);

        let (sc, si, _) = light_storage::world_to_chunk_local(light_pos);
        assert_eq!(grid.chunk_light(sc).unwrap().block_light_r[si], 15);

        grid.mutator().remove(light_pos);
        queue.submit(job(light_pos, 10));

        let completed = queue.process_frame(&mut grid);
        assert_eq!(completed, 1);

        assert_eq!(grid.chunk_light(sc).unwrap().block_light_r[si], 0);
    }

    #[test]
    fn test_frame_budget() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::new(LightFrameBudget {
            max_blocks_per_frame: 1,
            max_time_us: u64::MAX,
        });
        for i in 0..3i32 {
            grid.place_block(IVec3::new(i, 0, 0), BRIGHT, None);
            queue.submit(job(IVec3::new(i, 0, 0), i));
        }

        let first = queue.process_frame(&mut grid);
        let pending_after_first = queue.pending_jobs();
        let second = queue.process_frame(&mut grid);
        let third = queue.process_frame(&mut grid);

        assert_eq!(first, 1);
        assert_eq!(pending_after_first, 2);
        assert_eq!(second, 1);
        assert_eq!(third, 1);
        assert_eq!(queue.pending_jobs(), 0);
    }

    /// A dim emitter lights one voxel, so each finished job is charged one chunk.
    fn single_chunk_job(grid: &mut VoxelGrid, x: i32) -> LightUpdateJob {
        let pos = IVec3::new(x, 8, 8);
        grid.place_block(pos, DIM, None);
        job(pos, x)
    }

    fn budget_of(max_blocks_per_frame: usize) -> LightFrameBudget {
        LightFrameBudget {
            max_blocks_per_frame,
            max_time_us: u64::MAX,
        }
    }

    #[test]
    fn frame_budget_leaves_excess_jobs_queued() {
        // (budget in blocks, jobs finished in one frame); a finished job costs 20 per chunk.
        for (budget, finished) in [(19, 1), (20, 1), (21, 2), (40, 2), (41, 3)] {
            let mut grid = transparent_grid();
            let mut queue = LightJobQueue::new(budget_of(budget));
            for x in 0..4 {
                let job = single_chunk_job(&mut grid, x);
                queue.submit(job);
            }

            let completed = queue.process_frame(&mut grid);

            assert_eq!(completed, finished, "budget {budget}");
            assert_eq!(queue.pending_jobs(), 4 - finished, "budget {budget}");
        }
    }

    #[test]
    fn zero_budget_processes_nothing_and_keeps_the_job_queued() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::new(budget_of(0));
        let job = single_chunk_job(&mut grid, 0);
        queue.submit(job);

        let completed = queue.process_frame(&mut grid);

        assert_eq!(completed, 0);
        assert_eq!(queue.pending_jobs(), 1);
        assert_eq!(grid.block_light_rgb_at(IVec3::new(0, 8, 8)), [0, 0, 0]);
    }

    /// Chunks holding any lit voxel within 16 blocks of `center`.
    fn lit_chunks(grid: &VoxelGrid, center: IVec3) -> std::collections::HashSet<IVec3> {
        let mut chunks = std::collections::HashSet::new();
        for x in -16..=16 {
            for y in -16..=16 {
                for z in -16..=16 {
                    let pos = center + IVec3::new(x, y, z);
                    if grid.block_light_rgb_at(pos) != [0, 0, 0] {
                        chunks.insert(light_storage::world_to_chunk_local(pos).0);
                    }
                }
            }
        }
        chunks
    }

    fn assert_affected_chunks(
        result: &LightUpdateResult,
        expected: &std::collections::HashSet<IVec3>,
    ) {
        assert!(
            expected.len() > 1,
            "the source must sit where light spills over"
        );
        assert_eq!(
            result.affected_chunks.len(),
            expected.len(),
            "no duplicates"
        );
        let got: std::collections::HashSet<IVec3> =
            result.affected_chunks.iter().copied().collect();
        assert_eq!(&got, expected);
    }

    #[test]
    fn affected_chunks_of_an_add_are_the_chunks_it_lit() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::new(LightFrameBudget::default());
        let source = IVec3::new(15, 15, 15);
        grid.place_block(source, BRIGHT, None);
        queue.submit(job(source, 0));

        queue.process_frame(&mut grid);

        let results = queue.collect_results();
        assert_eq!(results.len(), 1);
        assert_affected_chunks(&results[0], &lit_chunks(&grid, source));
    }

    #[test]
    fn affected_chunks_of_a_remove_are_the_chunks_it_darkened() {
        let mut grid = transparent_grid();
        let mut queue = LightJobQueue::new(LightFrameBudget::default());
        let source = IVec3::new(15, 15, 15);
        grid.place_block(source, BRIGHT, None);
        LightPropagator::new().add_light_rgb(&mut grid, source, [15, 0, 0]);
        let lit_before = lit_chunks(&grid, source);
        grid.mutator().remove(source);
        queue.submit(job(source, 0));

        queue.process_frame(&mut grid);

        assert!(lit_chunks(&grid, source).is_empty());
        let results = queue.collect_results();
        assert_eq!(results.len(), 1);
        assert_affected_chunks(&results[0], &lit_before);
    }

    #[test]
    fn jobs_are_equal_by_id_not_by_content() {
        let first = job(IVec3::new(1, 2, 3), 7);
        let twin = job(IVec3::new(1, 2, 3), 7);

        assert_eq!(first, first.clone());
        assert_ne!(first, twin);
    }

    #[test]
    fn test_player_distance_priority() {
        let player_pos = Vec3::new(0.0, 0.0, 0.0);

        let job_close = job(IVec3::new(5, 0, 0), 100).with_player_distance(player_pos);

        let job_far = job(IVec3::new(50, 0, 0), 100).with_player_distance(player_pos);

        assert!(job_close.priority < job_far.priority);
    }
}
