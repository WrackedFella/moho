//! Colony state: the orders the player has given the world, independent of any pawn.

use std::collections::BTreeSet;

use crate::inventory::Inventory;
use crate::pawn::PawnId;

/// The orders a colony holds, and what its pawns have brought in.
#[derive(Debug, Default)]
pub struct Colony {
    pub marks: MineMarks,
    /// The colony total: what pawn jobs have mined, per resource.
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
    /// The pawn working the order, if one is.
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

    /// Gives the order on `pos` to `pawn`. Returns whether it did: an unmarked block, or a
    /// mark another pawn holds, is left as it was.
    pub fn claim(&mut self, _pos: [i32; 3], _pawn: PawnId) -> bool {
        todo!("claims are stored with the marks")
    }

    /// Takes the claim off the mark on `pos`; the mark stays. Does nothing for an unmarked
    /// block or an unclaimed mark.
    pub fn release(&mut self, _pos: [i32; 3]) {
        todo!("claims are stored with the marks")
    }

    /// The marks no pawn holds, in key order.
    pub fn unclaimed(&self) -> Vec<[i32; 3]> {
        todo!("claims are stored with the marks")
    }

    /// Drops the mark on a block that no longer exists; does nothing for an unmarked one.
    pub fn block_removed(&mut self, pos: [i32; 3]) {
        self.marks.remove(&pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn holder(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(p, _)| *p == pos)
            .expect("the block is marked")
            .1
            .claimed_by
    }

    #[test]
    fn claim_and_release_round_trip() {
        let mut marks = MineMarks::default();
        marks.toggle([2, 0, 0]);
        marks.toggle([1, 0, 0]);
        let first = PawnId(0);
        let second = PawnId(1);
        assert_eq!(holder(&marks, [1, 0, 0]), None, "a new mark is unclaimed");
        assert_eq!(marks.unclaimed(), [[1, 0, 0], [2, 0, 0]]);

        let claimed = marks.claim([1, 0, 0], first);
        let stolen = marks.claim([1, 0, 0], second);
        let on_air = marks.claim([9, 9, 9], first);

        assert!(claimed, "an unclaimed mark can be claimed");
        assert!(!stolen, "a held mark is not given to a second pawn");
        assert!(!on_air, "an unmarked block cannot be claimed");
        assert_eq!(holder(&marks, [1, 0, 0]), Some(first));
        assert_eq!(holder(&marks, [2, 0, 0]), None, "only the claimed mark");
        assert_eq!(marks.unclaimed(), [[2, 0, 0]]);
        assert!(!marks.contains([9, 9, 9]), "claiming marks nothing");

        marks.release([1, 0, 0]);
        marks.release([9, 9, 9]);

        assert_eq!(holder(&marks, [1, 0, 0]), None);
        assert!(marks.contains([1, 0, 0]), "releasing keeps the mark");
        assert_eq!(marks.unclaimed(), [[1, 0, 0], [2, 0, 0]]);
        assert!(marks.claim([1, 0, 0], second), "a released mark is free");
        assert_eq!(holder(&marks, [1, 0, 0]), Some(second));
    }

    #[test]
    fn cancelling_a_claimed_mark_and_marking_again_gives_an_unclaimed_mark() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 0, 0]);
        let claimed = marks.claim([1, 0, 0], PawnId(0));
        assert!(claimed, "setup: the claim is taken");
        assert_eq!(holder(&marks, [1, 0, 0]), Some(PawnId(0)));

        marks.toggle([1, 0, 0]);
        marks.toggle([1, 0, 0]);

        assert_eq!(holder(&marks, [1, 0, 0]), None);
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
