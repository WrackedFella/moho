//! The colony's pawns: a store addressed by [`PawnId`], and where a new crew stands.

use std::collections::BTreeMap;

use flagstone_voxel::{BlockPos, VoxelGrid};

use crate::pawn::{Pawn, PawnId};
use crate::routing::standable;

/// How many pawns a new world starts with.
pub const CREW_SIZE: usize = 3;

/// Every pawn of the colony, by the id the simulation issued it. Iterates in id order.
#[derive(Debug, Default)]
pub struct Crew {
    pawns: BTreeMap<PawnId, Pawn>,
    next: u32,
}

impl Crew {
    /// Adds `pawn` under the next id; ids ascend from `PawnId(0)` and are never reused.
    pub fn spawn(&mut self, pawn: Pawn) -> PawnId {
        let id = PawnId(self.next);
        self.next += 1;
        self.pawns.insert(id, pawn);
        id
    }

    pub fn get(&self, id: PawnId) -> Option<&Pawn> {
        self.pawns.get(&id)
    }

    pub fn get_mut(&mut self, id: PawnId) -> Option<&mut Pawn> {
        self.pawns.get_mut(&id)
    }

    pub fn ids(&self) -> impl Iterator<Item = PawnId> + '_ {
        self.pawns.keys().copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (PawnId, &Pawn)> {
        self.pawns.iter().map(|(&id, pawn)| (id, pawn))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (PawnId, &mut Pawn)> {
        self.pawns.iter_mut().map(|(&id, pawn)| (id, pawn))
    }

    pub fn len(&self) -> usize {
        self.pawns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pawns.is_empty()
    }
}

/// How far from `around`, in columns and in blocks of height, [`spawn_cells`] looks.
const SPAWN_REACH: i32 = 8;

/// The `count` standable cells nearest `around`, one per column, ordered by horizontal
/// distance and then by key. Looks at columns within 8 blocks and heights within 8 of `around`;
/// returns fewer cells when fewer exist. Of a column's standable cells it takes the one
/// nearest `around`'s height, the lower one on a tie.
pub fn spawn_cells(grid: &VoxelGrid, around: BlockPos, count: usize) -> Vec<BlockPos> {
    let mut found: Vec<(i32, BlockPos)> = (-SPAWN_REACH..=SPAWN_REACH)
        .flat_map(|dx| (-SPAWN_REACH..=SPAWN_REACH).map(move |dz| (dx, dz)))
        .filter_map(|(dx, dz)| {
            let (x, z) = (around.x + dx, around.z + dz);
            (around.y - SPAWN_REACH..=around.y + SPAWN_REACH)
                .map(|y| BlockPos::new(x, y, z))
                .filter(|&cell| standable(grid, cell))
                .min_by_key(|cell| ((cell.y - around.y).abs(), cell.y))
                .map(|cell| (dx * dx + dz * dz, cell))
        })
        .collect();
    found.sort_unstable_by_key(|&(distance, cell)| (distance, cell.to_array()));
    found
        .into_iter()
        .take(count)
        .map(|(_, cell)| cell)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::standable;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    const STONE: u32 = 1;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn ground(min: (i32, i32), max: (i32, i32)) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(min.0, 0, min.1), cell(max.0, 0, max.1), STONE);
        grid
    }

    fn columns(cells: &[BlockPos]) -> Vec<(i32, i32)> {
        cells.iter().map(|c| (c.x, c.z)).collect()
    }

    fn squared_distance(from: BlockPos, to: BlockPos) -> i32 {
        (to.x - from.x).pow(2) + (to.z - from.z).pow(2)
    }

    #[test]
    fn ids_are_issued_in_ascending_order_and_iterate_in_id_order() {
        let mut crew = Crew::default();
        let mut first = Pawn::default();
        first.select_slot(1);
        let mut second = Pawn::default();
        second.select_slot(2);
        let mut third = Pawn::default();
        third.select_slot(3);

        let issued = [crew.spawn(first), crew.spawn(second), crew.spawn(third)];

        assert_eq!(issued, [PawnId(0), PawnId(1), PawnId(2)]);
        assert_eq!(crew.len(), 3);
        assert_eq!(crew.ids().collect::<Vec<_>>(), issued);
        let slots: Vec<(PawnId, usize)> = crew
            .iter()
            .map(|(id, pawn)| (id, pawn.selected_slot))
            .collect();
        assert_eq!(
            slots,
            [(PawnId(0), 1), (PawnId(1), 2), (PawnId(2), 3)],
            "each id keeps the pawn it was issued for"
        );
        assert!(crew.get(PawnId(3)).is_none(), "no pawn beyond the crew");
    }

    #[test]
    fn spawn_cells_are_standable_distinct_columns_nearest_first() {
        let mut grid = ground((-6, -6), (6, 6));
        // A platform over column (1, 0) makes two cells standable there: y 1 and y 5.
        grid.mutator()
            .fill_region(cell(1, 4, 0), cell(1, 4, 0), STONE);

        let cells = spawn_cells(&grid, cell(0, 1, 0), 5);
        let from_between = spawn_cells(&grid, cell(0, 3, 0), 5);

        assert_eq!(
            columns(&cells),
            [(0, 0), (-1, 0), (0, -1), (0, 1), (1, 0)],
            "by horizontal distance, ties by key"
        );
        for c in &cells {
            assert!(standable(&grid, *c), "{c} is not ground to stand on");
        }
        assert_eq!(
            cells[4],
            cell(1, 1, 0),
            "of a column's standable cells, the one nearest around's height"
        );
        let platform_column: Vec<BlockPos> = from_between
            .into_iter()
            .filter(|c| (c.x, c.z) == (1, 0))
            .collect();
        assert_eq!(
            platform_column,
            [cell(1, 1, 0)],
            "two cells equally near around's height: the lower one"
        );
    }

    #[test]
    fn spawn_cells_take_the_higher_cell_of_a_column_when_around_is_nearer_to_it() {
        let mut grid = ground((-2, -2), (2, 2));
        // Column (1, 0) has standable cells at y 1 and y 5.
        grid.mutator()
            .fill_region(cell(1, 4, 0), cell(1, 4, 0), STONE);

        let cells = spawn_cells(&grid, cell(0, 5, 0), 5);

        let platform_column: Vec<BlockPos> =
            cells.into_iter().filter(|c| (c.x, c.z) == (1, 0)).collect();
        assert_eq!(platform_column, [cell(1, 5, 0)]);
    }

    #[test]
    fn get_mut_changes_the_pawn_get_returns_and_no_other() {
        let mut crew = Crew::default();
        let first = crew.spawn(Pawn::default());
        let second = crew.spawn(Pawn::default());
        let untouched = crew.get(second).expect("the second pawn").selected_slot;

        crew.get_mut(first).expect("the first pawn").select_slot(2);

        assert_eq!(crew.get(first).expect("the first pawn").selected_slot, 2);
        assert_eq!(
            crew.get(second).expect("the second pawn").selected_slot,
            untouched
        );
        assert_ne!(untouched, 2, "setup: the slot change must be visible");
        assert!(crew.get_mut(PawnId(9)).is_none(), "no pawn beyond the crew");
    }

    #[test]
    fn a_crew_is_empty_until_a_pawn_spawns() {
        let mut crew = Crew::default();
        assert!(crew.is_empty());

        crew.spawn(Pawn::default());

        assert!(!crew.is_empty());
    }

    #[test]
    fn spawn_cells_start_at_the_nearest_standable_column_when_around_is_not_ground() {
        let mut grid = ground((-6, -6), (6, 6));
        grid.mutator().remove(cell(0, 0, 0));
        grid.mutator().remove(cell(-1, 0, 0));

        let cells = spawn_cells(&grid, cell(0, 1, 0), 3);

        assert_eq!(columns(&cells), [(0, -1), (0, 1), (1, 0)]);
    }

    #[test]
    fn spawn_cells_returns_fewer_when_the_ground_runs_out() {
        let grid = ground((0, 0), (1, 1));

        let cells = spawn_cells(&grid, cell(0, 1, 0), 6);

        assert_eq!(
            columns(&cells),
            [(0, 0), (0, 1), (1, 0), (1, 1)],
            "all four columns of the island, no more"
        );
        assert!(spawn_cells(&grid, cell(0, 1, 0), 0).is_empty());
    }

    #[test]
    fn spawn_cells_look_eight_columns_out_and_no_further() {
        for (dx, found) in [(8, true), (9, false), (-8, true), (-9, false)] {
            let grid = ground((dx, 0), (dx, 0));

            let cells = spawn_cells(&grid, cell(0, 1, 0), 1);

            assert_eq!(
                columns(&cells),
                if found { vec![(dx, 0)] } else { Vec::new() },
                "ground {dx} columns away"
            );
        }
    }

    #[test]
    fn spawn_cells_look_eight_blocks_up_and_down_and_no_further() {
        for (dy, found) in [(8, true), (9, false), (-8, true), (-9, false)] {
            let mut grid = VoxelGrid::new(16);
            grid.mutator().place(cell(2, 1 + dy - 1, 0), STONE, None);

            let cells = spawn_cells(&grid, cell(0, 1, 0), 1);

            assert_eq!(
                cells,
                if found {
                    vec![cell(2, 1 + dy, 0)]
                } else {
                    Vec::new()
                },
                "ground whose standing cell is {dy} blocks off"
            );
        }
    }

    proptest! {
        #[test]
        fn every_spawn_cell_is_standable(
            blocks in proptest::collection::btree_set((-4..=4i32, 1..=3i32, -4..=4i32), 0..60),
            around in (-1..=1i32, 1..=2i32, -1..=1i32),
            count in 1..=6usize,
        ) {
            let mut grid = ground((-4, -4), (4, 4));
            for &(x, y, z) in &blocks {
                grid.mutator().place(cell(x, y, z), STONE, None);
            }
            let around = cell(around.0, around.1, around.2);

            let cells = spawn_cells(&grid, around, count);

            let reachable_columns: BTreeSet<(i32, i32)> = (-4..=4)
                .flat_map(|x| (-4..=4).map(move |z| (x, z)))
                .filter(|&(x, z)| {
                    (around.y - 8..=around.y + 8).any(|y| standable(&grid, cell(x, y, z)))
                })
                .collect();
            prop_assert_eq!(cells.len(), count.min(reachable_columns.len()));
            let distinct: BTreeSet<(i32, i32)> = columns(&cells).into_iter().collect();
            prop_assert_eq!(distinct.len(), cells.len(), "one cell per column: {:?}", cells);
            for c in &cells {
                prop_assert!(standable(&grid, *c), "{} is not standable", c);
                let nearer = (around.y - 8..=around.y + 8)
                    .filter(|&y| standable(&grid, cell(c.x, y, c.z)))
                    .find(|&y| (y - around.y).abs() < (c.y - around.y).abs()
                        || ((y - around.y).abs() == (c.y - around.y).abs() && y < c.y));
                prop_assert!(
                    nearer.is_none(),
                    "{} is chosen but y {:?} in its column is nearer around's height",
                    c, nearer
                );
            }
            let order: Vec<(i32, [i32; 3])> = cells
                .iter()
                .map(|c| (squared_distance(around, *c), c.to_array()))
                .collect();
            prop_assert!(
                order.windows(2).all(|pair| pair[0] <= pair[1]),
                "not nearest first: {:?}", order
            );
        }
    }
}
