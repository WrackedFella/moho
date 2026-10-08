//! Named block mutations on a [`VoxelGrid`].

use super::grid::{BlockPos, VoxelGrid};

/// A short-lived guard for semantically meaningful block mutations on a [`VoxelGrid`].
///
/// Wraps `&mut VoxelGrid` and exposes named operations so call sites read as intent
/// rather than raw grid plumbing. Obtain one via [`VoxelGrid::mutator`].
///
/// `VoxelMutator` does **not** publish events: it is intended for terrain generation
/// and other bulk writes where the caller controls the surrounding lifecycle.
#[derive(Debug)]
pub struct VoxelMutator<'a> {
    grid: &'a mut VoxelGrid,
}

impl<'a> VoxelMutator<'a> {
    /// Construct from a mutable grid reference. Prefer [`VoxelGrid::mutator`].
    pub(crate) fn new(grid: &'a mut VoxelGrid) -> Self {
        Self { grid }
    }

    /// Place a block, replacing any existing block at `pos`.
    ///
    /// Delegates to [`VoxelGrid::place_block`]; see that method's documentation
    /// for the full list of side effects (dirty flags, sky-column updates, etc.).
    pub fn place(&mut self, pos: BlockPos, material_id: u32, resource_id: Option<u32>) {
        self.grid.place_block(pos, material_id, resource_id);
    }

    /// Remove the block at `pos`. Returns `true` if a block was removed.
    pub fn remove(&mut self, pos: BlockPos) -> bool {
        self.grid.clear_block(pos)
    }

    /// Fill an axis-aligned rectangular region `[min, max]` (inclusive) with
    /// `material_id` and no resource attachment.
    ///
    /// Calls `place_block` for every position in the region, so each block sets
    /// the usual dirty flags. For large fills this is intentionally simple —
    /// callers that need optimized bulk writes can work with the grid directly.
    pub fn fill_region(&mut self, min: BlockPos, max: BlockPos, material_id: u32) {
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    self.grid
                        .place_block(BlockPos::new(x, y, z), material_id, None);
                }
            }
        }
    }
}
