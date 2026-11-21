//! Colony state: the orders the player has given the world, independent of any pawn.

use std::collections::BTreeSet;

use crate::inventory::Inventory;
use crate::pawn::PawnId;

/// The orders a colony holds, and what its pawns have brought in.
#[derive(Debug, Default)]
pub struct Colony {
    pub marks: MineMarks,
    /// The colony's total of each resource.
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
    /// The pawn that has taken the order, if any.
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
        const UNCLAIMED: MineMark = MineMark { claimed_by: None };
        self.marks.iter().map(|pos| (*pos, &UNCLAIMED))
    }

    /// Gives the mark on `pos` to `pawn`. Returns whether it did: false when `pos` is not
    /// marked or another pawn already holds it.
    pub fn claim(&mut self, pos: [i32; 3], pawn: PawnId) -> bool {
        todo!("claim {pos:?} for {pawn:?}")
    }

    /// Frees the mark on `pos` for any pawn to take; the mark stays. Does nothing for an
    /// unmarked or unclaimed block.
    pub fn release(&mut self, pos: [i32; 3]) {
        todo!("release {pos:?}")
    }

    /// The marked blocks no pawn holds, in key order.
    pub fn unclaimed(&self) -> impl Iterator<Item = [i32; 3]> + '_ {
        std::iter::empty()
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

    fn claimed_by(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(marked, _)| *marked == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn unclaimed(marks: &MineMarks) -> Vec<[i32; 3]> {
        marks.unclaimed().collect()
    }

    #[test]
    fn claim_and_release_round_trip() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);
        marks.toggle([4, 5, 6]);
        let before = unclaimed(&marks);

        let claimed = marks.claim([1, 2, 3], PawnId(0));
        let while_claimed = unclaimed(&marks);
        let holder = claimed_by(&marks, [1, 2, 3]);
        marks.release([1, 2, 3]);

        assert_eq!(before, [[1, 2, 3], [4, 5, 6]]);
        assert!(claimed, "an unclaimed mark can be taken");
        assert_eq!(while_claimed, [[4, 5, 6]], "a claimed mark is not offered");
        assert_eq!(holder, Some(PawnId(0)));
        assert_eq!(unclaimed(&marks), before, "released, it is offered again");
        assert_eq!(claimed_by(&marks, [1, 2, 3]), None);
        assert!(marks.contains([1, 2, 3]), "releasing keeps the mark");
    }

    #[test]
    fn a_claimed_mark_cannot_be_taken_by_another_pawn() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);
        marks.claim([1, 2, 3], PawnId(0));

        let second = marks.claim([1, 2, 3], PawnId(1));

        assert!(!second, "the second pawn is refused");
        assert_eq!(claimed_by(&marks, [1, 2, 3]), Some(PawnId(0)));
    }

    #[test]
    fn claiming_an_unmarked_block_marks_nothing() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);

        let claimed = marks.claim([9, 9, 9], PawnId(0));

        assert!(!claimed);
        assert!(!marks.contains([9, 9, 9]));
        assert_eq!(unclaimed(&marks), [[1, 2, 3]]);
    }

    #[test]
    fn releasing_an_unclaimed_or_unmarked_block_changes_nothing() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);
        marks.toggle([4, 5, 6]);
        marks.claim([4, 5, 6], PawnId(2));

        marks.release([1, 2, 3]);
        marks.release([8, 8, 8]);

        assert_eq!(unclaimed(&marks), [[1, 2, 3]]);
        assert_eq!(claimed_by(&marks, [4, 5, 6]), Some(PawnId(2)));
        assert!(!marks.contains([8, 8, 8]));
    }

    #[test]
    fn cancelling_a_claimed_mark_drops_the_claim_with_it() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);
        marks.claim([1, 2, 3], PawnId(0));

        marks.toggle([1, 2, 3]);
        marks.toggle([1, 2, 3]);

        assert_eq!(
            claimed_by(&marks, [1, 2, 3]),
            None,
            "a re-marked block starts unclaimed"
        );
        assert_eq!(unclaimed(&marks), [[1, 2, 3]]);
    }

    #[test]
    fn unclaimed_marks_come_in_key_order() {
        let mut marks = MineMarks::default();
        for pos in [[5, 0, 0], [-1, 9, 9], [5, -3, 2], [0, 0, 0]] {
            marks.toggle(pos);
        }
        marks.claim([0, 0, 0], PawnId(0));

        assert_eq!(unclaimed(&marks), [[-1, 9, 9], [5, -3, 2], [5, 0, 0]]);
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
