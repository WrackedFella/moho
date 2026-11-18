//! Where a pawn can walk: standable cells and the shortest way between them.
//!
//! A cell is the block position of a pawn's feet. Reading the grid through `is_solid_at`
//! keeps this a pawn rule on the strategy line rather than a property of the voxel store.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Node expansions a search may spend before it gives up with `None`.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground below and three cells of air
/// from the feet up (the capsule is 2.3 m tall).
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    false
}

/// The cell a pawn's feet are in.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// The cells to walk through, in order, from `start` to the nearest cell that satisfies
/// `is_goal`; empty when `start` already does.
///
/// A* over standable cells with four horizontal neighbours, stepping up or down one block
/// (up needs a fourth cell of air above the current feet), a Manhattan heuristic towards
/// `toward`, and ties broken by cost then insertion order so a grid always gives the same
/// route. A `start` that is not standable tries the cell above and the cell below it.
/// `None` when no route exists or `budget` expansions are spent first.
pub fn route(
    _grid: &VoxelGrid,
    _start: BlockPos,
    _is_goal: impl Fn(BlockPos) -> bool,
    _toward: BlockPos,
    _budget: usize,
) -> Option<Vec<BlockPos>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(BlockPos::new(x, y, z), 1, None);
    }

    /// A floor of blocks at y = 0 spanning `0..width` by `0..depth`, so feet cells are y = 1.
    fn floor(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    fn goal_at(target: BlockPos) -> impl Fn(BlockPos) -> bool {
        move |cell| cell == target
    }

    fn cells(list: &[(i32, i32, i32)]) -> Vec<BlockPos> {
        list.iter()
            .map(|&(x, y, z)| BlockPos::new(x, y, z))
            .collect()
    }

    fn run(
        grid: &VoxelGrid,
        start: (i32, i32, i32),
        goal: (i32, i32, i32),
    ) -> Option<Vec<BlockPos>> {
        let goal = BlockPos::new(goal.0, goal.1, goal.2);
        route(
            grid,
            BlockPos::new(start.0, start.1, start.2),
            goal_at(goal),
            goal,
            ROUTE_NODE_BUDGET,
        )
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = floor(5, 5);
        place(&mut grid, 1, 1, 1);
        place(&mut grid, 3, 2, 3);
        place(&mut grid, 4, 4, 4);

        let open = standable(&grid, BlockPos::new(2, 1, 2));
        let air_below = standable(&grid, BlockPos::new(2, 2, 2));
        let inside_ground = standable(&grid, BlockPos::new(2, 0, 2));
        let on_a_block = standable(&grid, BlockPos::new(1, 2, 1));
        let inside_a_block = standable(&grid, BlockPos::new(1, 1, 1));
        let low_ceiling = standable(&grid, BlockPos::new(3, 1, 3));
        let ceiling_above_the_capsule = standable(&grid, BlockPos::new(4, 1, 4));

        assert!(open, "air from the feet up over solid ground");
        assert!(!air_below, "no ground under the feet");
        assert!(!inside_ground, "the feet cell must be air");
        assert!(on_a_block, "a placed block is ground too");
        assert!(!inside_a_block, "the feet cell must be air");
        assert!(
            !low_ceiling,
            "a block two cells above the feet touches the head"
        );
        assert!(
            ceiling_above_the_capsule,
            "three cells of air is enough; a block above that is not in the way"
        );
        assert!(
            !standable(&grid, BlockPos::new(2, 1, 9)),
            "off the floor there is nothing to stand on"
        );
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = floor(8, 3);

        let found = run(&grid, (0, 1, 0), (4, 1, 0));

        assert_eq!(
            found,
            Some(cells(&[(1, 1, 0), (2, 1, 0), (3, 1, 0), (4, 1, 0)]))
        );
    }

    #[test]
    fn a_route_from_the_goal_is_empty() {
        let grid = floor(4, 4);

        let found = run(&grid, (2, 1, 2), (2, 1, 2));

        assert_eq!(found, Some(Vec::new()));
    }

    /// A floor three wide with a one-block rise across x = 2..=4.
    fn rise() -> VoxelGrid {
        let mut grid = floor(8, 3);
        for x in 2..=4 {
            for z in 0..3 {
                place(&mut grid, x, 1, z);
            }
        }
        grid
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let grid = rise();

        let found = run(&grid, (0, 1, 0), (6, 1, 0));

        assert_eq!(
            found,
            Some(cells(&[
                (1, 1, 0),
                (2, 2, 0),
                (3, 2, 0),
                (4, 2, 0),
                (5, 1, 0),
                (6, 1, 0),
            ]))
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = floor(8, 3);
        for z in 0..3 {
            place(&mut grid, 3, 1, z);
            place(&mut grid, 3, 2, z);
        }

        let found = run(&grid, (0, 1, 1), (6, 1, 1));

        assert_eq!(found, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = rise();
        assert!(
            run(&grid, (0, 1, 0), (6, 1, 0)).is_some(),
            "the rise is passable while the air above the foot of it is open"
        );
        for z in 0..3 {
            place(&mut grid, 1, 4, z);
        }

        let found = run(&grid, (0, 1, 0), (6, 1, 0));

        assert_eq!(found, None, "a fourth cell of air is needed to step up");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = floor(9, 9);
        assert!(run(&grid, (0, 1, 0), (4, 1, 4)).is_some());
        for x in 3..=5 {
            for z in 3..=5 {
                if (x, z) != (4, 4) {
                    place(&mut grid, x, 1, z);
                    place(&mut grid, x, 2, z);
                }
            }
        }

        let found = run(&grid, (0, 1, 0), (4, 1, 4));

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = floor(100, 100);
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(99, 1, 0);
        // A heuristic that pulls away from the goal makes the search cover the floor.
        let misleading = BlockPos::new(0, 1, 99);

        let within_budget = route(&grid, start, goal_at(goal), misleading, ROUTE_NODE_BUDGET);
        let unlimited = route(&grid, start, goal_at(goal), misleading, 100_000);
        let tiny = route(
            &grid,
            BlockPos::new(0, 1, 0),
            goal_at(BlockPos::new(6, 1, 0)),
            BlockPos::new(6, 1, 0),
            3,
        );
        let enough = route(
            &grid,
            BlockPos::new(0, 1, 0),
            goal_at(BlockPos::new(6, 1, 0)),
            BlockPos::new(6, 1, 0),
            50,
        );

        assert_eq!(
            within_budget, None,
            "4096 expansions do not cover the floor"
        );
        assert_eq!(
            unlimited.map(|r| r.len()),
            Some(99),
            "the goal is reachable"
        );
        assert_eq!(tiny, None, "three expansions cannot reach six cells away");
        assert_eq!(enough.map(|r| r.len()), Some(6));
    }

    /// Columns of height 1 or 2 over a 6 by 6 board: neighbours never differ by more than one.
    fn terrain(heights: &[bool]) -> (VoxelGrid, Vec<i32>) {
        let mut grid = VoxelGrid::new(16);
        let mut tops = Vec::new();
        for (i, &high) in heights.iter().enumerate() {
            let (x, z) = ((i % 6) as i32, (i / 6) as i32);
            let top = if high { 2 } else { 1 };
            for y in 0..top {
                place(&mut grid, x, y, z);
            }
            tops.push(top);
        }
        (grid, tops)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(any::<bool>(), 36),
            from in (0..6i32, 0..6i32),
            to in (0..6i32, 0..6i32),
        ) {
            let (grid, tops) = terrain(&heights);
            let top = |(x, z): (i32, i32)| tops[(z * 6 + x) as usize];
            let start = BlockPos::new(from.0, top(from), from.1);
            let goal = BlockPos::new(to.0, top(to), to.1);

            let found = route(
                &grid,
                start,
                |cell| cell.x == goal.x && cell.z == goal.z,
                goal,
                ROUTE_NODE_BUDGET,
            );

            let path = found.expect("every column is within a step of its neighbours");
            let mut previous = start;
            for cell in &path {
                prop_assert!(standable(&grid, *cell), "{cell} is not standable");
                let step = *cell - previous;
                prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "{} to {}", previous, cell);
                prop_assert!(step.y.abs() <= 1, "{} to {}", previous, cell);
                previous = *cell;
            }
            prop_assert_eq!((previous.x, previous.z), (goal.x, goal.z));
            prop_assert!(path.len() as i32 >= (to.0 - from.0).abs() + (to.1 - from.1).abs());
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(any::<bool>(), 36),
            from in (0..6i32, 0..6i32),
            to in (0..6i32, 0..6i32),
        ) {
            let (first, tops) = terrain(&heights);
            let (second, _) = terrain(&heights);
            let top = |(x, z): (i32, i32)| tops[(z * 6 + x) as usize];
            let start = BlockPos::new(from.0, top(from), from.1);
            let goal = BlockPos::new(to.0, top(to), to.1);
            let search = |grid: &VoxelGrid| {
                route(grid, start, |c| c.x == goal.x && c.z == goal.z, goal, ROUTE_NODE_BUDGET)
            };

            let a = search(&first);
            let b = search(&second);

            prop_assert!(a.is_some(), "a route exists");
            prop_assert_eq!(a, b);
        }
    }
}
