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
//! use flagstone_voxel::{LightSystem, VoxelGrid, LightFrameBudget};
//! use moho_core::events::EventBus;
//!
//! let grid = VoxelGrid::new(16);
//! let event_bus = EventBus::new();
//! let mut light_system = LightSystem::new(grid, event_bus, LightFrameBudget::default());
//!
//! // Process light updates each frame
//! light_system.process_frame();
//!
//! // Notify the mesh system of chunks whose light changed
//! let dirtied = light_system.emit_dirty_events();
//! ```

use super::grid::VoxelGrid;
use super::light_jobs::{LightFrameBudget, LightJobQueue, LightUpdateJob};
use crate::events::{BlockChangeReason, WorldEvent};
use glam::{IVec3, Vec3};
use moho_core::events::EventBus;
use std::collections::HashSet;
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
}

impl LightSystem {
    /// Create a new light system
    ///
    /// # Arguments
    /// * `grid` - The voxel grid to propagate light through
    /// * `event_bus` - Event bus for listening and publishing events
    /// * `budget` - Frame budget configuration
    pub fn new(grid: VoxelGrid, event_bus: Arc<EventBus>, budget: LightFrameBudget) -> Self {
        let job_queue = LightJobQueue::new(budget);

        Self {
            grid,
            job_queue,
            event_bus,
            player_pos: Vec3::ZERO,
            affected_chunks: HashSet::new(),
        }
    }

    /// A light system with [`LightFrameBudget::default`].
    pub fn with_default_budget(grid: VoxelGrid, event_bus: Arc<EventBus>) -> Self {
        Self::new(grid, event_bus, LightFrameBudget::default())
    }

    /// Update player position for priority calculation
    pub fn set_player_position(&mut self, pos: Vec3) {
        self.player_pos = pos;
    }

    /// Whether a block of `material_id` (`None` for no block) can change any
    /// light: it emits, or it is opaque, or the position is empty.
    fn affects_light(&self, material_id: Option<u32>) -> bool {
        material_id.is_none_or(|id| {
            let lighting = self.grid.material_registry.lighting(id);
            lighting.is_emissive() || lighting.is_opaque()
        })
    }

    fn submit_reconcile(&mut self, position: IVec3) {
        let job = LightUpdateJob::at(position).with_player_distance(self.player_pos);
        self.job_queue.submit(job);
    }

    /// Handle a block placed event. The job reads the grid when it runs, so the
    /// block must already be in the grid. A transparent, non-emissive block queues
    /// nothing, which assumes it was placed into an empty position.
    pub fn on_block_placed(&mut self, position: IVec3, material_id: u32) {
        if self.affects_light(Some(material_id)) {
            self.submit_reconcile(position);
        }
    }

    /// Handle a block removed event.
    pub fn on_block_removed(&mut self, position: IVec3, old_material_id: u32) {
        self.submit_reconcile(position);

        tracing::debug!(
            pos = ?position,
            material_id = old_material_id,
            "Block removed, enqueued light reconcile job"
        );
    }

    /// Handle a batch block modification event
    ///
    /// Enqueues a reconcile job for every position the edit left empty, opaque or emissive.
    pub fn on_blocks_batch_modified(&mut self, positions: &[IVec3], reason: &BlockChangeReason) {
        tracing::debug!(count = positions.len(), reason = ?reason, "Batch modification of blocks");

        for &position in positions {
            if self.affects_light(self.grid.material_at(position)) {
                self.submit_reconcile(position);
            }
        }
    }

    /// Process light update jobs for this frame
    ///
    /// Returns the number of jobs completed this frame
    pub fn process_frame(&mut self) -> usize {
        let completed = self.job_queue.process_frame(&mut self.grid);

        self.affected_chunks.extend(
            self.job_queue
                .collect_results()
                .into_iter()
                .flat_map(|r| r.affected_chunks),
        );

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
        // Light shades both terrain and structure meshes.
        for chunk_pos in self.affected_chunks.drain() {
            self.event_bus.publish(WorldEvent::ChunkMeshDirty {
                chunk_pos,
                terrain_dirty: true,
                structure_dirty: true,
            });
        }
        count
    }

    /// Get number of pending jobs
    pub fn pending_jobs(&self) -> usize {
        self.job_queue.pending_jobs()
    }

    /// Queue light work for a world event. The grid must already hold the edit
    /// the event describes.
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
            WorldEvent::ChunkMeshDirty { .. } => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::light_propagation::LightPropagator;

    #[test]
    fn test_block_placed_light_source() {
        use crate::grid::MaterialLighting;
        let mut grid = VoxelGrid::new(16);
        // Material 1: warm torch emission.
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [15, 8, 2],
                opacity_cost: 0,
            },
        );

        let event_bus = Arc::new(EventBus::new());
        let mut light_system = LightSystem::with_default_budget(grid, event_bus);

        // Placing an emissive block should enqueue a job.
        light_system.on_block_placed(IVec3::new(2, 2, 2), 1);
        assert_eq!(light_system.pending_jobs(), 1);
    }

    const TORCH: [u8; 3] = [15, 8, 2];

    /// Chunks holding any lit voxel within 16 blocks of `center`.
    fn lit_chunks(grid: &VoxelGrid, center: IVec3) -> HashSet<IVec3> {
        let mut chunks = HashSet::new();
        for x in -16..=16 {
            for y in -16..=16 {
                for z in -16..=16 {
                    let pos = center + IVec3::new(x, y, z);
                    if grid.block_light_rgb_at(pos) != [0, 0, 0] {
                        chunks.insert(crate::light_storage::world_to_chunk_local(pos).0);
                    }
                }
            }
        }
        chunks
    }

    /// Material 0 is an opaque block (the default), 1 a torch, 2 a transparent
    /// block with no emission; the registry only holds ids 0 to 2.
    fn test_grid() -> VoxelGrid {
        use crate::grid::MaterialLighting;
        let mut grid = VoxelGrid::new(16);
        for (id, emission, opacity_cost) in [(1, TORCH, 0), (2, [0, 0, 0], 0)] {
            grid.material_registry.set_lighting(
                id,
                MaterialLighting {
                    emission,
                    opacity_cost,
                },
            );
        }
        grid
    }

    type DirtyLog = Arc<std::sync::Mutex<Vec<IVec3>>>;

    fn system_with_budget(budget: LightFrameBudget) -> (LightSystem, DirtyLog) {
        let event_bus = Arc::new(EventBus::new());
        let received: DirtyLog = Arc::default();
        let sink = Arc::clone(&received);
        event_bus.subscribe(move |event: &WorldEvent| {
            if let WorldEvent::ChunkMeshDirty { chunk_pos, .. } = event {
                sink.lock().unwrap().push(*chunk_pos);
            }
        });
        (LightSystem::new(test_grid(), event_bus, budget), received)
    }

    fn system() -> (LightSystem, DirtyLog) {
        system_with_budget(LightFrameBudget::default())
    }

    fn placed(position: IVec3, material_id: u32) -> WorldEvent {
        WorldEvent::BlockPlaced {
            position,
            material_id,
            reason: BlockChangeReason::Player,
        }
    }

    fn assert_published_once_each(received: &DirtyLog, expected: &HashSet<IVec3>) {
        let got = received.lock().unwrap().clone();
        assert_eq!(got.len(), expected.len(), "one event per chunk: {got:?}");
        assert_eq!(&got.into_iter().collect::<HashSet<_>>(), expected);
    }

    #[test]
    fn placing_an_emissive_block_emits_dirty_events_for_reached_chunks() {
        let (mut light_system, received) = system();
        let source = IVec3::new(15, 15, 15);
        light_system.grid_mut().place_block(source, 1, None);

        light_system.handle_event(&placed(source, 1));
        let completed = light_system.process_frame();
        let emitted = light_system.emit_dirty_events();

        assert_eq!(completed, 1);
        assert_eq!(light_system.pending_jobs(), 0, "the frame drains the queue");
        assert_eq!(light_system.grid().block_light_rgb_at(source), TORCH);
        let expected = lit_chunks(light_system.grid(), source);
        assert!(
            expected.len() > 1,
            "light must spill into neighbouring chunks"
        );
        assert_eq!(emitted, expected.len());
        assert_published_once_each(&received, &expected);
        assert_eq!(light_system.emit_dirty_events(), 0);
        assert_eq!(received.lock().unwrap().len(), expected.len());
    }

    #[test]
    fn removing_an_emissive_block_darkens_it_and_emits_dirty_events_for_those_chunks() {
        let (mut light_system, received) = system();
        let source = IVec3::new(15, 15, 15);
        light_system.grid_mut().place_block(source, 1, None);
        light_system.handle_event(&placed(source, 1));
        light_system.process_frame();
        light_system.emit_dirty_events();
        let lit = lit_chunks(light_system.grid(), source);
        assert_eq!(
            light_system.grid().block_light_rgb_at(source),
            TORCH,
            "the emitter starts lit"
        );
        assert!(!lit.is_empty(), "the removal starts from a lit area");
        received.lock().unwrap().clear();
        light_system.grid_mut().mutator().remove(source);

        light_system.handle_event(&WorldEvent::BlockRemoved {
            position: source,
            old_material_id: 1,
            reason: BlockChangeReason::Player,
        });
        let completed = light_system.process_frame();
        light_system.emit_dirty_events();

        assert_eq!(completed, 1);
        assert_eq!(light_system.grid().block_light_rgb_at(source), [0, 0, 0]);
        assert!(lit_chunks(light_system.grid(), source).is_empty());
        assert_published_once_each(&received, &lit);
    }

    #[test]
    fn a_batch_lights_emissive_blocks_and_clears_light_at_non_emissive_ones() {
        let (mut light_system, received) = system_with_budget(LightFrameBudget {
            max_blocks_per_frame: usize::MAX,
            max_time_us: u64::MAX,
        });
        let torch_pos = IVec3::new(5, 5, 5);
        let stone_pos = IVec3::new(40, 5, 5);
        let empty_pos = IVec3::new(70, 5, 5);
        light_system.grid_mut().place_block(torch_pos, 1, None);
        light_system.grid_mut().place_block(stone_pos, 0, None);
        for pos in [stone_pos, empty_pos] {
            LightPropagator::new().add_light_rgb(light_system.grid_mut(), pos, [10, 0, 0]);
        }
        let darkened = lit_chunks(light_system.grid(), stone_pos);
        let darkened_empty = lit_chunks(light_system.grid(), empty_pos);

        light_system.handle_event(&WorldEvent::BlocksBatchModified {
            chunk_pos: IVec3::ZERO,
            positions: vec![torch_pos, stone_pos, empty_pos],
            reason: BlockChangeReason::Explosion,
        });
        assert_eq!(
            light_system.pending_jobs(),
            3,
            "one job per position, the empty one included"
        );
        let completed = light_system.process_frame();

        assert_eq!(completed, 3);
        let grid = light_system.grid();
        assert_eq!(grid.block_light_rgb_at(torch_pos), TORCH);
        assert_eq!(grid.block_light_rgb_at(stone_pos), [0, 0, 0]);
        assert_eq!(
            grid.block_light_rgb_at(empty_pos),
            [0, 0, 0],
            "light at a position emptied by the edit is reconciled away"
        );
        let mut expected = lit_chunks(grid, torch_pos);
        expected.extend(darkened);
        expected.extend(darkened_empty);
        light_system.emit_dirty_events();
        assert_published_once_each(&received, &expected);
    }

    #[test]
    fn a_batch_queues_every_changed_position() {
        let (mut light_system, _received) = system_with_budget(LightFrameBudget {
            max_blocks_per_frame: usize::MAX,
            max_time_us: u64::MAX,
        });
        let torch_pos = IVec3::new(5, 5, 5);
        let lit_stone_pos = IVec3::new(7, 5, 5);
        let stone_pos = IVec3::new(40, 5, 5);
        let empty_positions = [IVec3::new(70, 5, 5), IVec3::new(85, 5, 5)];
        let glass_positions = [
            IVec3::new(100, 5, 5),
            IVec3::new(115, 5, 5),
            IVec3::new(130, 5, 5),
        ];
        light_system.grid_mut().place_block(torch_pos, 1, None);
        LightPropagator::new().add_light_rgb(light_system.grid_mut(), torch_pos, TORCH);
        light_system.grid_mut().place_block(lit_stone_pos, 0, None);
        light_system.grid_mut().place_block(stone_pos, 0, None);
        for pos in glass_positions {
            light_system.grid_mut().place_block(pos, 2, None);
        }
        assert_ne!(
            light_system.grid().block_light_rgb_at(lit_stone_pos),
            [0, 0, 0],
            "the stone is placed into real light"
        );
        let mut positions = vec![torch_pos, lit_stone_pos, stone_pos];
        positions.extend(empty_positions);
        positions.extend(glass_positions);

        light_system.handle_event(&WorldEvent::BlocksBatchModified {
            chunk_pos: IVec3::ZERO,
            positions,
            reason: BlockChangeReason::Explosion,
        });

        assert_eq!(
            light_system.pending_jobs(),
            5,
            "emitter, two opaque and two emptied positions queue; the three transparent placements do not"
        );
        assert_eq!(light_system.process_frame(), 5);
        assert_eq!(
            light_system.grid().block_light_rgb_at(lit_stone_pos),
            [0, 0, 0],
            "the stone placed into light is darkened"
        );
    }

    /// Place an emitter at `pos`, mine it again, and publish both events in the
    /// given order before one frame runs; a distant torch proves jobs do run.
    fn place_and_mine_before_processing(removed_event_first: bool) {
        let (mut light_system, _received) = system();
        let pos = IVec3::new(5, 5, 5);
        let witness = IVec3::new(45, 5, 5);
        light_system.grid_mut().place_block(witness, 1, None);
        light_system.handle_event(&placed(witness, 1));
        light_system.grid_mut().place_block(pos, 1, None);
        let place = placed(pos, 1);
        let remove = WorldEvent::BlockRemoved {
            position: pos,
            old_material_id: 1,
            reason: BlockChangeReason::Player,
        };

        let (first, second) = if removed_event_first {
            (&remove, &place)
        } else {
            (&place, &remove)
        };
        light_system.handle_event(first);
        light_system.handle_event(second);
        light_system.grid_mut().mutator().remove(pos);
        light_system.process_frame();

        let grid = light_system.grid();
        assert_eq!(
            grid.block_light_rgb_at(witness),
            TORCH,
            "the witness torch is lit"
        );
        assert!(
            lit_chunks(grid, pos).is_empty(),
            "the mined torch leaves no light (removed event first: {removed_event_first})"
        );
    }

    #[test]
    fn place_then_remove_before_processing_leaves_no_light() {
        place_and_mine_before_processing(false);
        place_and_mine_before_processing(true);
    }

    #[test]
    fn placing_an_opaque_non_emissive_block_queues_one_job_that_clears_its_light() {
        let (mut light_system, _received) = system();
        let pos = IVec3::new(5, 5, 5);
        let emitter = IVec3::new(6, 5, 5);
        light_system.grid_mut().place_block(emitter, 1, None);
        LightPropagator::new().add_light_rgb(light_system.grid_mut(), emitter, TORCH);
        light_system.grid_mut().place_block(pos, 0, None);
        assert_ne!(
            light_system.grid().block_light_rgb_at(pos),
            [0, 0, 0],
            "the block is placed into real light"
        );

        light_system.handle_event(&placed(pos, 0));

        assert_eq!(light_system.pending_jobs(), 1);
        light_system.process_frame();
        assert_eq!(light_system.grid().block_light_rgb_at(pos), [0, 0, 0]);
        assert_eq!(light_system.grid().block_light_rgb_at(emitter), TORCH);
    }

    #[test]
    fn placing_a_transparent_non_emissive_block_queues_nothing() {
        let (mut light_system, _received) = system();
        let pos = IVec3::new(5, 5, 5);
        LightPropagator::new().add_light_rgb(light_system.grid_mut(), pos, [10, 0, 0]);

        light_system.handle_event(&placed(pos, 2));

        assert_eq!(light_system.pending_jobs(), 0);
        light_system.process_frame();
        assert_eq!(light_system.grid().block_light_rgb_at(pos), [10, 0, 0]);
    }

    #[test]
    fn a_chunk_mesh_dirty_event_queues_nothing() {
        let (mut light_system, _received) = system();

        light_system.handle_event(&WorldEvent::ChunkMeshDirty {
            chunk_pos: IVec3::ZERO,
            terrain_dirty: true,
            structure_dirty: true,
        });

        assert_eq!(light_system.pending_jobs(), 0);
    }

    #[test]
    fn the_job_nearest_the_player_is_processed_first() {
        let (mut light_system, _received) = system_with_budget(LightFrameBudget {
            max_blocks_per_frame: 1,
            max_time_us: u64::MAX,
        });
        let far = IVec3::new(0, 8, 8);
        let near = IVec3::new(100, 8, 8);
        light_system.set_player_position(Vec3::new(100.0, 8.0, 8.0));
        light_system.grid_mut().place_block(far, 1, None);
        light_system.grid_mut().place_block(near, 1, None);
        light_system.handle_event(&placed(far, 1));
        light_system.handle_event(&placed(near, 1));

        let completed = light_system.process_frame();

        assert_eq!(completed, 1);
        assert_eq!(light_system.pending_jobs(), 1);
        assert_eq!(light_system.grid().block_light_rgb_at(near), TORCH);
        assert_eq!(light_system.grid().block_light_rgb_at(far), [0, 0, 0]);
    }
}
