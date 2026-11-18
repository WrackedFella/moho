//! Colony state: the orders the player has given the world, independent of any pawn.

use std::collections::BTreeMap;

use crate::inventory::{Inventory, ResourceYield};
use crate::pawn::PawnId;

/// The orders a colony holds, and the resources it has gathered.
#[derive(Debug, Default)]
pub struct Colony {
    pub marks: MineMarks,
    /// The colony total of each resource; only pawn jobs add to it.
    pub stock: Inventory,
}

impl Colony {
    /// Adds a mining yield to the colony's stock.
    pub fn deposit(&mut self, _yield: ResourceYield) {}
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
    /// The pawn that is working on this mark, if any.
    pub claimed_by: Option<PawnId>,
}

/// The blocks marked for mining, keyed by block position.
#[derive(Debug, Default)]
pub struct MineMarks {
    marks: BTreeMap<[i32; 3], MineMark>,
}

impl MineMarks {
    /// Marks an unmarked block and cancels a marked one.
    pub fn toggle(&mut self, pos: [i32; 3]) -> MarkChange {
        if self.marks.remove(&pos).is_some() {
            MarkChange::Cancelled
        } else {
            self.marks.insert(pos, MineMark::default());
            MarkChange::Marked
        }
    }

    pub fn contains(&self, pos: [i32; 3]) -> bool {
        self.marks.contains_key(&pos)
    }

    /// The marked blocks in key order.
    pub fn iter(&self) -> impl Iterator<Item = ([i32; 3], &MineMark)> {
        self.marks.iter().map(|(pos, mark)| (*pos, mark))
    }

    /// Gives an unclaimed mark to `pawn`. Returns whether the claim was made: `false` when
    /// the block is not marked or another pawn (or this one) already holds it.
    pub fn claim(&mut self, _pos: [i32; 3], _pawn: PawnId) -> bool {
        false
    }

    /// Frees the claim on a mark and keeps the mark; does nothing for an unmarked block.
    pub fn release(&mut self, _pos: [i32; 3]) {}

    /// The marks no pawn has claimed, in key order.
    pub fn unclaimed(&self) -> impl Iterator<Item = [i32; 3]> {
        self.marks.keys().copied()
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
            .find(|(p, _)| *p == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    #[test]
    fn stock_grows_by_the_yield() {
        let mut colony = Colony::default();

        colony.deposit(ResourceYield {
            resource_id: 7,
            amount: 1,
        });
        colony.deposit(ResourceYield {
            resource_id: 7,
            amount: 2,
        });
        colony.deposit(ResourceYield {
            resource_id: 3,
            amount: 1,
        });

        assert_eq!(colony.stock.count(7), 3);
        assert_eq!(colony.stock.count(3), 1);
        assert_eq!(colony.stock.count(9), 0, "other resources stay at zero");
    }

    #[test]
    fn claim_and_release_round_trip() {
        let mut marks = MineMarks::default();
        marks.toggle([1, 1, 1]);
        marks.toggle([2, 1, 1]);

        let first = marks.claim([1, 1, 1], PawnId(0));
        let stolen = marks.claim([1, 1, 1], PawnId(1));
        let absent = marks.claim([9, 9, 9], PawnId(1));

        assert!(first, "an unclaimed mark can be claimed");
        assert!(!stolen, "a claimed mark cannot be claimed again");
        assert!(!absent, "an unmarked block cannot be claimed");
        assert_eq!(claimed_by(&marks, [1, 1, 1]), Some(PawnId(0)));
        assert_eq!(claimed_by(&marks, [2, 1, 1]), None);
        assert_eq!(marks.unclaimed().collect::<Vec<_>>(), [[2, 1, 1]]);

        marks.release([1, 1, 1]);

        assert!(marks.contains([1, 1, 1]), "releasing keeps the mark");
        assert_eq!(claimed_by(&marks, [1, 1, 1]), None);
        assert_eq!(
            marks.unclaimed().collect::<Vec<_>>(),
            [[1, 1, 1], [2, 1, 1]]
        );
        assert!(marks.claim([1, 1, 1], PawnId(1)), "a released mark is free");
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
