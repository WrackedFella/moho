//! Colony state: the orders the player has given the world, independent of any pawn.

use std::collections::BTreeSet;

use crate::inventory::Inventory;
use crate::pawn::PawnId;

/// The orders a colony holds and what its pawns have brought in.
#[derive(Debug, Default)]
pub struct Colony {
    pub marks: MineMarks,
    /// The colony total per resource.
    pub stock: Inventory,
}

/// What a mark order does to a block that is, or is not, marked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkChange {
    Marked,
    Cancelled,
}

/// An order to dig one block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MineMark {
    /// The pawn walking to or digging this block, if any.
    pub claimed_by: Option<PawnId>,
}

/// The blocks marked for mining, keyed by block position.
///
/// A set while a mark carries no data (`clippy::zero_sized_map_values`); it becomes a map
/// from position to [`MineMark`] when a mark gains a claim.
#[derive(Debug, Default)]
pub struct MineMarks {
    marks: BTreeSet<[i32; 3]>,
}

impl MineMarks {
    /// Marks an unmarked block and cancels a marked one.
    pub fn toggle(&mut self, pos: [i32; 3]) -> MarkChange {
        if self.marks.remove(&pos) {
            MarkChange::Cancelled
        } else {
            self.marks.insert(pos);
            MarkChange::Marked
        }
    }

    pub fn contains(&self, pos: [i32; 3]) -> bool {
        self.marks.contains(&pos)
    }

    /// The marked blocks in key order.
    pub fn iter(&self) -> impl Iterator<Item = ([i32; 3], &MineMark)> {
        self.marks
            .iter()
            .map(|pos| (*pos, &MineMark { claimed_by: None }))
    }

    /// Records `pawn` as the claimant of the marked block at `pos`; does nothing for an
    /// unmarked block.
    pub fn claim(&mut self, pos: [i32; 3], pawn: PawnId) {
        let _ = (pos, pawn);
        todo!()
    }

    /// Clears the claim on the block at `pos`; the mark stays.
    pub fn release(&mut self, pos: [i32; 3]) {
        let _ = pos;
        todo!()
    }

    /// The marked blocks nobody has claimed, in key order.
    pub fn unclaimed(&self) -> impl Iterator<Item = [i32; 3]> {
        let none: Option<[i32; 3]> = todo!();
        none.into_iter()
    }

    /// Drops the mark on a block that no longer exists; does nothing for an unmarked one.
    pub fn block_removed(&mut self, pos: [i32; 3]) {
        self.marks.remove(&pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawn::Pawn;
    use flagstone_voxel::{BlockPos, VoxelGrid};
    use glam::Vec3;
    use proptest::prelude::*;

    fn marked(marks: &MineMarks) -> Vec<[i32; 3]> {
        marks.iter().map(|(pos, _)| pos).collect()
    }

    #[test]
    fn toggling_an_unmarked_block_marks_it() {
        let mut marks = MineMarks::default();

        let change = marks.toggle([3, 4, 5]);

        assert_eq!(change, MarkChange::Marked);
        assert!(marks.contains([3, 4, 5]));
        assert_eq!(marked(&marks), [[3, 4, 5]]);
        assert!(!marks.contains([3, 4, 6]), "only the toggled block");
    }

    #[test]
    fn toggling_a_marked_block_cancels_it() {
        let mut marks = MineMarks::default();
        marks.toggle([3, 4, 5]);
        marks.toggle([9, 9, 9]);

        let change = marks.toggle([3, 4, 5]);

        assert_eq!(change, MarkChange::Cancelled);
        assert!(!marks.contains([3, 4, 5]));
        assert_eq!(marked(&marks), [[9, 9, 9]], "other marks stay");
    }

    #[test]
    fn block_removed_drops_its_mark_and_leaves_others() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);
        marks.toggle([1, 2, 4]);
        marks.toggle([-7, 0, 3]);

        marks.block_removed([1, 2, 3]);

        assert!(!marks.contains([1, 2, 3]));
        assert_eq!(marked(&marks), [[-7, 0, 3], [1, 2, 4]]);
    }

    #[test]
    fn block_removed_on_an_unmarked_block_changes_nothing() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);

        marks.block_removed([8, 8, 8]);

        assert_eq!(marked(&marks), [[1, 2, 3]]);
    }

    #[test]
    fn marks_iterate_in_key_order() {
        let mut marks = MineMarks::default();
        for pos in [[5, 0, 0], [-1, 9, 9], [5, -3, 2], [0, 0, 0], [5, 0, -1]] {
            marks.toggle(pos);
        }

        let order = marked(&marks);

        assert_eq!(
            order,
            [[-1, 9, 9], [0, 0, 0], [5, -3, 2], [5, 0, -1], [5, 0, 0]]
        );
    }

    fn claimant(marks: &MineMarks, pos: [i32; 3]) -> Option<Option<PawnId>> {
        marks
            .iter()
            .find(|(p, _)| *p == pos)
            .map(|(_, mark)| mark.claimed_by)
    }

    #[test]
    fn claim_and_release_round_trip() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 0, 0]);
        marks.toggle([2, 0, 0]);

        marks.claim([1, 0, 0], PawnId(4));

        assert_eq!(claimant(&marks, [1, 0, 0]), Some(Some(PawnId(4))));
        assert_eq!(
            claimant(&marks, [2, 0, 0]),
            Some(None),
            "other marks stay free"
        );
        assert_eq!(marks.unclaimed().collect::<Vec<_>>(), [[2, 0, 0]]);

        marks.release([1, 0, 0]);

        assert_eq!(claimant(&marks, [1, 0, 0]), Some(None));
        assert!(marks.contains([1, 0, 0]), "release keeps the mark");
        assert_eq!(
            marks.unclaimed().collect::<Vec<_>>(),
            [[1, 0, 0], [2, 0, 0]]
        );
    }

    #[test]
    fn claiming_an_unmarked_block_marks_nothing() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 0, 0]);

        marks.claim([9, 9, 9], PawnId(0));
        marks.release([8, 8, 8]);

        assert!(!marks.contains([9, 9, 9]));
        assert_eq!(marked(&marks), [[1, 0, 0]]);
        assert_eq!(marks.unclaimed().collect::<Vec<_>>(), [[1, 0, 0]]);
    }

    #[test]
    fn cancelling_a_claimed_mark_removes_it_from_the_unclaimed_list() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 0, 0]);
        marks.claim([1, 0, 0], PawnId(0));

        let change = marks.toggle([1, 0, 0]);

        assert_eq!(change, MarkChange::Cancelled);
        assert_eq!(marks.unclaimed().count(), 0);
        assert_eq!(claimant(&marks, [1, 0, 0]), None);
    }

    fn dug_into_stock(resource: Option<u32>) -> Colony {
        let mut grid = VoxelGrid::new(16);
        grid.mutator().place(BlockPos::new(0, 0, 0), 1, resource);
        let pawn = Pawn::default();
        let mut colony = Colony::default();

        let outcome = pawn
            .dig(&mut grid, Vec3::new(0.5, 0.5, -5.0), Vec3::Z, 10.0)
            .expect("ray should hit the placed block");
        if let Some(yield_) = outcome.yield_ {
            colony.stock.add(yield_.resource_id, yield_.amount);
        }

        assert!(
            pawn.inventory.is_empty(),
            "a pawn job does not fill the pawn's own inventory"
        );
        colony
    }

    #[test]
    fn stock_grows_by_the_yield() {
        let colony = dug_into_stock(Some(7));

        assert_eq!(colony.stock.count(7), 1);
        assert_eq!(colony.stock.iter().count(), 1);
    }

    #[test]
    fn stock_is_unchanged_by_a_plain_block() {
        let colony = dug_into_stock(None);

        assert!(colony.stock.is_empty());
    }

    proptest! {
        #[test]
        fn toggling_twice_leaves_the_marks_unchanged(
            existing in proptest::collection::btree_set(any::<[i32; 3]>(), 0..8),
            fresh in any::<[i32; 3]>(),
            pick in any::<proptest::sample::Index>(),
            from_existing in any::<bool>(),
        ) {
            let pos = if from_existing && !existing.is_empty() {
                *existing.iter().nth(pick.index(existing.len())).expect("index is in range")
            } else {
                fresh
            };
            let mut marks = MineMarks::default();
            for &p in &existing {
                marks.toggle(p);
            }
            let before = marked(&marks);

            marks.toggle(pos);
            marks.toggle(pos);

            prop_assert_eq!(marked(&marks), before);
        }
    }
}
