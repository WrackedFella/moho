//! Walking routes over the voxel ground: what a pawn can stand on and the cells it passes
//! to reach a goal.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Node expansions a single [`route`] search may spend before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: solid ground below and three cells of
/// air from the feet up (the capsule is 2.3 m tall).
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    todo!("standable({grid:p}, {cell})")
}

/// The cell holding a pawn's feet at `feet`.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    todo!("feet_cell({feet})")
}

/// A* over standable cells from `start` to the first cell `is_goal` accepts, with the
/// Manhattan distance to `toward` as the heuristic. The returned cells exclude `start` and
/// end at the goal; a `start` that is already a goal gives an empty route. `None` when no
/// goal is reachable or `budget` expansions run out first.
pub fn route(
    grid: &VoxelGrid,
    start: BlockPos,
    is_goal: impl Fn(BlockPos) -> bool,
    toward: BlockPos,
    budget: usize,
) -> Option<Vec<BlockPos>> {
    let _ = (&is_goal, budget);
    todo!("route({grid:p}, {start}, {toward})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const STONE: u32 = 1;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// A floor of blocks at `y = 0` over `0..width` in x and `0..depth` in z, so cells at
    /// `y = 1` are standable.
    fn floor(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                grid.mutator().place(cell(x, 0, z), STONE, None);
            }
        }
        grid
    }

    /// Fills a full-depth wall of blocks at column `x` over the given heights.
    fn wall(grid: &mut VoxelGrid, x: i32, depth: i32, ys: std::ops::RangeInclusive<i32>) {
        for y in ys {
            for z in 0..depth {
                grid.mutator().place(cell(x, y, z), STONE, None);
            }
        }
    }

    fn goal_at(goal: BlockPos) -> impl Fn(BlockPos) -> bool {
        move |c| c == goal
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = floor(10, 10);
        grid.mutator().place(cell(2, 4, 2), STONE, None);
        grid.mutator().place(cell(3, 3, 3), STONE, None);

        assert!(standable(&grid, cell(5, 1, 5)), "ground below, open air");
        assert!(!standable(&grid, cell(5, 2, 5)), "air below is not ground");
        assert!(!standable(&grid, cell(5, 0, 5)), "inside the ground");
        assert!(
            standable(&grid, cell(2, 1, 2)),
            "a block at the fourth cell is above the capsule"
        );
        assert!(
            !standable(&grid, cell(3, 1, 3)),
            "a block at the third air cell hits the head"
        );
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        assert_eq!(feet_cell(Vec3::new(1.7, 2.0, -0.2)), cell(1, 2, -1));
        assert_eq!(feet_cell(Vec3::new(-0.5, 1.99, 3.0)), cell(-1, 1, 3));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = floor(10, 3);

        let path = route(
            &grid,
            cell(0, 1, 1),
            goal_at(cell(5, 1, 1)),
            cell(5, 1, 1),
            4096,
        );

        let expected: Vec<BlockPos> = (1..=5).map(|x| cell(x, 1, 1)).collect();
        assert_eq!(path, Some(expected));
    }

    #[test]
    fn a_start_that_is_already_a_goal_gives_an_empty_route() {
        let grid = floor(4, 3);

        let path = route(
            &grid,
            cell(1, 1, 1),
            goal_at(cell(1, 1, 1)),
            cell(1, 1, 1),
            4096,
        );

        assert_eq!(path, Some(Vec::new()));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = floor(10, 3);
        for x in 4..=5 {
            wall(&mut grid, x, 3, 1..=1);
        }

        let path = route(
            &grid,
            cell(0, 1, 1),
            goal_at(cell(9, 1, 1)),
            cell(9, 1, 1),
            4096,
        )
        .expect("a one-block rise can be walked over");

        assert_eq!(path.last(), Some(&cell(9, 1, 1)));
        assert!(path.contains(&cell(4, 2, 1)), "climbs onto the rise");
        assert!(path.contains(&cell(5, 2, 1)), "crosses the top");
        assert!(path.contains(&cell(6, 1, 1)), "descends the far side");
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = floor(10, 3);
        wall(&mut grid, 4, 3, 1..=2);

        let path = route(
            &grid,
            cell(0, 1, 1),
            goal_at(cell(9, 1, 1)),
            cell(9, 1, 1),
            4096,
        );

        assert_eq!(path, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = floor(10, 3);
        wall(&mut grid, 4, 3, 1..=1);
        wall(&mut grid, 3, 3, 4..=4);

        let path = route(
            &grid,
            cell(0, 1, 1),
            goal_at(cell(9, 1, 1)),
            cell(9, 1, 1),
            4096,
        );

        assert_eq!(
            path, None,
            "no fourth air cell above the cell before the rise"
        );
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = floor(10, 10);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            for y in 1..=2 {
                grid.mutator().place(cell(5 + dx, y, 5 + dz), STONE, None);
            }
        }

        let path = route(
            &grid,
            cell(0, 1, 0),
            goal_at(cell(5, 1, 5)),
            cell(5, 1, 5),
            4096,
        );

        assert_eq!(path, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = floor(30, 3);
        let goal = cell(29, 1, 1);

        let starved = route(&grid, cell(0, 1, 1), goal_at(goal), goal, 5);
        let funded = route(&grid, cell(0, 1, 1), goal_at(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None);
        assert_eq!(funded.map(|p| p.len()), Some(29));
        assert_eq!(ROUTE_NODE_BUDGET, 4096);
    }

    #[test]
    fn a_start_on_a_slope_tries_the_cells_above_and_below() {
        let grid = floor(10, 3);

        let from_above = route(
            &grid,
            cell(0, 2, 1),
            goal_at(cell(3, 1, 1)),
            cell(3, 1, 1),
            4096,
        );
        let from_below = route(
            &grid,
            cell(0, 0, 1),
            goal_at(cell(3, 1, 1)),
            cell(3, 1, 1),
            4096,
        );

        assert_eq!(
            from_above.map(|p| p.last().copied()),
            Some(Some(cell(3, 1, 1)))
        );
        assert_eq!(
            from_below.map(|p| p.last().copied()),
            Some(Some(cell(3, 1, 1)))
        );
    }

    /// Column heights over an 8x8 patch: solid from `y = 0` up to the height, so a pawn's
    /// feet cell is at `y = height`.
    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (i, &h) in heights.iter().enumerate() {
            let (x, z) = ((i % 8) as i32, (i / 8) as i32);
            for y in 0..h {
                grid.mutator().place(cell(x, y, z), STONE, None);
            }
        }
        grid
    }

    fn column_cell(heights: &[i32], x: i32, z: i32) -> BlockPos {
        cell(x, heights[(z * 8 + x) as usize], z)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1..=3i32, 64),
            (sx, sz, gx, gz) in (0..8i32, 0..8i32, 0..8i32, 0..8i32),
        ) {
            let grid = terrain(&heights);
            let start = column_cell(&heights, sx, sz);
            let goal = column_cell(&heights, gx, gz);

            let Some(path) = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET) else {
                return Ok(());
            };

            prop_assert_eq!(path.last().copied().unwrap_or(start), goal);
            let mut previous = start;
            for step in path {
                prop_assert!(standable(&grid, step), "{step} is not standable");
                let delta = step - previous;
                prop_assert_eq!(delta.x.abs() + delta.z.abs(), 1, "{} to {}", previous, step);
                prop_assert!(delta.y.abs() <= 1, "{} to {}", previous, step);
                previous = step;
            }
        }

        #[test]
        fn a_route_across_open_flat_ground_has_the_manhattan_length(
            (sx, sz, gx, gz) in (0..8i32, 0..8i32, 0..8i32, 0..8i32),
        ) {
            let grid = floor(8, 8);
            let (start, goal) = (cell(sx, 1, sz), cell(gx, 1, gz));

            let path = route(&grid, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);

            let length = ((gx - sx).abs() + (gz - sz).abs()) as usize;
            prop_assert_eq!(path.map(|p| p.len()), Some(length));
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1..=2i32, 64),
            (sx, sz, gx, gz) in (0..8i32, 0..8i32, 0..8i32, 0..8i32),
        ) {
            let forward = terrain(&heights);
            let mut backwards = VoxelGrid::new(16);
            for (i, &h) in heights.iter().enumerate().rev() {
                let (x, z) = ((i % 8) as i32, (i / 8) as i32);
                for y in (0..h).rev() {
                    backwards.mutator().place(cell(x, y, z), STONE, None);
                }
            }
            let start = column_cell(&heights, sx, sz);
            let goal = column_cell(&heights, gx, gz);

            let first = route(&forward, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);
            let again = route(&forward, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);
            let other = route(&backwards, start, goal_at(goal), goal, ROUTE_NODE_BUDGET);

            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &other);
        }
    }
}
