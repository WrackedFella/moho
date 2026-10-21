//! RGB block-light propagation.
//!
//! Implements per-channel BFS flood-fill for three independent R/G/B block-light
//! channels. Each channel decays by 1 per step and is blocked by fully-opaque
//! voxels (opacity_cost == 15). Air (no block present) is transparent.
//!
//! Sky exposure is NOT handled here; that is the binary `sky_exposed` bit
//! computed by the `light_sky` pass.
//!
//! # Algorithms
//! - **add_light_rgb**: standard BFS; only updates voxels where new level > stored.
//! - **remove_light**: two-queue removal — dark wave clears source-lit voxels,
//!   then re-seeds cleared emitter blocks from their emission and relights from
//!   adjacent voxels still lit by surviving sources.

use super::grid::VoxelGrid;
use super::light_storage::{
    self, CHUNK_SIZE, CHUNK_VOLUME, ChunkLight, border_offsets, idx_to_local, local_index,
};
use glam::IVec3;
use std::collections::{HashSet, VecDeque};

/// One of the three block-light color channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightChannel {
    R = 0,
    G = 1,
    B = 2,
}

#[derive(Debug, Clone, Copy)]
struct PropNode {
    chunk_pos: IVec3,
    idx: usize,
    level: u8,
}

const CHANNELS: [LightChannel; 3] = [LightChannel::R, LightChannel::G, LightChannel::B];

const OFFSETS: [IVec3; 6] = [
    IVec3::new(1, 0, 0),
    IVec3::new(-1, 0, 0),
    IVec3::new(0, 1, 0),
    IVec3::new(0, -1, 0),
    IVec3::new(0, 0, 1),
    IVec3::new(0, 0, -1),
];

/// Propagates RGB block-light through the voxel grid via per-channel BFS.
pub struct LightPropagator {
    queue: VecDeque<PropNode>,
}

impl Default for LightPropagator {
    fn default() -> Self {
        Self::new()
    }
}

impl LightPropagator {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::with_capacity(1024),
        }
    }

    /// Propagate RGB block-light outward from `pos` with emission `rgb`.
    /// Runs three independent BFS passes (R → G → B).
    /// Returns the set of chunk positions that were modified.
    pub fn add_light_rgb(&mut self, grid: &mut VoxelGrid, pos: IVec3, rgb: [u8; 3]) -> Vec<IVec3> {
        if !grid.supports_lighting() {
            return vec![];
        }
        let mut affected = HashSet::new();
        self.add_rgb(grid, pos, rgb, &mut affected);
        affected.into_iter().collect()
    }

    /// Remove the block-light contributed by the source at `pos` using two-queue
    /// removal. Each channel is processed independently.
    /// The block at `pos` must already be out of the grid: every emitter block the
    /// dark wave clears, `pos` included, is re-seeded from its emission.
    /// Returns the set of chunk positions that were modified.
    pub fn remove_light(&mut self, grid: &mut VoxelGrid, pos: IVec3) -> Vec<IVec3> {
        if !grid.supports_lighting() {
            return vec![];
        }
        let mut affected = HashSet::new();
        for ch in [LightChannel::R, LightChannel::G, LightChannel::B] {
            self.two_queue_remove(grid, pos, ch, &mut affected);
        }
        affected.into_iter().collect()
    }

    /// Scan all blocks for non-zero emission and flood-fill their RGB light.
    /// Used for initial world-load or full-grid recompute.
    pub fn flood_fill_block_lights(&mut self, grid: &mut VoxelGrid) -> Vec<IVec3> {
        if !grid.supports_lighting() {
            return vec![];
        }
        let positions: Vec<IVec3> = grid.block_positions().collect();
        let mut affected = HashSet::new();
        self.seed_emitters(grid, &positions, &mut affected);
        affected.into_iter().collect()
    }

    /// Block light for chunks that just entered the grid: clears light that
    /// spread into their area before their blocks were known, seeds their
    /// emitters, and refills from the lit face-neighbour chunks. Returns every
    /// chunk whose light changed.
    pub(crate) fn light_arrived(
        &mut self,
        grid: &mut VoxelGrid,
        arrived: &[IVec3],
    ) -> HashSet<IVec3> {
        let mut affected = HashSet::new();
        if !grid.supports_lighting() {
            return affected;
        }
        let arrived_set: HashSet<IVec3> = arrived.iter().copied().collect();
        let arrived: Vec<IVec3> = arrived_set.iter().copied().collect();
        let arrived = arrived.as_slice();

        for ch in CHANNELS {
            let stale = stale_light(grid, arrived, ch);
            self.remove_nodes(grid, stale, ch, &mut affected);
        }

        for &chunk in arrived {
            let positions: Vec<IVec3> = grid
                .chunk_block_data(chunk)
                .iter()
                .map(|block| block.position)
                .collect();
            self.seed_emitters(grid, &positions, &mut affected);
        }

        for ch in CHANNELS {
            let seeds = neighbour_face_light(grid, arrived, &arrived_set, ch);
            if !seeds.is_empty() {
                self.bfs_add_nodes(grid, &[], &seeds, ch, &mut affected);
            }
        }
        affected
    }

    /// Flood-fill the light of every emissive block among `positions`.
    fn seed_emitters(
        &mut self,
        grid: &mut VoxelGrid,
        positions: &[IVec3],
        affected: &mut HashSet<IVec3>,
    ) {
        let seeds: Vec<(IVec3, [u8; 3])> = positions
            .iter()
            .filter_map(|&pos| {
                let emission = grid.material_registry.emission(grid.material_at(pos)?);
                (emission != [0, 0, 0]).then_some((pos, emission))
            })
            .collect();

        // Order does not matter: each voxel ends at the brightest level any seed reaches it with.
        for (pos, rgb) in seeds {
            self.add_rgb(grid, pos, rgb, affected);
        }
    }

    /// One BFS pass per lit channel of `rgb`, recording touched chunks in `affected`.
    fn add_rgb(
        &mut self,
        grid: &mut VoxelGrid,
        pos: IVec3,
        rgb: [u8; 3],
        affected: &mut HashSet<IVec3>,
    ) {
        for (ch, level) in [LightChannel::R, LightChannel::G, LightChannel::B]
            .into_iter()
            .zip(rgb)
        {
            if level > 0 {
                self.bfs_add(grid, pos, level, ch, affected);
            }
        }
    }

    // --- Internal BFS helpers ---

    #[inline]
    fn read_ch(grid: &VoxelGrid, chunk_pos: IVec3, idx: usize, ch: LightChannel) -> u8 {
        match grid.chunk_light(chunk_pos) {
            Some(cl) => match ch {
                LightChannel::R => cl.block_light_r[idx],
                LightChannel::G => cl.block_light_g[idx],
                LightChannel::B => cl.block_light_b[idx],
            },
            None => 0,
        }
    }

    #[inline]
    fn write_ch(grid: &mut VoxelGrid, chunk_pos: IVec3, idx: usize, ch: LightChannel, v: u8) {
        let cl = grid.chunk_light_mut(chunk_pos);
        match ch {
            LightChannel::R => cl.block_light_r[idx] = v,
            LightChannel::G => cl.block_light_g[idx] = v,
            LightChannel::B => cl.block_light_b[idx] = v,
        }
    }

    /// Standard "only-if-brighter" BFS addition for a single channel.
    fn bfs_add(
        &mut self,
        grid: &mut VoxelGrid,
        pos: IVec3,
        level: u8,
        ch: LightChannel,
        affected: &mut HashSet<IVec3>,
    ) {
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        self.queue.clear();
        self.queue.push_back(PropNode {
            chunk_pos,
            idx,
            level,
        });
        affected.insert(chunk_pos);
        self.drain(grid, ch, affected);
    }

    /// Re-seed BFS after a dark wave. `emitters` are written at their level;
    /// `relight` voxels keep their stored value, and each spreads at the level it
    /// reads now (a later dark node may have cleared it, in which case it spreads
    /// nothing).
    fn bfs_add_nodes(
        &mut self,
        grid: &mut VoxelGrid,
        emitters: &[PropNode],
        relight: &[PropNode],
        ch: LightChannel,
        affected: &mut HashSet<IVec3>,
    ) {
        self.queue.clear();
        self.queue.extend(emitters.iter().copied());
        for &seed in relight {
            let level = Self::read_ch(grid, seed.chunk_pos, seed.idx, ch);
            self.enqueue_neighbours(grid, PropNode { level, ..seed }, ch);
        }
        self.drain(grid, ch, affected);
    }

    /// Enqueue each neighbour of `node` that `node.level - 1` would brighten.
    fn enqueue_neighbours(&mut self, grid: &VoxelGrid, node: PropNode, ch: LightChannel) {
        let next = node.level.saturating_sub(1);
        if next == 0 {
            return;
        }
        let world = node.chunk_pos * CHUNK_SIZE + idx_to_local(node.idx);
        for nb_world in neighbours(world) {
            // Fully opaque blocks absorb — no propagation through them.
            if grid
                .material_at(nb_world)
                .is_some_and(|mat_id| grid.material_registry.opacity_cost(mat_id) >= 15)
            {
                continue;
            }
            let (nb_chunk, nb_idx, _) = light_storage::world_to_chunk_local(nb_world);
            if next > Self::read_ch(grid, nb_chunk, nb_idx, ch) {
                self.queue.push_back(PropNode {
                    chunk_pos: nb_chunk,
                    idx: nb_idx,
                    level: next,
                });
            }
        }
    }

    /// BFS over the queued nodes that only overwrites dimmer voxels.
    fn drain(&mut self, grid: &mut VoxelGrid, ch: LightChannel, affected: &mut HashSet<IVec3>) {
        while let Some(node) = self.queue.pop_front() {
            let stored = Self::read_ch(grid, node.chunk_pos, node.idx, ch);
            if node.level <= stored {
                continue;
            }
            Self::write_ch(grid, node.chunk_pos, node.idx, ch, node.level);
            mark_light_write(grid, node.chunk_pos, node.idx, affected);
            self.enqueue_neighbours(grid, node, ch);
        }
    }

    /// Two-queue removal for a single channel.
    fn two_queue_remove(
        &mut self,
        grid: &mut VoxelGrid,
        pos: IVec3,
        ch: LightChannel,
        affected: &mut HashSet<IVec3>,
    ) {
        let (start_chunk, start_idx, _) = light_storage::world_to_chunk_local(pos);
        let start_level = Self::read_ch(grid, start_chunk, start_idx, ch);
        if start_level == 0 {
            return;
        }
        let start = PropNode {
            chunk_pos: start_chunk,
            idx: start_idx,
            level: start_level,
        };
        self.remove_nodes(grid, vec![start], ch, affected);
    }

    /// Two-queue removal of the light at every node of `starts`, each carrying
    /// the level currently stored there.
    fn remove_nodes(
        &mut self,
        grid: &mut VoxelGrid,
        starts: Vec<PropNode>,
        ch: LightChannel,
        affected: &mut HashSet<IVec3>,
    ) {
        let mut dark: VecDeque<PropNode> = VecDeque::with_capacity(512);
        let mut relight: Vec<PropNode> = Vec::new();
        let mut emitters: Vec<PropNode> = Vec::new();
        let mut relight_seen: HashSet<(IVec3, usize)> = HashSet::new();

        dark.extend(starts);

        while let Some(node) = dark.pop_front() {
            let stored = Self::read_ch(grid, node.chunk_pos, node.idx, ch);
            if stored == 0 {
                continue;
            }
            // A dark node carries the value stored when it was pushed, and the only
            // write in this loop zeroes a voxel, which the check above skips.
            debug_assert_eq!(stored, node.level);
            // Clear this voxel.
            Self::write_ch(grid, node.chunk_pos, node.idx, ch, 0);
            mark_light_write(grid, node.chunk_pos, node.idx, affected);

            let world = node.chunk_pos * CHUNK_SIZE + idx_to_local(node.idx);
            // A surviving emitter's own voxel must read its emission again; a
            // block that emits nothing on `ch` seeds 0, which `drain` skips.
            if let Some(mat_id) = grid.material_at(world) {
                let level = grid.material_registry.emission(mat_id)[ch as usize];
                emitters.push(PropNode { level, ..node });
            }
            for nb_world in neighbours(world) {
                let (nb_chunk, nb_idx, _) = light_storage::world_to_chunk_local(nb_world);
                let nb_stored = Self::read_ch(grid, nb_chunk, nb_idx, ch);
                if nb_stored == 0 {
                    continue;
                }
                if nb_stored < node.level {
                    // Was lit by the chain we're removing.
                    dark.push_back(PropNode {
                        chunk_pos: nb_chunk,
                        idx: nb_idx,
                        level: nb_stored,
                    });
                } else {
                    // Lit by another surviving source — re-seed.
                    if relight_seen.insert((nb_chunk, nb_idx)) {
                        relight.push(PropNode {
                            chunk_pos: nb_chunk,
                            idx: nb_idx,
                            level: nb_stored,
                        });
                    }
                }
            }
        }

        // Light held inside an opaque block that does not emit is a leftover,
        // not a source.
        relight.retain(|n| is_light_source(grid, *n));
        self.bfs_add_nodes(grid, &emitters, &relight, ch, affected);
    }
}

/// Records a light write at `idx` of `chunk_pos`: the chunk itself, and the loaded chunks
/// across the face layers the voxel lies on, whose surfaces read it.
fn mark_light_write(grid: &VoxelGrid, chunk_pos: IVec3, idx: usize, affected: &mut HashSet<IVec3>) {
    affected.insert(chunk_pos);
    affected.extend(
        border_offsets(idx_to_local(idx))
            .map(|offset| chunk_pos + offset)
            .filter(|&across| grid.has_chunk(across)),
    );
}

/// Whether the light stored at `node` can spread: it sits in a transparent
/// voxel or in an emitting block.
fn is_light_source(grid: &VoxelGrid, node: PropNode) -> bool {
    let world = node.chunk_pos * CHUNK_SIZE + idx_to_local(node.idx);
    grid.material_at(world).is_none_or(|mat_id| {
        grid.material_registry.opacity_cost(mat_id) < 15
            || grid.material_registry.emission(mat_id) != [0, 0, 0]
    })
}

/// The node for `idx` of `chunk_pos` if it holds light in `levels`.
fn lit_node(chunk_pos: IVec3, levels: &[u8; CHUNK_VOLUME], idx: usize) -> Option<PropNode> {
    let level = levels[idx];
    (level != 0).then_some(PropNode {
        chunk_pos,
        idx,
        level,
    })
}

/// The channel's level array of a chunk's light state.
fn channel_levels(light: &ChunkLight, ch: LightChannel) -> &[u8; CHUNK_VOLUME] {
    match ch {
        LightChannel::R => &light.block_light_r,
        LightChannel::G => &light.block_light_g,
        LightChannel::B => &light.block_light_b,
    }
}

/// Every voxel of the `arrived` chunks that holds light in `ch`.
fn stale_light(grid: &VoxelGrid, arrived: &[IVec3], ch: LightChannel) -> Vec<PropNode> {
    let mut stale = Vec::new();
    for &chunk_pos in arrived {
        let Some(light) = grid.chunk_light(chunk_pos) else {
            continue;
        };
        let levels = channel_levels(light, ch);
        stale.extend((0..CHUNK_VOLUME).filter_map(|idx| lit_node(chunk_pos, levels, idx)));
    }
    stale
}

/// Chunk-local indices of the 16x16 layer of a chunk that touches the chunk
/// lying in direction `-dir` from it.
fn face_layer(dir: IVec3) -> impl Iterator<Item = usize> {
    let axis = dir.abs().max_position();
    let face = if dir[axis] == 1 { 0 } else { CHUNK_SIZE - 1 };
    (0..CHUNK_SIZE).flat_map(move |a| {
        (0..CHUNK_SIZE).map(move |b| {
            let mut cell = IVec3::ZERO;
            cell[axis] = face;
            cell[(axis + 1) % 3] = a;
            cell[(axis + 2) % 3] = b;
            local_index(cell.x, cell.y, cell.z)
        })
    })
}

/// Lit voxels on the face layers of loaded chunks (outside `arrived_set`) that
/// touch an arrived chunk.
fn neighbour_face_light(
    grid: &VoxelGrid,
    arrived: &[IVec3],
    arrived_set: &HashSet<IVec3>,
    ch: LightChannel,
) -> Vec<PropNode> {
    let mut seeds = Vec::new();
    for &chunk in arrived {
        for dir in OFFSETS {
            let chunk_pos = chunk + dir;
            if !arrived_set.contains(&chunk_pos) {
                scan_face(grid, chunk_pos, dir, ch, &mut seeds);
            }
        }
    }
    seeds
}

/// Push the lit voxels of `chunk_pos`'s layer facing its neighbour in `-dir`.
fn scan_face(
    grid: &VoxelGrid,
    chunk_pos: IVec3,
    dir: IVec3,
    ch: LightChannel,
    seeds: &mut Vec<PropNode>,
) {
    let Some(light) = grid.chunk_light(chunk_pos) else {
        return;
    };
    let levels = channel_levels(light, ch);
    seeds.extend(face_layer(dir).filter_map(|idx| lit_node(chunk_pos, levels, idx)));
}

/// The six face neighbours of `world`.
fn neighbours(world: IVec3) -> impl Iterator<Item = IVec3> {
    OFFSETS.iter().map(move |&off| world + off)
}
