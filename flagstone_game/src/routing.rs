//! Walking routes over the voxel ground for the strategy line's pawns.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Most cells a search may expand before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground below and the capsule's
/// height of air from the feet up.
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    todo!()
}

/// The cell a pawn with its feet at `feet` stands in.
pub fn feet_cell(_feet: Vec3) -> BlockPos {
    todo!()
}

/// The standable cells from `start` to the first cell `is_goal` accepts, or `None` when
/// none is reachable within `budget` expansions. `toward` steers the search.
pub fn route(
    _grid: &VoxelGrid,
    _start: BlockPos,
    _is_goal: impl Fn(BlockPos) -> bool,
    _toward: BlockPos,
    _budget: usize,
) -> Option<Vec<BlockPos>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::ops::RangeInclusive;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(cell(x, y, z), 1, None);
    }

    /// A one-block-thick floor at `y = 0`, so feet stand in cells at `y = 1`.
    fn floor(xs: RangeInclusive<i32>, zs: RangeInclusive<i32>) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in xs {
            for z in zs.clone() {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    /// The route without its start cell, whether or not the search lists it.
    fn after_start(route: &[BlockPos], start: BlockPos) -> Vec<BlockPos> {
        route
            .iter()
            .copied()
            .skip(usize::from(route.first() == Some(&start)))
            .collect()
    }

    fn find(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = floor(0..=8, 0..=8);
        assert!(standable(&grid, cell(4, 1, 4)), "ground below, open air");
        assert!(
            !standable(&grid, cell(4, 2, 4)),
            "a cell above air has no ground"
        );
        assert!(!standable(&grid, cell(4, 0, 4)), "inside the floor");

        place(&mut grid, 4, 4, 4);
        assert!(
            standable(&grid, cell(4, 1, 4)),
            "a block three cells above the feet leaves three cells of air"
        );

        place(&mut grid, 4, 3, 4);
        assert!(
            !standable(&grid, cell(4, 1, 4)),
            "a block in the third cell of the capsule's height"
        );
        assert!(
            standable(&grid, cell(5, 1, 4)),
            "the neighbouring column is untouched"
        );
    }

    #[test]
    fn standable_rejects_a_block_in_the_second_cell_of_air() {
        let mut grid = floor(0..=8, 0..=8);
        place(&mut grid, 4, 2, 4);

        assert!(!standable(&grid, cell(4, 1, 4)));
    }

    #[test]
    fn feet_cell_floors_the_position() {
        assert_eq!(feet_cell(Vec3::new(2.9, 1.0, 2.1)), cell(2, 1, 2));
        assert_eq!(feet_cell(Vec3::new(-0.1, 4.99, -3.5)), cell(-1, 4, -4));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = floor(0..=10, 0..=8);
        let start = cell(2, 1, 2);
        let goal = cell(6, 1, 2);

        let route = find(&grid, start, goal).expect("open floor");

        assert_eq!(
            after_start(&route, start),
            [cell(3, 1, 2), cell(4, 1, 2), cell(5, 1, 2), cell(6, 1, 2)]
        );
    }

    /// A floor with a one-block rise across columns `x = 4..=5`, spanning every row.
    fn floor_with_a_rise() -> VoxelGrid {
        let mut grid = floor(0..=10, 0..=8);
        for x in 4..=5 {
            for z in 0..=8 {
                place(&mut grid, x, 1, z);
            }
        }
        grid
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let grid = floor_with_a_rise();
        let start = cell(2, 1, 2);
        let goal = cell(8, 1, 2);

        let route = find(&grid, start, goal).expect("a one-block rise is walkable");

        assert_eq!(
            after_start(&route, start),
            [
                cell(3, 1, 2),
                cell(4, 2, 2),
                cell(5, 2, 2),
                cell(6, 1, 2),
                cell(7, 1, 2),
                cell(8, 1, 2)
            ]
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut low = floor(0..=8, 0..=8);
        let mut high = floor(0..=8, 0..=8);
        for z in 0..=8 {
            place(&mut low, 4, 1, z);
            place(&mut high, 4, 1, z);
            place(&mut high, 4, 2, z);
        }
        let start = cell(2, 1, 2);
        let goal = cell(6, 1, 2);

        assert!(find(&low, start, goal).is_some(), "one block is a step");
        assert_eq!(find(&high, start, goal), None, "two blocks are a wall");
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut open = floor_with_a_rise();
        let mut capped = floor_with_a_rise();
        for z in 0..=8 {
            // Column 3 keeps its three cells of air; the fourth, the climb's, is taken.
            place(&mut capped, 3, 4, z);
        }
        let start = cell(2, 1, 2);
        let goal = cell(8, 1, 2);
        assert!(standable(&capped, cell(3, 1, 2)), "setup: still standable");
        place(&mut open, 3, 9, 2);

        assert!(
            find(&open, start, goal).is_some(),
            "a roof far above is fine"
        );
        assert_eq!(find(&capped, start, goal), None);
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = floor(0..=10, 0..=10);
        let goal = cell(6, 1, 6);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            for y in 1..=2 {
                place(&mut grid, 6 + dx, y, 6 + dz);
            }
        }

        assert!(standable(&grid, goal), "setup: the goal itself is fine");
        assert_eq!(find(&grid, cell(2, 1, 2), goal), None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = floor(0..=40, 0..=2);
        let start = cell(0, 1, 1);
        let goal = cell(40, 1, 1);

        let starved = route(&grid, start, |c| c == goal, goal, 5);
        let funded = route(&grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None, "forty steps do not fit in five expansions");
        assert!(funded.is_some());
    }

    #[test]
    fn a_start_a_cell_above_the_ground_still_routes() {
        let grid = floor(0..=10, 0..=8);
        let goal = cell(6, 1, 2);

        let route = find(&grid, cell(2, 2, 2), goal);

        assert_eq!(
            route.expect("a pawn a cell high still routes").last(),
            Some(&goal)
        );
    }

    const SIDE: i32 = 7;

    /// A terrain of columns `SIDE` by `SIDE`, each `heights[i]` blocks tall; the feet of a
    /// pawn on column `(x, z)` are in the cell at its height.
    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..SIDE {
            for z in 0..SIDE {
                for y in 0..heights[(x * SIDE + z) as usize] {
                    place(&mut grid, x, y, z);
                }
            }
        }
        grid
    }

    fn stand_on(heights: &[i32], index: usize) -> BlockPos {
        let (x, z) = (index as i32 / SIDE, index as i32 % SIDE);
        cell(x, heights[index], z)
    }

    fn heights_strategy() -> impl Strategy<Value = Vec<i32>> {
        proptest::collection::vec(1..=3i32, (SIDE * SIDE) as usize)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in heights_strategy(),
            from in 0..(SIDE * SIDE) as usize,
            to in 0..(SIDE * SIDE) as usize,
        ) {
            let grid = terrain(&heights);
            let (start, goal) = (stand_on(&heights, from), stand_on(&heights, to));

            let Some(route) = find(&grid, start, goal) else {
                return Ok(());
            };

            prop_assert_eq!(route.last(), Some(&goal));
            let mut previous = start;
            for step in after_start(&route, start) {
                prop_assert!(standable(&grid, step), "{step:?} cannot be stood on");
                let moved = step - previous;
                prop_assert_eq!(moved.x.abs() + moved.z.abs(), 1, "{:?} to {:?}", previous, step);
                prop_assert!(moved.y.abs() <= 1, "{previous:?} to {step:?} drops or climbs too far");
                previous = step;
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in heights_strategy(),
            from in 0..(SIDE * SIDE) as usize,
            to in 0..(SIDE * SIDE) as usize,
        ) {
            let grid = terrain(&heights);
            let (start, goal) = (stand_on(&heights, from), stand_on(&heights, to));

            let first = find(&grid, start, goal);
            let second = find(&grid, start, goal);

            prop_assert_eq!(first, second);
        }
    }
}
