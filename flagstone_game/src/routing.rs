//! Walking routes over the voxel grid: where a pawn can stand and how it gets between cells.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Search expansions before [`route`] gives up with `None`.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: solid ground below and three cells of
/// air from the feet up (the capsule is 2.3 m tall).
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    false
}

/// The block cell containing a feet position.
pub fn feet_cell(_feet: Vec3) -> BlockPos {
    BlockPos::ZERO
}

/// A* over standable cells from `start` to the nearest cell satisfying `is_goal`.
///
/// Moves are the four horizontal neighbours, stepping up or down one block (stepping up needs
/// a fourth air cell above the current feet). `toward` steers the Manhattan heuristic. The
/// route lists every cell walked, `start` first and the goal cell last. A `start` that is not
/// standable tries the cell above, then the cell below, before giving up. Gives up with `None`
/// when no route exists or `budget` expansions are spent.
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

    fn floor(
        grid: &mut VoxelGrid,
        xs: std::ops::RangeInclusive<i32>,
        zs: std::ops::RangeInclusive<i32>,
    ) {
        for x in xs {
            for z in zs.clone() {
                grid.mutator().place(BlockPos::new(x, 0, z), 1, None);
            }
        }
    }

    fn flat(xs: std::ops::RangeInclusive<i32>, zs: std::ops::RangeInclusive<i32>) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        floor(&mut grid, xs, zs);
        grid
    }

    fn solid(grid: &mut VoxelGrid, pos: BlockPos) {
        grid.mutator().place(pos, 1, None);
    }

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn go(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        assert_eq!(feet_cell(Vec3::new(2.7, 1.0, 3.99)), cell(2, 1, 3));
        assert_eq!(feet_cell(Vec3::new(-0.2, 0.0, -3.0)), cell(-1, 0, -3));
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let open = flat(0..=4, 0..=4);
        assert!(standable(&open, cell(2, 1, 2)), "ground and open air");
        assert!(!standable(&open, cell(2, 2, 2)), "air below the feet");
        assert!(!standable(&open, cell(2, 0, 2)), "feet inside the ground");

        for ceiling_y in [1, 2, 3] {
            let mut low = flat(0..=4, 0..=4);
            solid(&mut low, cell(2, ceiling_y, 2));
            assert!(
                !standable(&low, cell(2, 1, 2)),
                "solid at y={ceiling_y} blocks the capsule"
            );
        }
        let mut high = flat(0..=4, 0..=4);
        solid(&mut high, cell(2, 4, 2));
        assert!(
            standable(&high, cell(2, 1, 2)),
            "the fourth cell is not needed to stand"
        );
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = flat(-2..=8, -2..=2);

        let found = go(&grid, cell(0, 1, 0), cell(4, 1, 0));

        assert_eq!(
            found,
            Some(vec![
                cell(0, 1, 0),
                cell(1, 1, 0),
                cell(2, 1, 0),
                cell(3, 1, 0),
                cell(4, 1, 0)
            ])
        );
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = flat(-2..=10, -2..=2);
        for x in 3..=5 {
            for z in -2..=2 {
                solid(&mut grid, cell(x, 1, z));
            }
        }

        let found = go(&grid, cell(0, 1, 0), cell(8, 1, 0));

        let heights: Vec<(i32, i32)> = found
            .expect("a one-block rise can be walked")
            .iter()
            .map(|c| (c.x, c.y))
            .collect();
        assert_eq!(
            heights,
            [
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 2),
                (4, 2),
                (5, 2),
                (6, 1),
                (7, 1),
                (8, 1)
            ]
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let walled = |height: i32| {
            let mut grid = flat(-2..=8, -2..=2);
            for y in 1..=height {
                for z in -2..=2 {
                    solid(&mut grid, cell(3, y, z));
                }
            }
            grid
        };

        let over_one = go(&walled(1), cell(0, 1, 0), cell(6, 1, 0));
        let over_two = go(&walled(2), cell(0, 1, 0), cell(6, 1, 0));

        assert!(over_one.is_some(), "one block is a step");
        assert_eq!(over_two, None, "two blocks is a wall");
    }

    #[test]
    fn climbing_needs_headroom() {
        let with_ceiling = |ceiling: bool| {
            let mut grid = flat(-2..=8, -2..=2);
            for z in -2..=2 {
                solid(&mut grid, cell(3, 1, z));
                if ceiling {
                    solid(&mut grid, cell(2, 4, z));
                }
            }
            grid
        };

        let open = go(&with_ceiling(false), cell(0, 1, 0), cell(6, 1, 0));
        let low = go(&with_ceiling(true), cell(0, 1, 0), cell(6, 1, 0));

        assert!(open.is_some(), "the rise is climbable with headroom");
        assert_eq!(low, None, "no fourth air cell above the feet, no climb");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = flat(-2..=8, -2..=2);
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            for y in 1..=2 {
                solid(&mut grid, cell(5 + dx, y, dz));
            }
        }

        let found = go(&grid, cell(0, 1, 0), cell(5, 1, 0));

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = flat(0..=30, -2..=2);
        let goal = cell(30, 1, 0);

        let starved = route(&grid, cell(0, 1, 0), |c| c == goal, goal, 5);
        let funded = route(&grid, cell(0, 1, 0), |c| c == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None);
        assert_eq!(funded.map(|r| r.len()), Some(31));
    }

    #[test]
    fn a_start_inside_the_ground_tries_the_cell_above_and_nothing_further() {
        let mut grid = flat(-2..=8, -2..=2);
        solid(&mut grid, cell(0, 1, 0));

        let from_slope = go(&grid, cell(0, 1, 0), cell(3, 1, 0));
        let from_the_air = go(&grid, cell(0, 6, 0), cell(3, 1, 0));
        let open = flat(-2..=8, -2..=2);
        let from_above = go(&open, cell(0, 2, 0), cell(3, 1, 0));
        assert_eq!(
            from_above.and_then(|r| r.first().copied()),
            Some(cell(0, 1, 0)),
            "one cell above standable ground tries the cell below"
        );

        let from_slope = from_slope.expect("the pawn is one cell low on a slope");
        assert_eq!(from_slope.first(), Some(&cell(0, 2, 0)));
        assert_eq!(from_slope.last(), Some(&cell(3, 1, 0)));
        assert_eq!(from_the_air, None);
    }

    /// Blocks one high on a flat floor: every neighbouring column differs by at most one, so a
    /// route always exists.
    fn bumpy() -> impl Strategy<Value = Vec<(i32, i32)>> {
        proptest::collection::vec((0..8i32, 0..8i32), 0..20)
            .prop_map(|v| v.into_iter().filter(|&p| p != (0, 0)).collect())
    }

    fn bumpy_grid(bumps: &[(i32, i32)]) -> VoxelGrid {
        let mut grid = flat(0..=7, 0..=7);
        for &(x, z) in bumps {
            solid(&mut grid, cell(x, 1, z));
        }
        grid
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(bumps in bumpy()) {
            let grid = bumpy_grid(&bumps);
            let start = cell(0, 1, 0);

            let found = route(&grid, start, |c| c.x == 7 && c.z == 7, cell(7, 1, 7), ROUTE_NODE_BUDGET);

            let found = found.expect("one-block bumps never wall off a floor");
            prop_assert_eq!(found[0], start);
            prop_assert!(found.iter().all(|&c| standable(&grid, c)));
            let adjacent = found.windows(2).all(|w| {
                let d = w[1] - w[0];
                d.x.abs() + d.z.abs() == 1 && d.y.abs() <= 1
            });
            prop_assert!(adjacent);
            let last = *found.last().expect("a route has cells");
            prop_assert!(last.x == 7 && last.z == 7);
        }

        #[test]
        fn the_same_grid_gives_the_same_route(bumps in bumpy()) {
            let goal = cell(7, 1, 7);

            let first = route(&bumpy_grid(&bumps), cell(0, 1, 0), |c| c.x == 7 && c.z == 7, goal, ROUTE_NODE_BUDGET);
            let second = route(&bumpy_grid(&bumps), cell(0, 1, 0), |c| c.x == 7 && c.z == 7, goal, ROUTE_NODE_BUDGET);

            prop_assert!(first.is_some());
            prop_assert_eq!(first, second);
        }
    }
}
