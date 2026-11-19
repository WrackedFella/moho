//! Where a pawn can stand and walk on the voxel grid: standable cells and a route search
//! over them. A pawn rule, so it lives with the strategy line rather than in the voxel crate.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// The most cells a [`route`] search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground in the cell below and air in
/// the three cells from the feet up (the capsule is 2.3 m tall).
pub fn standable(_grid: &VoxelGrid, cell: BlockPos) -> bool {
    todo!("standable {cell:?}")
}

/// The cell a pawn whose feet are at `feet` stands in: each axis floored.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    todo!("feet_cell {feet:?}")
}

/// A route over standable cells from `start` to the first cell `is_goal` accepts, the
/// start and the goal cell included, or `None` when there is none within `budget`
/// expansions.
///
/// Moves go to one of four horizontal neighbours, up or down at most one block; a move up
/// needs a fourth cell of air above the current feet. A `start` that is not standable
/// is tried one cell above and one below before giving up. `toward` steers the search
/// (Manhattan distance); the same grid always gives the same route.
pub fn route(
    _grid: &VoxelGrid,
    start: BlockPos,
    _is_goal: impl Fn(BlockPos) -> bool,
    toward: BlockPos,
    budget: usize,
) -> Option<Vec<BlockPos>> {
    todo!("route {start:?} toward {toward:?} within {budget}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const ARENA: i32 = 10;

    /// One block of ground under every column of the arena: feet cells sit at y = 0.
    fn flat() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in -ARENA..=ARENA {
            for z in -ARENA..=ARENA {
                grid.mutator().place(BlockPos::new(x, -1, z), 1, None);
            }
        }
        grid
    }

    fn fill(
        grid: &mut VoxelGrid,
        xs: std::ops::RangeInclusive<i32>,
        ys: std::ops::RangeInclusive<i32>,
        zs: std::ops::RangeInclusive<i32>,
    ) {
        for x in xs {
            for y in ys.clone() {
                for z in zs.clone() {
                    grid.mutator().place(BlockPos::new(x, y, z), 1, None);
                }
            }
        }
    }

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn route_to(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let open = flat();
        let mut low_ceiling = flat();
        low_ceiling.mutator().place(cell(0, 2, 0), 1, None);
        let mut just_clear = flat();
        just_clear.mutator().place(cell(0, 3, 0), 1, None);
        let mut head_in_block = flat();
        head_in_block.mutator().place(cell(0, 1, 0), 1, None);
        let mut feet_in_block = flat();
        feet_in_block.mutator().place(cell(0, 0, 0), 1, None);

        assert!(standable(&open, cell(0, 0, 0)), "ground below, air above");
        assert!(!standable(&open, cell(0, 1, 0)), "nothing below the feet");
        assert!(!standable(&open, cell(ARENA + 5, 0, 0)), "off the ground");
        assert!(
            !standable(&low_ceiling, cell(0, 0, 0)),
            "block in the third cell of air"
        );
        assert!(
            standable(&just_clear, cell(0, 0, 0)),
            "a fourth cell is not needed"
        );
        assert!(
            !standable(&head_in_block, cell(0, 0, 0)),
            "block in the second cell"
        );
        assert!(
            !standable(&feet_in_block, cell(0, 0, 0)),
            "block in the feet cell"
        );
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        assert_eq!(feet_cell(Vec3::new(2.9, 0.0, 0.5)), cell(2, 0, 0));
        assert_eq!(feet_cell(Vec3::new(-0.2, 1.99, 3.0)), cell(-1, 1, 3));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = flat();

        let found = route_to(&grid, cell(0, 0, 0), cell(5, 0, 0));

        let expected: Vec<_> = (0..=5).map(|x| cell(x, 0, 0)).collect();
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let grid = rise();

        let up = route_to(&grid, cell(0, 0, 0), cell(6, 1, 0));
        let down = route_to(&grid, cell(6, 1, 0), cell(0, 0, 0));

        let climb = [
            cell(0, 0, 0),
            cell(1, 0, 0),
            cell(2, 0, 0),
            cell(3, 1, 0),
            cell(4, 1, 0),
            cell(5, 1, 0),
            cell(6, 1, 0),
        ];
        assert_eq!(up, Some(climb.to_vec()));
        let mut descent = climb.to_vec();
        descent.reverse();
        assert_eq!(down, Some(descent));
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = flat();
        fill(&mut grid, 3..=3, 0..=1, -ARENA..=ARENA);

        let found = route_to(&grid, cell(0, 0, 0), cell(6, 0, 0));

        assert_eq!(found, None);
    }

    #[test]
    fn a_route_through_a_gap_off_to_the_side_is_the_shortest() {
        let mut grid = flat();
        fill(&mut grid, 3..=3, 0..=1, -ARENA..=5);
        fill(&mut grid, 3..=3, 0..=1, 7..=ARENA);

        let found = route_to(&grid, cell(0, 0, 0), cell(6, 0, 0)).expect("the gap is a way round");

        assert_eq!(
            found.len(),
            19,
            "6 across and 6 each way to the gap and back"
        );
        assert_eq!(found.first().copied(), Some(cell(0, 0, 0)));
        assert_eq!(found.last().copied(), Some(cell(6, 0, 0)));
        assert!(found.contains(&cell(3, 0, 6)), "through the gap");
    }

    #[test]
    fn climbing_needs_headroom() {
        let open_rise = rise();
        let mut ceilinged_rise = rise();
        fill(&mut ceilinged_rise, 2..=2, 3..=3, -ARENA..=ARENA);

        let open = route_to(&open_rise, cell(0, 0, 0), cell(6, 1, 0));
        let ceilinged = route_to(&ceilinged_rise, cell(0, 0, 0), cell(6, 1, 0));

        assert!(open.is_some(), "the rise itself is climbable");
        assert_eq!(
            ceilinged, None,
            "no fourth cell of air above the foot of the rise"
        );
    }

    /// Flat ground with one more block of ground from x = 3 on.
    fn rise() -> VoxelGrid {
        let mut grid = flat();
        fill(&mut grid, 3..=ARENA, 0..=0, -ARENA..=ARENA);
        grid
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = flat();
        fill(&mut grid, 4..=6, 0..=1, -1..=1);
        grid.mutator().remove(cell(5, 0, 0));
        grid.mutator().remove(cell(5, 1, 0));

        let found = route_to(&grid, cell(0, 0, 0), cell(5, 0, 0));

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = flat();

        let starved = route(
            &grid,
            cell(-9, 0, 0),
            |c| c == cell(9, 0, 0),
            cell(9, 0, 0),
            10,
        );
        let fed = route(
            &grid,
            cell(-9, 0, 0),
            |c| c == cell(9, 0, 0),
            cell(9, 0, 0),
            1000,
        );

        assert_eq!(starved, None);
        assert_eq!(fed.map(|r| r.len()), Some(19));
    }

    #[test]
    fn a_start_in_the_ground_or_just_above_it_is_moved_onto_the_ground() {
        let grid = flat();

        let from_inside = route_to(&grid, cell(0, -1, 0), cell(3, 0, 0));
        let from_above = route_to(&grid, cell(0, 1, 0), cell(3, 0, 0));
        let from_far_above = route_to(&grid, cell(0, 4, 0), cell(3, 0, 0));

        assert_eq!(
            from_inside.and_then(|r| r.first().copied()),
            Some(cell(0, 0, 0))
        );
        assert_eq!(
            from_above.and_then(|r| r.first().copied()),
            Some(cell(0, 0, 0))
        );
        assert_eq!(from_far_above, None);
    }

    /// A rolling arena: the ground in column (x, z) tops out at `heights` in -2..=1,
    /// so neighbours differ by up to three blocks.
    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        let side = 7;
        for (i, &top) in heights.iter().enumerate() {
            let (x, z) = ((i as i32) % side, (i as i32) / side);
            for y in -3..=top {
                grid.mutator().place(cell(x, y, z), 1, None);
            }
        }
        grid
    }

    fn surface_cell(heights: &[i32], x: i32, z: i32) -> BlockPos {
        cell(x, heights[(z * 7 + x) as usize] + 1, z)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(-2i32..=1, 49),
            (sx, sz, gx, gz) in (0i32..7, 0i32..7, 0i32..7, 0i32..7),
        ) {
            let grid = terrain(&heights);
            let start = surface_cell(&heights, sx, sz);
            let goal = surface_cell(&heights, gx, gz);

            let found = route_to(&grid, start, goal);

            if let Some(cells) = found {
                prop_assert_eq!(cells.first().copied(), Some(start));
                prop_assert_eq!(cells.last().copied(), Some(goal));
                for c in &cells {
                    prop_assert!(standable(&grid, *c), "{:?} is not standable", c);
                }
                for pair in cells.windows(2) {
                    let step = pair[1] - pair[0];
                    prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "{:?} to {:?}", pair[0], pair[1]);
                    prop_assert!(step.y.abs() <= 1, "{:?} to {:?}", pair[0], pair[1]);
                }
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(-1i32..=0, 49),
            (sx, sz, gx, gz) in (0i32..7, 0i32..7, 0i32..7, 0i32..7),
        ) {
            let first_grid = terrain(&heights);
            let second_grid = terrain(&heights);
            let start = surface_cell(&heights, sx, sz);
            let goal = surface_cell(&heights, gx, gz);

            let first = route_to(&first_grid, start, goal);
            let again = route_to(&first_grid, start, goal);
            let rebuilt = route_to(&second_grid, start, goal);

            prop_assert!(first.is_some(), "all-reachable terrain has a route");
            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &rebuilt);
        }
    }
}
