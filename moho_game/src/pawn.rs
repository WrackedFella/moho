use glam::Vec3;
use moho_core::voxel::{BlockPos, VoxelGrid};

use crate::inventory::{Inventory, ResourceYield};
use crate::raycast;

/// A gameplay component owning an inventory and (later) the capability to
/// act on the world — mining, movement intent, etc. A player OR an
/// AI-controlled entity owns a `Pawn`; this is intentionally distinct from
/// `PlayerController`, which is camera/movement-only.
#[derive(Debug, Default)]
pub struct Pawn {
    pub inventory: Inventory,
}

/// Result of a mine attempt that hit a voxel. `Pawn::mine` only touches the
/// grid and the pawn's own inventory — it deliberately doesn't publish world
/// events, so it stays testable without an `EventBus` and reusable by AI
/// callers. The caller uses `block_pos`/`old_material_id` to publish whatever
/// world-change events its runtime needs (e.g. triggering a chunk remesh).
#[derive(Debug, Clone, Copy)]
pub struct MineOutcome {
    pub block_pos: BlockPos,
    pub old_material_id: u32,
    /// Distance from the ray origin to the hit, in world units. A value at or
    /// near `0.0` means the origin was already inside the mined voxel.
    pub distance: f32,
    pub yield_: Option<ResourceYield>,
}

impl Pawn {
    pub fn deposit(&mut self, yield_: ResourceYield) {
        self.inventory.add(yield_.resource_id, yield_.amount);
    }

    /// Raycast from `origin` along `dir`, break the first solid voxel within
    /// `max_distance`, and deposit its resource (if any) into this pawn's
    /// inventory. Returns `None` if nothing was in range.
    ///
    /// Takes plain `glam` types rather than any player-input or camera type,
    /// so AI-controlled pawns can call this identically with their own aim
    /// (per the cross-cutting Controller/Pawn design goal).
    pub fn mine(
        &mut self,
        grid: &mut VoxelGrid,
        origin: Vec3,
        dir: Vec3,
        max_distance: f32,
    ) -> Option<MineOutcome> {
        // Aims at the rendered isosurface, not the raw voxel cubes — see
        // `raycast_surface`'s docs for why the cube-stepping raycast misses
        // most of what the player can actually see.
        let hit = raycast::raycast_surface(grid, origin, dir, max_distance)?;
        // material_at is guaranteed Some here — raycast only stops on solid voxels.
        let old_material_id = grid.material_at(hit.block_pos)?;
        let resource_id = grid.resource_at(hit.block_pos);
        grid.mutator().remove(hit.block_pos);

        let yield_ = resource_id.map(|resource_id| {
            let yield_ = ResourceYield {
                resource_id,
                amount: 1,
            };
            self.deposit(yield_);
            yield_
        });

        Some(MineOutcome {
            block_pos: hit.block_pos,
            old_material_id,
            distance: hit.distance,
            yield_,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposit_updates_matching_inventory_counter() {
        let mut pawn = Pawn::default();

        pawn.deposit(ResourceYield {
            resource_id: 3,
            amount: 1,
        });

        assert_eq!(pawn.inventory.count(3), 1);
    }

    #[test]
    fn mine_ore_block_breaks_it_and_deposits_resource() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);
        grid.mutator().place(pos, 1, Some(7));
        let mut pawn = Pawn::default();

        let outcome = pawn
            .mine(&mut grid, Vec3::new(0.5, 0.5, -5.0), Vec3::Z, 10.0)
            .expect("ray should hit the placed block");

        assert_eq!(outcome.block_pos, pos);
        assert_eq!(outcome.old_material_id, 1);
        assert_eq!(pawn.inventory.count(7), 1);
        assert!(grid.material_at(pos).is_none());
    }

    #[test]
    fn mine_plain_block_breaks_it_without_yielding_resource() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);
        grid.mutator().place(pos, 1, None);
        let mut pawn = Pawn::default();

        let outcome = pawn
            .mine(&mut grid, Vec3::new(0.5, 0.5, -5.0), Vec3::Z, 10.0)
            .expect("ray should hit the placed block");

        assert!(outcome.yield_.is_none());
        assert!(pawn.inventory.is_empty());
        assert!(grid.material_at(pos).is_none());
    }

    #[test]
    fn mine_with_nothing_in_range_returns_none() {
        let mut grid = VoxelGrid::new(16);
        let mut pawn = Pawn::default();

        let outcome = pawn.mine(&mut grid, Vec3::ZERO, Vec3::Z, 10.0);

        assert!(outcome.is_none());
    }
}
