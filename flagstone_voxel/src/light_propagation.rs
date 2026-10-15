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
//!   then re-seeds from any adjacent voxels still lit by surviving sources.

use super::grid::VoxelGrid;
use super::light_storage::{self, CHUNK_SIZE};
use glam::IVec3;
use std::collections::VecDeque;

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
        let mut affected = std::collections::HashSet::new();
        for (ch, &level) in [LightChannel::R, LightChannel::G, LightChannel::B]
            .iter()
            .zip(rgb.iter())
        {
            if level > 0 {
                self.bfs_add(grid, pos, level, *ch, &mut affected);
            }
        }
        affected.into_iter().collect()
    }

    /// Remove the block-light contributed by the source at `pos` using two-queue
    /// removal. Each channel is processed independently.
    /// Returns the set of chunk positions that were modified.
    pub fn remove_light(&mut self, grid: &mut VoxelGrid, pos: IVec3) -> Vec<IVec3> {
        if !grid.supports_lighting() {
            return vec![];
        }
        let mut affected = std::collections::HashSet::new();
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
        let seeds: Vec<(IVec3, [u8; 3])> = grid
            .block_positions()
            .filter_map(|pos| {
                let mat_id = grid.material_at(pos)?;
                let emission = grid.material_registry.emission(mat_id);
                if emission == [0, 0, 0] {
                    None
                } else {
                    Some((pos, emission))
                }
            })
            .collect();

        let mut affected = std::collections::HashSet::new();
        for ch in [LightChannel::R, LightChannel::G, LightChannel::B] {
            let ci = ch as usize;
            for &(pos, rgb) in &seeds {
                if rgb[ci] > 0 {
                    self.bfs_add(grid, pos, rgb[ci], ch, &mut affected);
                }
            }
        }
        affected.into_iter().collect()
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
        affected: &mut std::collections::HashSet<IVec3>,
    ) {
        let (chunk_pos, idx, _) = light_storage::world_to_chunk_local(pos);
        self.queue.clear();
        self.queue.push_back(PropNode {
            chunk_pos,
            idx,
            level,
        });
        affected.insert(chunk_pos);

        while let Some(node) = self.queue.pop_front() {
            let stored = Self::read_ch(grid, node.chunk_pos, node.idx, ch);
            if node.level <= stored {
                continue;
            }
            Self::write_ch(grid, node.chunk_pos, node.idx, ch, node.level);
            affected.insert(node.chunk_pos);

            let next = node.level.saturating_sub(1);
            if next == 0 {
                continue;
            }
            let world = node.chunk_pos * CHUNK_SIZE + idx_to_local(node.idx);
            for &off in &OFFSETS {
                let nb_world = world + off;
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
    }

    /// Re-seed BFS from a set of nodes whose values are already written in the grid.
    /// Unlike `bfs_add`, we do not try to re-write seed positions — we start by
    /// immediately propagating each seed's stored value into its neighbors.
    fn bfs_add_nodes(
        &mut self,
        grid: &mut VoxelGrid,
        seeds: &[PropNode],
        ch: LightChannel,
        affected: &mut std::collections::HashSet<IVec3>,
    ) {
        self.queue.clear();
        // Emit from each seed directly to neighbors.
        for &seed in seeds {
            let next = seed.level.saturating_sub(1);
            if next == 0 {
                continue;
            }
            let world = seed.chunk_pos * CHUNK_SIZE + idx_to_local(seed.idx);
            for &off in &OFFSETS {
                let nb_world = world + off;
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
        // Standard "only-if-brighter" BFS from the enqueued neighbor entries.
        while let Some(node) = self.queue.pop_front() {
            let stored = Self::read_ch(grid, node.chunk_pos, node.idx, ch);
            if node.level <= stored {
                continue;
            }
            Self::write_ch(grid, node.chunk_pos, node.idx, ch, node.level);
            affected.insert(node.chunk_pos);

            let next = node.level.saturating_sub(1);
            if next == 0 {
                continue;
            }
            let world = node.chunk_pos * CHUNK_SIZE + idx_to_local(node.idx);
            for &off in &OFFSETS {
                let nb_world = world + off;
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
    }

    /// Two-queue removal for a single channel.
    fn two_queue_remove(
        &mut self,
        grid: &mut VoxelGrid,
        pos: IVec3,
        ch: LightChannel,
        affected: &mut std::collections::HashSet<IVec3>,
    ) {
        let (start_chunk, start_idx, _) = light_storage::world_to_chunk_local(pos);
        let start_level = Self::read_ch(grid, start_chunk, start_idx, ch);
        if start_level == 0 {
            return;
        }

        let mut dark: VecDeque<PropNode> = VecDeque::with_capacity(512);
        let mut relight: Vec<PropNode> = Vec::new();

        dark.push_back(PropNode {
            chunk_pos: start_chunk,
            idx: start_idx,
            level: start_level,
        });

        while let Some(node) = dark.pop_front() {
            let stored = Self::read_ch(grid, node.chunk_pos, node.idx, ch);
            if stored == 0 {
                continue;
            }
            if stored != node.level {
                // A brighter surviving source already owns this voxel — re-seed from it.
                if stored > 0
                    && !relight
                        .iter()
                        .any(|r| r.chunk_pos == node.chunk_pos && r.idx == node.idx)
                {
                    relight.push(PropNode {
                        chunk_pos: node.chunk_pos,
                        idx: node.idx,
                        level: stored,
                    });
                }
                continue;
            }
            // Clear this voxel.
            Self::write_ch(grid, node.chunk_pos, node.idx, ch, 0);
            affected.insert(node.chunk_pos);

            let world = node.chunk_pos * CHUNK_SIZE + idx_to_local(node.idx);
            for &off in &OFFSETS {
                let nb_world = world + off;
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
                    if !relight
                        .iter()
                        .any(|r| r.chunk_pos == nb_chunk && r.idx == nb_idx)
                    {
                        relight.push(PropNode {
                            chunk_pos: nb_chunk,
                            idx: nb_idx,
                            level: nb_stored,
                        });
                    }
                }
            }
        }

        if !relight.is_empty() {
            self.bfs_add_nodes(grid, &relight, ch, affected);
        }
    }
}

/// Reconstruct chunk-local (x, y, z) from a dense linear index.
/// Index layout: idx = x + y*16 + z*256.
#[inline]
fn idx_to_local(idx: usize) -> IVec3 {
    let cs = CHUNK_SIZE as usize;
    IVec3::new(
        (idx % cs) as i32,
        ((idx / cs) % cs) as i32,
        (idx / (cs * cs)) as i32,
    )
}
