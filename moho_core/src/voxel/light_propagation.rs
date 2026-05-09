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
    chunk_size: i32,
}

impl LightPropagator {
    pub fn new(chunk_size: i32) -> Self {
        Self {
            queue: VecDeque::with_capacity(1024),
            chunk_size,
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
                if emission != [0, 0, 0] {
                    Some((pos, emission))
                } else {
                    None
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

    /// Returns the chunk coordinate for a world position.
    #[inline]
    pub fn get_chunk_coord(&self, pos: &IVec3) -> IVec3 {
        IVec3::new(
            pos.x.div_euclid(self.chunk_size),
            pos.y.div_euclid(self.chunk_size),
            pos.z.div_euclid(self.chunk_size),
        )
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
                    .map(|mat_id| grid.material_registry.opacity_cost(mat_id) >= 15)
                    .unwrap_or(false)
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
                    .map(|mat_id| grid.material_registry.opacity_cost(mat_id) >= 15)
                    .unwrap_or(false)
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
                    .map(|mat_id| grid.material_registry.opacity_cost(mat_id) >= 15)
                    .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::grid::{MaterialLighting, VoxelGrid};

    fn opaque_grid() -> VoxelGrid {
        VoxelGrid::new(16)
    }

    #[test]
    fn test_light_propagation_basic() {
        let mut grid = opaque_grid();
        let mut propagator = LightPropagator::new(16);

        // 3×3×3 solid cube of transparent material (material 0, opacity=0 default?).
        // Material 0 defaults to NON_EMISSIVE_OPAQUE (opacity_cost=15), so we need
        // a transparent material. Use material 1 registered above.
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        for x in 0..3i32 {
            for y in 0..3i32 {
                for z in 0..3i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        // Light source at (1, 2, 1) — top-center.
        let source = IVec3::new(1, 2, 1);
        propagator.add_light_rgb(&mut grid, source, [15, 0, 0]);

        let (sc, si, _) = light_storage::world_to_chunk_local(source);
        let cl = grid.chunk_light(sc).unwrap();
        assert_eq!(cl.block_light_r[si], 15, "source should have level 15");

        // Neighbor one step away should have 14.
        let nb = IVec3::new(1, 1, 1);
        let (nc, ni, _) = light_storage::world_to_chunk_local(nb);
        let cl = grid.chunk_light(nc).unwrap();
        assert_eq!(cl.block_light_r[ni], 14, "one step from source → level 14");
    }

    #[test]
    fn test_chunk_boundary_detection() {
        let p = LightPropagator::new(16);
        assert_eq!(p.get_chunk_coord(&IVec3::new(0, 0, 0)), IVec3::ZERO);
        assert_eq!(p.get_chunk_coord(&IVec3::new(15, 15, 15)), IVec3::ZERO);
        assert_eq!(
            p.get_chunk_coord(&IVec3::new(16, 0, 0)),
            IVec3::new(1, 0, 0)
        );
        assert_eq!(
            p.get_chunk_coord(&IVec3::new(-1, 0, 0)),
            IVec3::new(-1, 0, 0)
        );
    }

    #[test]
    fn test_incremental_light_addition() {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        let mut propagator = LightPropagator::new(16);

        for x in 0..5i32 {
            for y in 0..5i32 {
                for z in 0..5i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        let light_pos = IVec3::new(2, 2, 2);
        let affected = propagator.add_light_rgb(&mut grid, light_pos, [15, 8, 2]);
        assert!(!affected.is_empty(), "should track affected chunks");

        let (sc, si, _) = light_storage::world_to_chunk_local(light_pos);
        let cl = grid.chunk_light(sc).unwrap();
        assert_eq!(cl.block_light_r[si], 15);
        assert_eq!(cl.block_light_g[si], 8);
        assert_eq!(cl.block_light_b[si], 2);

        let nb = IVec3::new(3, 2, 2);
        let (nc, ni, _) = light_storage::world_to_chunk_local(nb);
        let cl = grid.chunk_light(nc).unwrap();
        assert_eq!(cl.block_light_r[ni], 14, "neighbor should have R level 14");
    }

    #[test]
    fn test_incremental_light_stops_at_brighter() {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        let mut propagator = LightPropagator::new(16);

        // Pre-fill a line with R=10.
        for x in 0..5i32 {
            grid.place_block(IVec3::new(x, 0, 0), 1, None);
        }
        // Manually set R=10 in chunk light.
        for x in 0..5i32 {
            let pos = IVec3::new(x, 0, 0);
            let (cp, idx, _) = light_storage::world_to_chunk_local(pos);
            grid.chunk_light_mut(cp).block_light_r[idx] = 10;
        }

        // Adding a weaker source (R=8) should not overwrite R=10.
        propagator.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [8, 0, 0]);

        let far = IVec3::new(4, 0, 0);
        let (cp, idx, _) = light_storage::world_to_chunk_local(far);
        assert_eq!(
            grid.chunk_light(cp).unwrap().block_light_r[idx],
            10,
            "existing brighter light must not be reduced"
        );
    }

    #[test]
    fn test_light_removal_basic() {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        let mut propagator = LightPropagator::new(16);

        for x in 0..5i32 {
            for y in 0..5i32 {
                for z in 0..5i32 {
                    grid.place_block(IVec3::new(x, y, z), 1, None);
                }
            }
        }

        let light_pos = IVec3::new(2, 2, 2);
        propagator.add_light_rgb(&mut grid, light_pos, [15, 0, 0]);

        // Source should be lit.
        let (sc, si, _) = light_storage::world_to_chunk_local(light_pos);
        assert_eq!(grid.chunk_light(sc).unwrap().block_light_r[si], 15);

        // Remove it.
        let affected = propagator.remove_light(&mut grid, light_pos);
        assert!(!affected.is_empty());

        // Source and neighbor should now be dark.
        assert_eq!(grid.chunk_light(sc).unwrap().block_light_r[si], 0);
        let nb = IVec3::new(3, 2, 2);
        let (nc, ni, _) = light_storage::world_to_chunk_local(nb);
        assert_eq!(grid.chunk_light(nc).unwrap().block_light_r[ni], 0);
    }

    #[test]
    fn test_light_removal_with_multiple_sources() {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        let mut propagator = LightPropagator::new(16);

        for x in 0..11i32 {
            grid.place_block(IVec3::new(x, 0, 0), 1, None);
        }

        let light1 = IVec3::new(0, 0, 0);
        let light2 = IVec3::new(10, 0, 0);
        propagator.add_light_rgb(&mut grid, light1, [15, 0, 0]);
        propagator.add_light_rgb(&mut grid, light2, [15, 0, 0]);

        // Remove first source.
        propagator.remove_light(&mut grid, light1);

        // Light2 still illuminates light1's old position (distance 10 → level 5).
        let (c, i, _) = light_storage::world_to_chunk_local(light1);
        assert_eq!(
            grid.chunk_light(c).unwrap().block_light_r[i],
            5,
            "light1 position should now have level 5 from light2"
        );

        // Light2 unchanged.
        let (c2, i2, _) = light_storage::world_to_chunk_local(light2);
        assert_eq!(grid.chunk_light(c2).unwrap().block_light_r[i2], 15);
    }

    #[test]
    fn test_light_removal_corner_case() {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        let mut propagator = LightPropagator::new(16);

        let center = IVec3::new(5, 5, 5);
        grid.place_block(center, 1, None);
        for i in 1..5i32 {
            for &dir in &[
                IVec3::new(i, 0, 0),
                IVec3::new(0, i, 0),
                IVec3::new(0, 0, i),
            ] {
                grid.place_block(center + dir, 1, None);
            }
        }

        propagator.add_light_rgb(&mut grid, center, [15, 0, 0]);
        propagator.remove_light(&mut grid, center);

        let (cc, ci, _) = light_storage::world_to_chunk_local(center);
        assert_eq!(grid.chunk_light(cc).unwrap().block_light_r[ci], 0);

        for i in 1..5i32 {
            for &dir in &[
                IVec3::new(i, 0, 0),
                IVec3::new(0, i, 0),
                IVec3::new(0, 0, i),
            ] {
                let p = center + dir;
                let (pc, pi, _) = light_storage::world_to_chunk_local(p);
                assert_eq!(
                    grid.chunk_light(pc).unwrap().block_light_r[pi],
                    0,
                    "arm block at {:?} should be dark",
                    p
                );
            }
        }
    }

    #[test]
    fn test_cross_chunk_propagation() {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        let mut propagator = LightPropagator::new(16);

        for x in 14..=18i32 {
            grid.place_block(IVec3::new(x, 8, 8), 1, None);
        }

        let light_pos = IVec3::new(15, 8, 8);
        let affected = propagator.add_light_rgb(&mut grid, light_pos, [15, 0, 0]);

        let chunk_0 = IVec3::new(0, 0, 0);
        let chunk_1 = IVec3::new(1, 0, 0);
        assert!(affected.contains(&chunk_0), "chunk 0 should be affected");
        assert!(affected.contains(&chunk_1), "chunk 1 should be affected");

        for (x, expected) in [(15, 15u8), (16, 14), (17, 13), (18, 12)] {
            let (cp, idx, _) = light_storage::world_to_chunk_local(IVec3::new(x, 8, 8));
            assert_eq!(
                grid.chunk_light(cp).unwrap().block_light_r[idx],
                expected,
                "x={x} expected R={expected}"
            );
        }
    }

    #[test]
    fn test_cross_chunk_removal() {
        let mut grid = VoxelGrid::new(16);
        grid.material_registry.set_lighting(
            1,
            MaterialLighting {
                emission: [0, 0, 0],
                opacity_cost: 0,
            },
        );
        let mut propagator = LightPropagator::new(16);

        for x in 12..=20i32 {
            grid.place_block(IVec3::new(x, 8, 8), 1, None);
        }

        let light_pos = IVec3::new(15, 8, 8);
        propagator.add_light_rgb(&mut grid, light_pos, [15, 0, 0]);
        let affected = propagator.remove_light(&mut grid, light_pos);

        assert!(affected.contains(&IVec3::new(0, 0, 0)));
        assert!(affected.contains(&IVec3::new(1, 0, 0)));

        for x in 12..=20i32 {
            let (cp, idx, _) = light_storage::world_to_chunk_local(IVec3::new(x, 8, 8));
            assert_eq!(
                grid.chunk_light(cp).unwrap().block_light_r[idx],
                0,
                "x={x} should be dark after removal"
            );
        }
    }
}
