//! Colony state: the orders the player has given the world, independent of any pawn.

use std::collections::BTreeSet;

use crate::inventory::Inventory;
use crate::pawn::PawnId;

/// The orders a colony holds, and what its pawns have brought in.
#[derive(Debug, Default)]
pub struct Colony {
    pub marks: MineMarks,
    /// The colony total of each resource, filled by pawn jobs.
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
    /// The pawn walking to this block, if any.
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

    /// Claims a marked block for `pawn`. `false` when the block is unmarked or already claimed.
    pub fn claim(&mut self, pos: [i32; 3], pawn: PawnId) -> bool {
        let _ = (pos, pawn);
        todo!("claim")
    }

    /// Frees the claim on a block; the mark stays. Does nothing for an unmarked or unclaimed block.
    pub fn release(&mut self, pos: [i32; 3]) {
        let _ = pos;
        todo!("release")
    }

    /// The marked blocks nobody has claimed, in key order.
    pub fn unclaimed(&self) -> std::vec::IntoIter<[i32; 3]> {
        todo!("unclaimed")
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

    fn claim_of(marks: &MineMarks, pos: [i32; 3]) -> Option<Option<PawnId>> {
        marks
            .iter()
            .find(|(marked, _)| *marked == pos)
            .map(|(_, mark)| mark.claimed_by)
    }

    #[test]
    fn stock_grows_by_the_yield() {
        let mut colony = Colony::default();

        colony.stock.add(7, 1);
        colony.stock.add(7, 2);
        colony.stock.add(9, 5);

        assert_eq!(colony.stock.count(7), 3);
        assert_eq!(colony.stock.count(9), 5);
        assert_eq!(colony.stock.count(1), 0);
    }

    #[test]
    fn claim_and_release_round_trip() {
        let mut marks = MineMarks::default();
        marks.toggle([3, 4, 5]);
        marks.toggle([1, 1, 1]);

        let claimed = marks.claim([3, 4, 5], PawnId(0));
        let taken: Vec<[i32; 3]> = marks.unclaimed().collect();
        let second_claim = marks.claim([3, 4, 5], PawnId(1));
        let unmarked_claim = marks.claim([9, 9, 9], PawnId(0));
        marks.release([3, 4, 5]);
        let freed: Vec<[i32; 3]> = marks.unclaimed().collect();

        assert!(claimed);
        assert_eq!(claim_of(&marks, [1, 1, 1]), Some(None));
        assert_eq!(taken, [[1, 1, 1]], "a claimed mark is not offered");
        assert!(!second_claim, "a claim is not taken from its pawn");
        assert!(!unmarked_claim, "nothing to claim at an unmarked block");
        assert!(!marks.contains([9, 9, 9]));
        assert_eq!(freed, [[1, 1, 1], [3, 4, 5]], "release keeps the mark");
        assert_eq!(claim_of(&marks, [3, 4, 5]), Some(None));
    }

    #[test]
    fn the_claim_names_its_pawn() {
        let mut marks = MineMarks::default();
        marks.toggle([3, 4, 5]);

        marks.claim([3, 4, 5], PawnId(2));

        assert_eq!(claim_of(&marks, [3, 4, 5]), Some(Some(PawnId(2))));
    }

    #[test]
    fn releasing_an_unclaimed_or_unmarked_block_changes_nothing() {
        let mut marks = MineMarks::default();
        marks.toggle([3, 4, 5]);

        marks.release([3, 4, 5]);
        marks.release([8, 8, 8]);

        assert_eq!(marked(&marks), [[3, 4, 5]]);
        assert_eq!(claim_of(&marks, [3, 4, 5]), Some(None));
    }

    #[test]
    fn a_cancelled_mark_forgets_its_claim() {
        let mut marks = MineMarks::default();
        marks.toggle([3, 4, 5]);
        marks.claim([3, 4, 5], PawnId(0));

        marks.toggle([3, 4, 5]);
        marks.toggle([3, 4, 5]);

        assert_eq!(claim_of(&marks, [3, 4, 5]), Some(None));
    }

    #[test]
    fn a_removed_block_takes_its_claim_with_its_mark() {
        let mut marks = MineMarks::default();
        marks.toggle([3, 4, 5]);
        marks.claim([3, 4, 5], PawnId(0));

        marks.block_removed([3, 4, 5]);

        assert_eq!(claim_of(&marks, [3, 4, 5]), None);
        assert_eq!(marks.unclaimed().count(), 0);
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
