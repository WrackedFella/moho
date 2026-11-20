//! Colony state: the orders the player has given the world, independent of any pawn.

use std::collections::BTreeSet;

use crate::inventory::Inventory;
use crate::pawn::PawnId;

/// The orders a colony holds, and what its pawns have brought in.
#[derive(Debug, Default)]
pub struct Colony {
    pub marks: MineMarks,
    /// The colony total of each resource; only pawn jobs add to it.
    pub stock: Inventory,
}

/// What a mark order does to a block that is, or is not, marked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkChange {
    Marked,
    Cancelled,
}

/// An order to dig one block, and the pawn that has taken it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MineMark {
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

    /// Gives the mark on `pos` to `pawn`. `false` when the block is not marked or another
    /// pawn already holds the mark.
    pub fn claim(&mut self, _pos: [i32; 3], _pawn: PawnId) -> bool {
        todo!("claims are not stored yet")
    }

    /// Frees the claim on `pos`; the mark stays. Does nothing for an unmarked or unclaimed block.
    pub fn release(&mut self, _pos: [i32; 3]) {
        todo!("claims are not stored yet")
    }

    /// The marked blocks no pawn holds, in key order.
    pub fn unclaimed(&self) -> impl Iterator<Item = [i32; 3]> {
        self.marks.iter().copied()
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

    fn holder(marks: &MineMarks, pos: [i32; 3]) -> Option<Option<PawnId>> {
        marks
            .iter()
            .find(|(marked, _)| *marked == pos)
            .map(|(_, mark)| mark.claimed_by)
    }

    #[test]
    fn claim_and_release_round_trip() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);
        marks.toggle([4, 5, 6]);

        let first = marks.claim([1, 2, 3], PawnId(0));
        let held = holder(&marks, [1, 2, 3]);
        let open: Vec<_> = marks.unclaimed().collect();
        let taken = marks.claim([1, 2, 3], PawnId(1));
        let still_held = holder(&marks, [1, 2, 3]);
        let unmarked = marks.claim([9, 9, 9], PawnId(1));
        marks.release([1, 2, 3]);
        marks.release([9, 9, 9]);

        assert!(first, "an unclaimed mark can be claimed");
        assert_eq!(held, Some(Some(PawnId(0))));
        assert_eq!(open, [[4, 5, 6]], "a claimed mark is not offered");
        assert!(!taken, "a claimed mark cannot be claimed by another pawn");
        assert_eq!(still_held, Some(Some(PawnId(0))), "the first pawn keeps it");
        assert!(!unmarked, "an unmarked block cannot be claimed");
        assert_eq!(holder(&marks, [1, 2, 3]), Some(None), "release frees it");
        assert_eq!(
            marked(&marks),
            [[1, 2, 3], [4, 5, 6]],
            "release keeps marks"
        );
        assert_eq!(
            marks.unclaimed().collect::<Vec<_>>(),
            [[1, 2, 3], [4, 5, 6]]
        );
        assert!(
            marks.claim([1, 2, 3], PawnId(1)),
            "a freed mark is open again"
        );
    }

    #[test]
    fn a_removed_mark_takes_its_claim_with_it() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);
        marks.claim([1, 2, 3], PawnId(0));

        marks.block_removed([1, 2, 3]);
        marks.toggle([1, 2, 3]);

        assert_eq!(holder(&marks, [1, 2, 3]), Some(None));
    }

    #[test]
    fn a_cancelled_and_re_marked_block_starts_unclaimed() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 2, 3]);
        marks.claim([1, 2, 3], PawnId(0));

        marks.toggle([1, 2, 3]);
        marks.toggle([1, 2, 3]);

        assert_eq!(holder(&marks, [1, 2, 3]), Some(None));
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
