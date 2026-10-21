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

    /// Handle a block placed event.
    pub fn on_block_placed(&mut self, position: IVec3, material_id: u32) {
        let emission = self.grid.material_registry.emission(material_id);
        if emission == [0u8, 0, 0] {
            // Opaque block placed — any existing light at this position must be removed.
            let opacity = self.grid.material_registry.opacity_cost(material_id);
            if opacity >= 15 {
                let job =
                    LightUpdateJob::remove_light(position, 0).with_player_distance(self.player_pos);
                self.job_queue.submit(job);
            }
        } else {
            let job = LightUpdateJob::add_light(position, emission, 0)
                .with_player_distance(self.player_pos);
            self.job_queue.submit(job);
        }
    }

    /// Handle a block removed event.
    pub fn on_block_removed(&mut self, position: IVec3, old_material_id: u32) {
        // Remove any block-light the removed block was contributing or blocking.
        let job = LightUpdateJob::remove_light(position, 0).with_player_distance(self.player_pos);
        self.job_queue.submit(job);

        tracing::debug!(
            pos = ?position,
            material_id = old_material_id,
            "Block removed, enqueued light removal job"
        );
    }

    /// Handle a batch block modification event
    ///
    /// Enqueues light update jobs for all affected positions
    pub fn on_blocks_batch_modified(&mut self, positions: &[IVec3], reason: &BlockChangeReason) {
        tracing::debug!(count = positions.len(), reason = ?reason, "Batch modification of blocks");

        // For batch operations, we can optimize by:
        // 1. Collecting all affected chunks
        // 2. Submitting fewer, larger jobs

        // For now, simple approach: submit individual jobs for each position
        for &position in positions {
            if let Some(mat_id) = self.grid.material_at(position) {
                let emission = self.grid.material_registry.emission(mat_id);
                let job = if emission == [0u8, 0, 0] {
                    LightUpdateJob::remove_light(position, 0)
                } else {
                    LightUpdateJob::add_light(position, emission, 0)
                };
                self.job_queue
                    .submit(job.with_player_distance(self.player_pos));
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

    /// Light chunks that have just entered the grid (world load, generation
    /// preload, streaming): refresh their sky exposure and block light.
    pub fn light_arrived_chunks(&mut self, _chunks: &[IVec3]) {}

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

        // Placing an emissive block should enqueue an add-light job.
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
        light_system.handle_event(&placed(source, 1));
        light_system.process_frame();
        light_system.emit_dirty_events();
        let lit = lit_chunks(light_system.grid(), source);
        received.lock().unwrap().clear();

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
        light_system.grid_mut().place_block(stone_pos, 2, None);
        for pos in [stone_pos, empty_pos] {
            LightPropagator::new().add_light_rgb(light_system.grid_mut(), pos, [10, 0, 0]);
        }
        let darkened = lit_chunks(light_system.grid(), stone_pos);

        light_system.handle_event(&WorldEvent::BlocksBatchModified {
            chunk_pos: IVec3::ZERO,
            positions: vec![torch_pos, stone_pos, empty_pos],
            reason: BlockChangeReason::Explosion,
        });
        assert_eq!(
            light_system.pending_jobs(),
            2,
            "no job for the empty position"
        );
        let completed = light_system.process_frame();

        assert_eq!(completed, 2);
        let grid = light_system.grid();
        assert_eq!(grid.block_light_rgb_at(torch_pos), TORCH);
        assert_eq!(grid.block_light_rgb_at(stone_pos), [0, 0, 0]);
        assert_eq!(
            grid.block_light_rgb_at(empty_pos),
            [10, 0, 0],
            "nothing queued for a position without a block"
        );
        let mut expected = lit_chunks(grid, torch_pos);
        expected.extend(darkened);
        light_system.emit_dirty_events();
        assert_published_once_each(&received, &expected);
    }

    #[test]
    fn placing_an_opaque_non_emissive_block_queues_one_removal_that_clears_its_light() {
        let (mut light_system, _received) = system();
        let pos = IVec3::new(5, 5, 5);
        LightPropagator::new().add_light_rgb(light_system.grid_mut(), pos, [10, 0, 0]);

        light_system.handle_event(&placed(pos, 0));

        assert_eq!(light_system.pending_jobs(), 1);
        light_system.process_frame();
        assert_eq!(light_system.grid().block_light_rgb_at(pos), [0, 0, 0]);
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
        light_system.handle_event(&placed(far, 1));
        light_system.handle_event(&placed(near, 1));

        let completed = light_system.process_frame();

        assert_eq!(completed, 1);
        assert_eq!(light_system.pending_jobs(), 1);
        assert_eq!(light_system.grid().block_light_rgb_at(near), TORCH);
        assert_eq!(light_system.grid().block_light_rgb_at(far), [0, 0, 0]);
    }

    fn unbounded_system() -> (LightSystem, DirtyLog) {
        system_with_budget(LightFrameBudget {
            max_blocks_per_frame: usize::MAX,
            max_time_us: u64::MAX,
        })
    }

    /// Runs one frame and publishes its dirty events, then forgets them, so a
    /// test starts from a settled world.
    fn settle(light_system: &mut LightSystem, received: &DirtyLog) {
        light_system.process_frame();
        light_system.emit_dirty_events();
        received.lock().unwrap().clear();
    }

    fn removed(position: IVec3) -> WorldEvent {
        WorldEvent::BlockRemoved {
            position,
            old_material_id: 0,
            reason: BlockChangeReason::Player,
        }
    }

    /// Digs `column` from `top` down to `bottom` (inclusive) the way the game
    /// does: edit the grid, then publish the removal.
    fn dig(light_system: &mut LightSystem, column: (i32, i32), top: i32, bottom: i32) {
        for y in (bottom..=top).rev() {
            let pos = IVec3::new(column.0, y, column.1);
            assert!(light_system.grid_mut().mutator().remove(pos));
            light_system.handle_event(&removed(pos));
        }
    }

    /// Solid ground in a 4x4 footprint, 34 blocks tall: chunk layers 0 and 1
    /// full, layer 2 holding two block layers.
    fn solid_ground(light_system: &mut LightSystem) {
        light_system
            .grid_mut()
            .mutator()
            .fill_region(IVec3::ZERO, IVec3::new(3, 33, 3), 0);
    }

    fn chunks_of(positions: &[IVec3]) -> HashSet<IVec3> {
        positions
            .iter()
            .map(|p| crate::light_storage::world_to_chunk_local(*p).0)
            .collect()
    }

    #[test]
    fn digging_a_shaft_exposes_its_floor_after_one_frame() {
        let (mut light_system, received) = unbounded_system();
        solid_ground(&mut light_system);
        settle(&mut light_system, &received);
        let floor = IVec3::new(1, 29, 1);
        assert!(!light_system.grid().sky_exposed_at(floor), "ground is dark");

        dig(&mut light_system, (1, 1), 33, 29);
        light_system.process_frame();
        light_system.emit_dirty_events();

        let grid = light_system.grid();
        assert!(grid.sky_exposed_at(floor), "the shaft floor sees the sky");
        assert!(grid.sky_exposed_at(IVec3::new(1, 33, 1)), "so does the top");
        assert!(
            !grid.sky_exposed_at(IVec3::new(1, 28, 1)),
            "the ground under the shaft stays shadowed"
        );
        assert!(
            !grid.sky_exposed_at(IVec3::new(2, 29, 1)),
            "the wall beside the shaft stays shadowed"
        );
        let crossed = chunks_of(&[IVec3::new(1, 33, 1), floor]);
        assert_eq!(crossed.len(), 2, "the shaft crosses a chunk border");
        assert_published_once_each(&received, &crossed);
    }

    #[test]
    fn sealing_a_shaft_hides_its_floor_from_the_sky() {
        let (mut light_system, received) = unbounded_system();
        solid_ground(&mut light_system);
        dig(&mut light_system, (1, 1), 33, 29);
        settle(&mut light_system, &received);
        let floor = IVec3::new(1, 29, 1);
        let cap = IVec3::new(1, 33, 1);
        assert!(light_system.grid().sky_exposed_at(floor), "open shaft");

        light_system.grid_mut().mutator().place(cap, 0, None);
        light_system.handle_event(&placed(cap, 0));
        light_system.process_frame();
        light_system.emit_dirty_events();

        let grid = light_system.grid();
        assert!(!grid.sky_exposed_at(floor), "the roof hides the floor");
        assert!(!grid.sky_exposed_at(IVec3::new(1, 31, 1)));
        assert!(
            grid.sky_exposed_at(IVec3::new(1, 34, 1)),
            "above the roof is open"
        );
        assert_published_once_each(&received, &chunks_of(&[cap, floor]));
    }

    #[test]
    fn sky_refresh_runs_top_down_across_chunk_layers() {
        let (mut light_system, received) = unbounded_system();
        let cap = IVec3::new(1, 4 * 16 + 3, 1);
        for layer in 0..=4 {
            // A filler block in another column keeps every layer loaded.
            let filler = IVec3::new(8, layer * 16 + 5, 8);
            light_system.grid_mut().mutator().place(filler, 0, None);
        }
        light_system.grid_mut().mutator().place(cap, 0, None);
        settle(&mut light_system, &received);
        let probes: Vec<IVec3> = (0..=4)
            .map(|layer| IVec3::new(1, layer * 16 + 1, 1))
            .collect();
        for probe in &probes {
            assert!(
                !light_system.grid().sky_exposed_at(*probe),
                "{probe:?} roofed"
            );
        }

        assert!(light_system.grid_mut().mutator().remove(cap));
        light_system.handle_event(&removed(cap));
        light_system.process_frame();
        light_system.emit_dirty_events();

        for probe in &probes {
            assert!(
                light_system.grid().sky_exposed_at(*probe),
                "{probe:?} sees the sky once the roof is gone"
            );
        }
        assert_published_once_each(&received, &chunks_of(&probes));
    }

    /// Brings the chunk at `chunk` into the grid the way streaming does: from
    /// serialized bytes, then handed to the light system. `build` places the
    /// chunk's blocks in a throwaway grid.
    fn arrive(light_system: &mut LightSystem, chunk: IVec3, build: impl FnOnce(&mut VoxelGrid)) {
        let mut donor = test_grid();
        build(&mut donor);
        let bytes = donor
            .serialize_chunk(chunk)
            .expect("the chunk holds blocks");
        light_system
            .grid_mut()
            .deserialize_chunk_into(chunk, &bytes)
            .expect("valid chunk bytes");
        light_system.light_arrived_chunks(&[chunk]);
    }

    #[test]
    fn arrived_chunk_with_an_emitter_lights_itself_and_its_neighbours() {
        let (mut light_system, received) = unbounded_system();
        let source = IVec3::new(15, 5, 5);
        let next_chunk = IVec3::new(1, 0, 0);

        arrive(&mut light_system, IVec3::ZERO, |donor| {
            donor.mutator().place(source, 1, None);
        });
        light_system.emit_dirty_events();

        let grid = light_system.grid();
        assert_eq!(grid.block_light_rgb_at(source), TORCH);
        assert_eq!(grid.block_light_rgb_at(IVec3::new(14, 5, 5)), [14, 7, 1]);
        assert_eq!(
            grid.block_light_rgb_at(IVec3::new(16, 5, 5)),
            [14, 7, 1],
            "light crosses into the next chunk"
        );
        assert_eq!(grid.block_light_rgb_at(IVec3::new(18, 5, 5)), [12, 5, 0]);
        let got = received.lock().unwrap().clone();
        let unique: HashSet<IVec3> = got.iter().copied().collect();
        assert_eq!(unique.len(), got.len(), "one event per chunk: {got:?}");
        assert!(unique.contains(&IVec3::ZERO), "the arrived chunk: {got:?}");
        assert!(unique.contains(&next_chunk), "the lit neighbour: {got:?}");
    }

    #[test]
    fn arrival_refills_from_lit_neighbours() {
        let (mut light_system, received) = unbounded_system();
        let neighbour = IVec3::new(1, 0, 0);
        LightPropagator::new().add_light_rgb(light_system.grid_mut(), IVec3::new(14, 5, 5), TORCH);
        light_system.grid_mut().remove_chunk(neighbour);
        assert_eq!(
            light_system.grid().block_light_rgb_at(IVec3::new(16, 5, 5)),
            [0, 0, 0],
            "the neighbour's light state is gone"
        );

        arrive(&mut light_system, neighbour, |donor| {
            donor.mutator().place(IVec3::new(30, 10, 10), 0, None);
        });
        light_system.emit_dirty_events();

        let grid = light_system.grid();
        assert_eq!(grid.block_light_rgb_at(IVec3::new(16, 5, 5)), [13, 6, 0]);
        assert_eq!(grid.block_light_rgb_at(IVec3::new(17, 5, 5)), [12, 5, 0]);
        assert!(received.lock().unwrap().contains(&neighbour));
    }

    #[test]
    fn arrival_clears_light_that_spread_before_the_blocks_loaded() {
        let (mut light_system, received) = unbounded_system();
        let wall_chunk = IVec3::new(1, 0, 0);
        LightPropagator::new().add_light_rgb(light_system.grid_mut(), IVec3::new(14, 5, 5), TORCH);
        assert_eq!(
            light_system.grid().block_light_rgb_at(IVec3::new(17, 5, 5)),
            [12, 5, 0],
            "light has spread into the unloaded chunk"
        );

        arrive(&mut light_system, wall_chunk, |donor| {
            donor
                .mutator()
                .fill_region(IVec3::new(16, 0, 0), IVec3::new(16, 15, 15), 0);
        });
        light_system.emit_dirty_events();

        let grid = light_system.grid();
        assert_eq!(grid.block_light_rgb_at(IVec3::new(16, 5, 5)), [0, 0, 0]);
        assert_eq!(
            grid.block_light_rgb_at(IVec3::new(17, 5, 5)),
            [0, 0, 0],
            "the wall now shades what lies behind it"
        );
        assert_eq!(
            grid.block_light_rgb_at(IVec3::new(15, 5, 5)),
            [14, 7, 1],
            "light on the open side is kept"
        );
        assert!(received.lock().unwrap().contains(&wall_chunk));
    }

    #[test]
    fn arrival_refreshes_sky_exposure_without_a_frame() {
        let (mut light_system, _received) = unbounded_system();
        let below = IVec3::ZERO;
        let above = IVec3::new(0, 1, 0);
        arrive(&mut light_system, below, |donor| {
            donor.mutator().place(IVec3::new(8, 2, 8), 0, None);
        });
        assert!(
            light_system.grid().sky_exposed_at(IVec3::new(1, 5, 1)),
            "open air in an arrived chunk sees the sky"
        );

        arrive(&mut light_system, above, |donor| {
            donor.mutator().place(IVec3::new(1, 20, 1), 0, None);
        });

        let grid = light_system.grid();
        assert!(
            !grid.sky_exposed_at(IVec3::new(1, 5, 1)),
            "the arriving roof shades the chunk below"
        );
        assert!(grid.sky_exposed_at(IVec3::new(1, 25, 1)));
        assert!(grid.sky_exposed_at(IVec3::new(8, 18, 8)));
        assert!(!grid.sky_exposed_at(IVec3::new(1, 18, 1)));
    }
}
