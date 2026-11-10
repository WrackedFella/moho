use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::inventory::{Inventory, ResourceYield};
use crate::jobs::Job;
use crate::pickups::PickupFeed;
use crate::raycast;
use crate::tools::{STARTING_TOOL, ToolId};

/// Number of hotbar slots. Shared by `select_slot`'s valid range and, today
/// coincidentally, `flagstone_ui`'s own `HOTBAR_SLOTS` — the two aren't coupled
/// (`flagstone_ui` stays decoupled from `flagstone_game`), they just agree in value.
pub const HOTBAR_SLOT_COUNT: usize = 8;

/// The hotbar slot reserved for the equipped tool. Selecting this slot
/// equips `STARTING_TOOL`; selecting any other slot un-equips it (the pawn
/// is "holding" a resource, not a tool) — see `Pawn::select_slot`.
pub const TOOL_SLOT: usize = 0;

/// Reach of the mining action, in metres, for a possessed pawn and a working one alike.
pub const MINE_REACH: f32 = 8.0;

/// Identifies one pawn; issued by the simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PawnId(pub u32);

/// A gameplay component owning an inventory and (later) the capability to
/// act on the world — mining, movement intent, etc. A player OR an
/// AI-controlled entity owns a `Pawn`; this is intentionally distinct from
/// `PlayerController`, which is camera/movement-only.
#[derive(Debug)]
pub struct Pawn {
    pub inventory: Inventory,
    /// The currently-equipped tool, if any. Mining requires `Some(_)`.
    /// `Option<ToolId>` rather than a bare `ToolId` so "nothing equipped" is
    /// representable — a future equipment system with multiple slot kinds
    /// (see `_todo/work-items/3_1c-tool-selection-gating.md`) can generalize
    /// this field without changing `Pawn::mine`'s gating logic.
    pub equipped_tool: Option<ToolId>,
    /// Which hotbar slot is currently selected (`0..HOTBAR_SLOT_COUNT`).
    pub selected_slot: usize,
    /// Recent pickups, for the HUD's popups.
    pub pickups: PickupFeed,
    /// What the pawn is doing while nobody possesses it.
    pub job: Job,
}

impl Default for Pawn {
    fn default() -> Self {
        Self {
            inventory: Inventory::default(),
            equipped_tool: Some(STARTING_TOOL),
            selected_slot: TOOL_SLOT,
            pickups: PickupFeed::default(),
            job: Job::default(),
        }
    }
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
        self.pickups.push(yield_);
    }

    /// Select a hotbar slot (`0..HOTBAR_SLOT_COUNT`). Selecting `TOOL_SLOT`
    /// equips `STARTING_TOOL`; selecting any other slot un-equips the tool.
    /// Out-of-range slots are ignored.
    pub fn select_slot(&mut self, slot: usize) {
        if slot >= HOTBAR_SLOT_COUNT {
            return;
        }
        self.selected_slot = slot;
        self.equipped_tool = if slot == TOOL_SLOT {
            Some(STARTING_TOOL)
        } else {
            None
        };
    }

    /// Like [`Pawn::mine`] but deposits nothing: the caller decides where the yield goes.
    pub fn dig(
        &mut self,
        grid: &mut VoxelGrid,
        origin: Vec3,
        dir: Vec3,
        reach: f32,
    ) -> Option<MineOutcome> {
        let _ = (grid, origin, dir, reach);
        todo!("Pawn::dig")
    }

    /// Raycast from `origin` along `dir`, break the first solid voxel within
    /// `max_distance`, and deposit its resource (if any) into this pawn's
    /// inventory. Returns `None` if nothing was in range, or if no tool is
    /// currently equipped (a no-op — the world is left unchanged).
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
        self.equipped_tool?;

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
            amount: 5,
        });

        assert_eq!(pawn.inventory.count(3), 5);
    }

    #[test]
    fn pawn_spawns_with_starting_tool_equipped() {
        let pawn = Pawn::default();

        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn selecting_non_tool_slot_unequips_tool() {
        let mut pawn = Pawn::default();

        pawn.select_slot(3);

        assert_eq!(pawn.equipped_tool, None);
        assert_eq!(pawn.selected_slot, 3);
    }

    #[test]
    fn select_slot_out_of_range_is_ignored() {
        let mut pawn = Pawn::default();
        pawn.select_slot(2);

        pawn.select_slot(HOTBAR_SLOT_COUNT);
        pawn.select_slot(usize::MAX);

        assert_eq!(pawn.selected_slot, 2);
        assert_eq!(pawn.equipped_tool, None);
    }

    #[test]
    fn select_slot_accepts_last_hotbar_slot() {
        let mut pawn = Pawn::default();

        pawn.select_slot(HOTBAR_SLOT_COUNT - 1);

        assert_eq!(pawn.selected_slot, HOTBAR_SLOT_COUNT - 1);
        assert_eq!(pawn.equipped_tool, None);
    }

    #[test]
    fn select_slot_out_of_range_keeps_equipped_tool() {
        let mut pawn = Pawn::default();

        pawn.select_slot(8);

        assert_eq!(pawn.selected_slot, TOOL_SLOT);
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
    }

    #[test]
    fn reselecting_tool_slot_reequips_tool() {
        let mut pawn = Pawn::default();
        pawn.select_slot(3);

        pawn.select_slot(TOOL_SLOT);

        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
    }

    #[test]
    fn mine_without_equipped_tool_is_noop() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);
        grid.mutator().place(pos, 1, Some(7));
        let mut pawn = Pawn::default();
        pawn.select_slot(1);

        let outcome = pawn.mine(&mut grid, Vec3::new(0.5, 0.5, -5.0), Vec3::Z, 10.0);

        assert!(outcome.is_none());
        assert!(pawn.inventory.is_empty());
        assert!(grid.material_at(pos).is_some());
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
        let pos = BlockPos::new(0, 0, 12);
        grid.mutator().place(pos, 1, Some(7));
        let mut pawn = Pawn::default();

        let outcome = pawn.mine(&mut grid, Vec3::new(0.5, 0.5, 0.0), Vec3::Z, 10.0);

        assert!(outcome.is_none());
        assert!(grid.material_at(pos).is_some());
        assert!(pawn.inventory.is_empty());
    }

    #[test]
    fn mining_ore_adds_one_pickup() {
        let mut grid = VoxelGrid::new(16);
        grid.mutator().place(BlockPos::new(0, 0, 0), 1, Some(7));
        let mut pawn = Pawn::default();

        pawn.mine(&mut grid, Vec3::new(0.5, 0.5, -5.0), Vec3::Z, 10.0)
            .expect("ray should hit the placed block");

        let listed: Vec<(u32, u32)> = pawn
            .pickups
            .entries()
            .iter()
            .map(|e| (e.resource_id, e.amount))
            .collect();
        assert_eq!(listed, [(7, 1)]);
    }

    #[test]
    fn mining_a_block_with_no_resource_adds_no_pickup() {
        let mut grid = VoxelGrid::new(16);
        grid.mutator().place(BlockPos::new(0, 0, 0), 1, None);
        let mut pawn = Pawn::default();

        pawn.mine(&mut grid, Vec3::new(0.5, 0.5, -5.0), Vec3::Z, 10.0)
            .expect("ray should hit the placed block");

        assert!(pawn.pickups.entries().is_empty());
    }

    fn ore_in_front(resource: Option<u32>) -> (VoxelGrid, BlockPos) {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);
        grid.mutator().place(pos, 1, resource);
        (grid, pos)
    }

    const AIM_FROM: Vec3 = Vec3::new(0.5, 0.5, -5.0);

    #[test]
    fn dig_removes_the_block_without_depositing() {
        let (mut grid, pos) = ore_in_front(Some(7));
        let mut pawn = Pawn::default();

        let outcome = pawn
            .dig(&mut grid, AIM_FROM, Vec3::Z, MINE_REACH)
            .expect("ray should hit the placed block");

        assert_eq!(outcome.block_pos, pos);
        assert_eq!(outcome.old_material_id, 1);
        let yielded = outcome.yield_.expect("an ore block yields");
        assert_eq!((yielded.resource_id, yielded.amount), (7, 1));
        assert!(grid.material_at(pos).is_none(), "the block is gone");
        assert!(
            pawn.inventory.is_empty(),
            "the caller decides where it goes"
        );
        assert!(pawn.pickups.entries().is_empty());
    }

    #[test]
    fn dig_of_a_plain_block_yields_nothing() {
        let (mut grid, pos) = ore_in_front(None);
        let mut pawn = Pawn::default();

        let outcome = pawn
            .dig(&mut grid, AIM_FROM, Vec3::Z, MINE_REACH)
            .expect("ray should hit the placed block");

        assert!(outcome.yield_.is_none());
        assert!(grid.material_at(pos).is_none());
    }

    #[test]
    fn mine_is_dig_plus_deposit() {
        let (mut mined_grid, _) = ore_in_front(Some(7));
        let (mut dug_grid, _) = ore_in_front(Some(7));
        let mut miner = Pawn::default();
        let mut digger = Pawn::default();

        let mined = miner
            .mine(&mut mined_grid, AIM_FROM, Vec3::Z, MINE_REACH)
            .expect("mine hits");
        let dug = digger
            .dig(&mut dug_grid, AIM_FROM, Vec3::Z, MINE_REACH)
            .expect("dig hits");
        digger.deposit(dug.yield_.expect("ore yields"));

        assert_eq!(mined.block_pos, dug.block_pos);
        assert_eq!(mined.old_material_id, dug.old_material_id);
        assert_eq!(mined.distance, dug.distance);
        assert_eq!(
            mined.yield_.map(|y| (y.resource_id, y.amount)),
            dug.yield_.map(|y| (y.resource_id, y.amount))
        );
        assert_eq!(
            miner.inventory.iter().collect::<Vec<_>>(),
            digger.inventory.iter().collect::<Vec<_>>()
        );
        assert_eq!(
            miner.pickups.entries().len(),
            digger.pickups.entries().len()
        );
        assert_eq!(
            mined_grid.material_at(BlockPos::new(0, 0, 0)),
            dug_grid.material_at(BlockPos::new(0, 0, 0))
        );
    }

    #[test]
    fn dig_without_a_tool_is_a_noop() {
        let (mut grid, pos) = ore_in_front(Some(7));
        let mut pawn = Pawn::default();
        pawn.select_slot(1);

        let outcome = pawn.dig(&mut grid, AIM_FROM, Vec3::Z, MINE_REACH);

        assert!(outcome.is_none());
        assert!(grid.material_at(pos).is_some());
        assert!(pawn.inventory.is_empty());
    }

    #[test]
    fn dig_beyond_the_reach_leaves_the_block() {
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 12);
        grid.mutator().place(pos, 1, Some(7));
        let mut pawn = Pawn::default();

        let missed = pawn.dig(&mut grid, Vec3::new(0.5, 0.5, 0.0), Vec3::Z, MINE_REACH);
        let reached = pawn.dig(&mut grid, Vec3::new(0.5, 0.5, 6.0), Vec3::Z, MINE_REACH);

        assert!(missed.is_none(), "12 m is beyond the reach");
        assert_eq!(reached.map(|o| o.block_pos), Some(pos));
    }
}
