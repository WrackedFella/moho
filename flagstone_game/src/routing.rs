//! Walking routes over the voxel grid: which cells a pawn can stand in and how it gets between them.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// The most cells one search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn's feet can be in `cell`: ground below and air for the capsule's height.
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    false
}

/// The cell holding a pawn's feet at `feet`.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// The cheapest walkable route from `start` to the first cell `is_goal` accepts, both ends
/// included, or `None` when there is none within `budget` expansions.
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

    fn floor(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                grid.mutator().place(BlockPos::new(x, 0, z), 1, None);
            }
        }
        grid
    }

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(BlockPos::new(x, y, z), 1, None);
    }

    /// A wall of blocks `height` high across every row at column `x`.
    fn wall(grid: &mut VoxelGrid, x: i32, height: i32, depth: i32) {
        for y in 1..=height {
            for z in 0..depth {
                place(grid, x, y, z);
            }
        }
    }

    fn find(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |cell| cell == goal, goal, ROUTE_NODE_BUDGET)
    }

    fn cells(list: &[(i32, i32, i32)]) -> Vec<BlockPos> {
        list.iter()
            .map(|&(x, y, z)| BlockPos::new(x, y, z))
            .collect()
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let open = floor(5, 5);
        let cell = BlockPos::new(2, 1, 2);
        let mut at_feet = floor(5, 5);
        place(&mut at_feet, 2, 1, 2);
        let mut at_chest = floor(5, 5);
        place(&mut at_chest, 2, 2, 2);
        let mut at_head = floor(5, 5);
        place(&mut at_head, 2, 3, 2);
        let mut above_head = floor(5, 5);
        place(&mut above_head, 2, 4, 2);
        let mut no_ground = floor(5, 5);
        no_ground.mutator().remove(BlockPos::new(2, 0, 2));

        assert!(standable(&open, cell), "open air over ground");
        assert!(!standable(&at_feet, cell), "a block at the feet");
        assert!(!standable(&at_chest, cell), "a block one above the feet");
        assert!(!standable(&at_head, cell), "a block two above the feet");
        assert!(
            standable(&above_head, cell),
            "three cells of air are enough"
        );
        assert!(!standable(&no_ground, cell), "nothing to stand on");
        assert!(!standable(&open, BlockPos::new(2, 2, 2)), "floating");
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = floor(8, 3);
        let start = BlockPos::new(0, 1, 1);
        let goal = BlockPos::new(3, 1, 1);

        let found = find(&grid, start, goal);

        assert_eq!(
            found,
            Some(cells(&[(0, 1, 1), (1, 1, 1), (2, 1, 1), (3, 1, 1)]))
        );
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = floor(7, 3);
        wall(&mut grid, 3, 1, 3);

        let found = find(&grid, BlockPos::new(0, 1, 1), BlockPos::new(6, 1, 1));

        assert_eq!(
            found,
            Some(cells(&[
                (0, 1, 1),
                (1, 1, 1),
                (2, 1, 1),
                (3, 2, 1),
                (4, 1, 1),
                (5, 1, 1),
                (6, 1, 1)
            ]))
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = floor(7, 3);
        wall(&mut grid, 3, 2, 3);

        let found = find(&grid, BlockPos::new(0, 1, 1), BlockPos::new(6, 1, 1));

        assert_eq!(found, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = floor(7, 3);
        wall(&mut grid, 3, 1, 3);
        let start = BlockPos::new(0, 1, 1);
        let goal = BlockPos::new(6, 1, 1);
        assert!(find(&grid, start, goal).is_some(), "open sky lets it climb");
        for z in 0..3 {
            place(&mut grid, 2, 4, z);
        }

        let found = find(&grid, start, goal);

        assert_eq!(
            found, None,
            "a block three above the feet stops the step up"
        );
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = floor(11, 11);
        for (x, z) in [(4, 5), (6, 5), (5, 4), (5, 6)] {
            place(&mut grid, x, 1, z);
            place(&mut grid, x, 2, z);
        }

        let found = find(&grid, BlockPos::new(0, 1, 0), BlockPos::new(5, 1, 5));

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = floor(30, 1);
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(29, 1, 0);

        let starved = route(&grid, start, |cell| cell == goal, goal, 5);
        let funded = route(&grid, start, |cell| cell == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None);
        assert_eq!(funded.map(|path| path.len()), Some(30));
    }

    #[test]
    fn a_start_inside_the_ground_steps_up_to_the_surface() {
        let grid = floor(6, 3);
        let sunk = BlockPos::new(1, 0, 1);

        let found = find(&grid, sunk, BlockPos::new(4, 1, 1));

        let path = found.expect("a start one below the surface is tried one above");
        assert_eq!(path.last(), Some(&BlockPos::new(4, 1, 1)));
        assert!(path.iter().all(|&cell| standable(&grid, cell)));
    }

    #[test]
    fn a_start_floating_above_the_ground_drops_to_the_surface() {
        let grid = floor(6, 3);
        let floating = BlockPos::new(1, 2, 1);

        let found = find(&grid, floating, BlockPos::new(4, 1, 1));

        let path = found.expect("a start one above the surface is tried one below");
        assert_eq!(path.last(), Some(&BlockPos::new(4, 1, 1)));
        assert!(path.iter().all(|&cell| standable(&grid, cell)));
    }

    /// A 6 by 6 patch of columns two or three blocks deep (surface cells at y 1 or 2), so
    /// every neighbour differs by at most one and every pair of surface cells is connected.
    fn rolling(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..6 {
            for z in 0..6 {
                let top = heights[usize::try_from(x * 6 + z).expect("small index")];
                for y in 0..top {
                    place(&mut grid, x, y, z);
                }
            }
        }
        grid
    }

    fn surface(heights: &[i32], x: i32, z: i32) -> BlockPos {
        BlockPos::new(
            x,
            heights[usize::try_from(x * 6 + z).expect("small index")],
            z,
        )
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1i32..=2, 36),
            sx in 0..6, sz in 0..6, gx in 0..6, gz in 0..6,
        ) {
            let grid = rolling(&heights);
            let start = surface(&heights, sx, sz);
            let goal = surface(&heights, gx, gz);

            let path = find(&grid, start, goal).expect("one-block steps connect every cell");

            prop_assert_eq!(path.first(), Some(&start));
            prop_assert_eq!(path.last(), Some(&goal));
            for &cell in &path {
                prop_assert!(standable(&grid, cell), "{cell:?} is not standable");
            }
            for pair in path.windows(2) {
                let step = pair[1] - pair[0];
                prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "one horizontal neighbour");
                prop_assert!(step.y.abs() <= 1, "at most one block up or down");
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1i32..=2, 36),
            sx in 0..6, sz in 0..6, gx in 0..6, gz in 0..6,
        ) {
            let grid = rolling(&heights);
            let start = surface(&heights, sx, sz);
            let goal = surface(&heights, gx, gz);

            let first = find(&grid, start, goal);
            let second = find(&grid, start, goal);

            prop_assert!(first.is_some());
            prop_assert_eq!(first, second);
        }
    }
}
