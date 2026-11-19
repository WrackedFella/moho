//! Walking routes over the voxel ground: what a pawn can stand on and the way between cells.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Most cells a search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// A cell a pawn's feet can occupy: ground below and three cells of air from the feet up.
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    todo!("routing::standable")
}

/// The cell holding `feet`.
pub fn feet_cell(_feet: Vec3) -> BlockPos {
    todo!("routing::feet_cell")
}

/// The cells to walk from `start` to the nearest cell satisfying `is_goal`, or `None` when
/// none is reachable within `budget` expansions. `toward` steers the search.
pub fn route(
    _grid: &VoxelGrid,
    _start: BlockPos,
    _is_goal: impl Fn(BlockPos) -> bool,
    _toward: BlockPos,
    _budget: usize,
) -> Option<Vec<BlockPos>> {
    todo!("routing::route")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const FLOOR_EXTENT: i32 = 10;

    fn flat_world() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in -FLOOR_EXTENT..=FLOOR_EXTENT {
            for z in -FLOOR_EXTENT..=FLOOR_EXTENT {
                grid.mutator().place(BlockPos::new(x, 0, z), 1, None);
            }
        }
        grid
    }

    fn place_column(grid: &mut VoxelGrid, x: i32, z: i32, from_y: i32, to_y: i32) {
        for y in from_y..=to_y {
            grid.mutator().place(BlockPos::new(x, y, z), 1, None);
        }
    }

    fn goal_at(goal: BlockPos) -> impl Fn(BlockPos) -> bool {
        move |cell| cell == goal
    }

    /// The route without a leading copy of the start cell.
    fn steps(route: &[BlockPos], start: BlockPos) -> Vec<BlockPos> {
        route
            .iter()
            .copied()
            .skip_while(|cell| *cell == start)
            .collect()
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = flat_world();
        grid.mutator().place(BlockPos::new(2, 1, 0), 1, None);
        grid.mutator().place(BlockPos::new(4, 2, 0), 1, None);
        grid.mutator().place(BlockPos::new(6, 3, 0), 1, None);
        grid.mutator().place(BlockPos::new(8, 4, 0), 1, None);

        assert!(standable(&grid, BlockPos::new(0, 1, 0)), "open ground");
        assert!(!standable(&grid, BlockPos::new(0, 2, 0)), "no ground below");
        assert!(
            !standable(&grid, BlockPos::new(0, 0, 0)),
            "feet in the ground"
        );
        assert!(
            !standable(&grid, BlockPos::new(2, 1, 0)),
            "feet inside a block"
        );
        assert!(
            !standable(&grid, BlockPos::new(4, 1, 0)),
            "block at the second cell"
        );
        assert!(
            !standable(&grid, BlockPos::new(6, 1, 0)),
            "block at the third cell"
        );
        assert!(
            standable(&grid, BlockPos::new(8, 1, 0)),
            "a block above the capsule does not matter"
        );
    }

    #[test]
    fn feet_cell_floors_the_position() {
        assert_eq!(feet_cell(Vec3::new(0.5, 1.0, 0.5)), BlockPos::new(0, 1, 0));
        assert_eq!(
            feet_cell(Vec3::new(-0.2, 1.9, 3.99)),
            BlockPos::new(-1, 1, 3)
        );
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = flat_world();
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(4, 1, 0);

        let found = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET)
            .expect("open ground has a route");

        let expected: Vec<BlockPos> = (1..=4).map(|x| BlockPos::new(x, 1, 0)).collect();
        assert_eq!(steps(&found, start), expected);
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = flat_world();
        for x in 3..=5 {
            for z in -FLOOR_EXTENT..=FLOOR_EXTENT {
                place_column(&mut grid, x, z, 1, 1);
            }
        }
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(8, 1, 0);

        let found = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET)
            .expect("a one-block rise can be walked over");

        assert_eq!(found.last(), Some(&goal));
        assert!(
            found.iter().any(|cell| cell.y == 2),
            "the route goes over the rise"
        );
        assert!(found.iter().all(|cell| standable(&grid, *cell)));
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = flat_world();
        for z in -FLOOR_EXTENT..=FLOOR_EXTENT {
            place_column(&mut grid, 3, z, 1, 2);
        }
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(6, 1, 0);

        let found = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(found, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut open = flat_world();
        for x in 1..=8 {
            for z in -FLOOR_EXTENT..=FLOOR_EXTENT {
                place_column(&mut open, x, z, 1, 1);
            }
        }
        let mut low_ceiling = flat_world();
        for x in 1..=8 {
            for z in -FLOOR_EXTENT..=FLOOR_EXTENT {
                place_column(&mut low_ceiling, x, z, 1, 1);
            }
        }
        for z in -FLOOR_EXTENT..=FLOOR_EXTENT {
            place_column(&mut low_ceiling, 0, z, 4, 4);
        }
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(2, 2, 0);

        let without_ceiling = route(&open, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);
        let with_ceiling = route(&low_ceiling, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);

        assert!(
            without_ceiling.is_some(),
            "the step up is walkable with headroom"
        );
        assert_eq!(with_ceiling, None, "no fourth air cell, no step up");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = flat_world();
        for (x, z) in [(3, 0), (5, 0), (4, 1), (4, -1)] {
            place_column(&mut grid, x, z, 1, 3);
        }
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(4, 1, 0);

        let found = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = flat_world();
        let start = BlockPos::new(-FLOOR_EXTENT, 1, -FLOOR_EXTENT);
        let goal = BlockPos::new(FLOOR_EXTENT, 1, FLOOR_EXTENT);

        let starved = route(&grid, start, goal_at(goal), goal, 5);
        let funded = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None);
        assert!(funded.is_some());
    }

    const SIDE: i32 = 6;

    fn heights_strategy() -> impl Strategy<Value = Vec<i32>> {
        proptest::collection::vec(1..=3i32, (SIDE * SIDE) as usize)
    }

    fn hilly_world(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..SIDE {
            for z in 0..SIDE {
                let height = heights[(x * SIDE + z) as usize];
                place_column(&mut grid, x, z, 0, height - 1);
            }
        }
        grid
    }

    fn top_cell(heights: &[i32], x: i32, z: i32) -> BlockPos {
        BlockPos::new(x, heights[(x * SIDE + z) as usize], z)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in heights_strategy(),
            sx in 0..SIDE, sz in 0..SIDE, gx in 0..SIDE, gz in 0..SIDE,
        ) {
            let grid = hilly_world(&heights);
            let start = top_cell(&heights, sx, sz);
            let goal = top_cell(&heights, gx, gz);

            if let Some(found) = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET) {
                prop_assert_eq!(found.last().copied(), Some(goal));
                let mut previous = start;
                for cell in steps(&found, start) {
                    prop_assert!(standable(&grid, cell), "{cell} is not standable");
                    let step = cell - previous;
                    prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "one horizontal move");
                    prop_assert!(step.y.abs() <= 1, "at most one block up or down");
                    previous = cell;
                }
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in heights_strategy(),
            sx in 0..SIDE, sz in 0..SIDE, gx in 0..SIDE, gz in 0..SIDE,
        ) {
            let grid = hilly_world(&heights);
            let start = top_cell(&heights, sx, sz);
            let goal = top_cell(&heights, gx, gz);

            let first = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);
            let second = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);

            prop_assert_eq!(first, second);
        }
    }
}
