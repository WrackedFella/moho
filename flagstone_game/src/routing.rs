//! Routing over the voxel ground: which cells a pawn can stand on and how it walks between them.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// The most cells one search expands before it gives up with no route.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: solid ground below and three cells of
/// air from the feet up, the height of its capsule.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    todo!("standable {cell:?} in a grid of {}", grid.chunk_size())
}

/// The cell holding `feet`.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    todo!("feet cell of {feet:?}")
}

/// The cells to walk from `start` to the nearest cell satisfying `is_goal`, `start` first and
/// the goal last, or `None` when no goal is reachable within `budget` expansions.
pub fn route(
    grid: &VoxelGrid,
    start: BlockPos,
    is_goal: impl Fn(BlockPos) -> bool,
    toward: BlockPos,
    budget: usize,
) -> Option<Vec<BlockPos>> {
    let _ = is_goal(start);
    todo!(
        "route from {start:?} toward {toward:?} in {budget} expansions over a grid of {}",
        grid.chunk_size()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn put(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(cell(x, y, z), 1, None);
    }

    /// One layer of ground at y 0 over `0..width` by `0..depth`, its top at y 1.
    fn ground(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                put(&mut grid, x, 0, z);
            }
        }
        grid
    }

    /// Stacks blocks from y 1 to `tops` over every `z` in `0..depth` of the columns `xs`.
    fn raise(grid: &mut VoxelGrid, xs: std::ops::RangeInclusive<i32>, tops: i32, depth: i32) {
        for x in xs {
            for z in 0..depth {
                for y in 1..=tops {
                    put(grid, x, y, z);
                }
            }
        }
    }

    fn walk(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = ground(12, 12);
        put(&mut grid, 5, 3, 5);
        put(&mut grid, 6, 4, 6);
        put(&mut grid, 7, 2, 7);
        put(&mut grid, 8, 1, 8);

        let open = standable(&grid, cell(2, 1, 2));
        let inside_the_ground = standable(&grid, cell(2, 0, 2));
        let over_air = standable(&grid, cell(2, 2, 2));
        let off_the_ground = standable(&grid, cell(20, 1, 20));
        let two_cells_of_air = standable(&grid, cell(5, 1, 5));
        let three_cells_of_air = standable(&grid, cell(6, 1, 6));
        let one_cell_of_air = standable(&grid, cell(7, 1, 7));
        let in_a_block = standable(&grid, cell(8, 1, 8));
        let on_a_block = standable(&grid, cell(8, 2, 8));

        assert!(open, "open ground");
        assert!(!inside_the_ground, "feet inside solid");
        assert!(!over_air, "nothing below");
        assert!(!off_the_ground, "no ground there at all");
        assert!(
            !two_cells_of_air,
            "a block three cells up leaves two cells of air"
        );
        assert!(three_cells_of_air, "a block four cells up leaves three");
        assert!(!one_cell_of_air, "a block two cells up leaves one");
        assert!(!in_a_block, "feet inside a block");
        assert!(on_a_block, "the top of a block is ground");
    }

    #[test]
    fn feet_cell_floors_the_position() {
        assert_eq!(feet_cell(Vec3::new(2.5, 1.0, 3.9)), cell(2, 1, 3));
        assert_eq!(feet_cell(Vec3::new(2.99, 1.99, 0.0)), cell(2, 1, 0));
        assert_eq!(feet_cell(Vec3::new(-0.5, 0.99, -0.01)), cell(-1, 0, -1));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = ground(10, 3);

        let route = walk(&grid, cell(0, 1, 1), cell(5, 1, 1));

        assert_eq!(
            route,
            Some(vec![
                cell(0, 1, 1),
                cell(1, 1, 1),
                cell(2, 1, 1),
                cell(3, 1, 1),
                cell(4, 1, 1),
                cell(5, 1, 1),
            ])
        );
    }

    #[test]
    fn a_route_from_a_goal_cell_is_that_cell_alone() {
        let grid = ground(4, 4);

        let route = route(
            &grid,
            cell(2, 1, 2),
            |c| c == cell(2, 1, 2),
            cell(2, 1, 2),
            8,
        );

        assert_eq!(route, Some(vec![cell(2, 1, 2)]));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = ground(12, 3);
        raise(&mut grid, 3..=5, 1, 3);

        let route = walk(&grid, cell(0, 1, 1), cell(8, 1, 1));

        assert_eq!(
            route,
            Some(vec![
                cell(0, 1, 1),
                cell(1, 1, 1),
                cell(2, 1, 1),
                cell(3, 2, 1),
                cell(4, 2, 1),
                cell(5, 2, 1),
                cell(6, 1, 1),
                cell(7, 1, 1),
                cell(8, 1, 1),
            ])
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut walled = ground(12, 3);
        raise(&mut walled, 4..=4, 2, 3);
        let mut low = ground(12, 3);
        raise(&mut low, 4..=4, 1, 3);

        let blocked = walk(&walled, cell(0, 1, 1), cell(8, 1, 1));
        let climbed = walk(&low, cell(0, 1, 1), cell(8, 1, 1));

        assert_eq!(blocked, None, "two blocks are too high to step");
        assert_eq!(
            climbed.expect("one block is a step").len(),
            9,
            "the same way over a one-block wall"
        );
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut roofed = ground(12, 3);
        raise(&mut roofed, 3..=8, 1, 3);
        for z in 0..3 {
            put(&mut roofed, 2, 4, z);
        }
        let mut open = ground(12, 3);
        raise(&mut open, 3..=8, 1, 3);

        let blocked = walk(&roofed, cell(0, 1, 1), cell(6, 2, 1));
        let climbed = walk(&open, cell(0, 1, 1), cell(6, 2, 1));

        assert!(
            standable(&roofed, cell(2, 1, 1)),
            "setup: three cells of air stand a pawn"
        );
        assert_eq!(blocked, None, "no fourth cell of air to climb into");
        assert_eq!(
            climbed.expect("without the roof it climbs").last(),
            Some(&cell(6, 2, 1))
        );
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut sealed = ground(11, 11);
        let mut open = ground(11, 11);
        for (x, z) in [(4, 5), (6, 5), (5, 4), (5, 6)] {
            for y in 1..=2 {
                put(&mut sealed, x, y, z);
                if (x, z) != (4, 5) {
                    put(&mut open, x, y, z);
                }
            }
        }

        let none = walk(&sealed, cell(0, 1, 0), cell(5, 1, 5));
        let found = walk(&open, cell(0, 1, 0), cell(5, 1, 5));

        assert_eq!(none, None, "walled in on four sides");
        assert_eq!(
            found.expect("one side open").last(),
            Some(&cell(5, 1, 5)),
            "the same goal with a gap"
        );
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = ground(20, 3);
        let goal = cell(15, 1, 1);

        let starved = route(&grid, cell(0, 1, 1), |c| c == goal, goal, 5);
        let fed = route(&grid, cell(0, 1, 1), |c| c == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None, "five expansions do not reach 15 cells away");
        assert_eq!(fed.expect("the default budget reaches it").len(), 16);
    }

    #[test]
    fn a_start_one_cell_off_the_ground_is_corrected() {
        let grid = ground(10, 3);
        let goal = cell(6, 1, 1);

        let from_above = walk(&grid, cell(2, 2, 1), goal).expect("one above finds the ground");
        let from_inside = walk(&grid, cell(2, 0, 1), goal).expect("one below finds the ground");
        let from_far = walk(&grid, cell(2, 5, 1), goal);

        assert_eq!(from_above.first(), Some(&cell(2, 1, 1)));
        assert_eq!(from_above.last(), Some(&goal));
        assert_eq!(from_inside.first(), Some(&cell(2, 1, 1)));
        assert_eq!(from_far, None, "five cells up is not a slope");
    }

    fn terrain(heights: &[Vec<i32>]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (z, row) in heights.iter().enumerate() {
            for (x, &height) in row.iter().enumerate() {
                for y in 0..height {
                    put(&mut grid, x as i32, y, z as i32);
                }
            }
        }
        grid
    }

    fn heights() -> impl Strategy<Value = Vec<Vec<i32>>> {
        proptest::collection::vec(proptest::collection::vec(1..=3i32, 6), 6)
    }

    fn top_cell(heights: &[Vec<i32>], x: usize, z: usize) -> BlockPos {
        cell(x as i32, heights[z][x], z as i32)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in heights(),
            sx in 0usize..6,
            sz in 0usize..6,
            gx in 0usize..6,
            gz in 0usize..6,
        ) {
            let grid = terrain(&heights);
            let start = top_cell(&heights, sx, sz);
            let goal = top_cell(&heights, gx, gz);

            let found = walk(&grid, start, goal);

            if let Some(path) = found {
                prop_assert_eq!(path.first(), Some(&start));
                prop_assert_eq!(path.last(), Some(&goal));
                for c in &path {
                    prop_assert!(standable(&grid, *c), "{c:?} is not standable");
                }
                for pair in path.windows(2) {
                    let step = pair[1] - pair[0];
                    prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "{:?} to {:?}", pair[0], pair[1]);
                    prop_assert!(step.y.abs() <= 1, "{:?} to {:?}", pair[0], pair[1]);
                }
                let flat = (goal.x - start.x).abs() + (goal.z - start.z).abs();
                prop_assert!(path.len() as i32 > flat, "shorter than the walk across");
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in heights(),
            sx in 0usize..6,
            sz in 0usize..6,
            gx in 0usize..6,
            gz in 0usize..6,
        ) {
            let start = top_cell(&heights, sx, sz);
            let goal = top_cell(&heights, gx, gz);

            let first = walk(&terrain(&heights), start, goal);
            let second = walk(&terrain(&heights), start, goal);

            prop_assert_eq!(first, second);
        }
    }
}
