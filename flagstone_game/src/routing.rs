//! Ground routing for pawns over the voxel grid.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Search expansions `route` spends before it gives up with `None`.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground below (`cell - Y` solid) and
/// three cells of air from the feet up.
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    todo!("standable")
}

/// The cell holding a feet position (each axis floored).
pub fn feet_cell(_feet: Vec3) -> BlockPos {
    todo!("feet_cell")
}

/// A* over standable cells from `start` to the nearest cell satisfying `is_goal`.
///
/// The route includes the first standable start cell and the goal. `toward` steers the
/// Manhattan heuristic. `None` when no goal is reachable within `budget` expansions.
pub fn route(
    _grid: &VoxelGrid,
    _start: BlockPos,
    _is_goal: impl Fn(BlockPos) -> bool,
    _toward: BlockPos,
    _budget: usize,
) -> Option<Vec<BlockPos>> {
    todo!("route")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(cell(x, y, z), 1, None);
    }

    /// A floor of solid blocks at y = 0, so feet cells sit at y = 1.
    fn floor(xs: std::ops::RangeInclusive<i32>, zs: std::ops::RangeInclusive<i32>) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in xs {
            for z in zs.clone() {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    fn reach(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    fn adjacent(a: BlockPos, b: BlockPos) -> bool {
        let horizontal = (a.x - b.x).abs() + (a.z - b.z).abs();
        horizontal == 1 && (a.y - b.y).abs() <= 1
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        assert_eq!(feet_cell(Vec3::new(2.9, 1.0, 0.1)), cell(2, 1, 0));
        assert_eq!(feet_cell(Vec3::new(-0.1, 3.5, -2.5)), cell(-1, 3, -3));
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let grid = floor(0..=2, 0..=2);
        let standing = cell(1, 1, 1);

        assert!(standable(&grid, standing));
        assert!(!standable(&grid, cell(1, 2, 1)), "no ground below");
        assert!(!standable(&grid, cell(1, 0, 1)), "inside the floor");
        for blocked in 0..3 {
            let mut grid = floor(0..=2, 0..=2);
            place(&mut grid, 1, 1 + blocked, 1);

            assert!(
                !standable(&grid, standing),
                "solid {blocked} cells above the ground blocks standing"
            );
        }
        let mut high_ceiling = floor(0..=2, 0..=2);
        place(&mut high_ceiling, 1, 4, 1);
        assert!(
            standable(&high_ceiling, standing),
            "the fourth cell is only needed to climb"
        );
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = floor(-1..=8, -2..=2);

        let found = reach(&grid, cell(0, 1, 0), cell(5, 1, 0));

        let expected: Vec<BlockPos> = (0..=5).map(|x| cell(x, 1, 0)).collect();
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn a_route_to_the_start_cell_is_just_that_cell() {
        let grid = floor(-1..=2, -1..=1);

        let found = reach(&grid, cell(0, 1, 0), cell(0, 1, 0));

        assert_eq!(found, Some(vec![cell(0, 1, 0)]));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = floor(-1..=8, -2..=2);
        for x in 3..=8 {
            for z in -2..=2 {
                place(&mut grid, x, 1, z);
            }
        }

        let up = reach(&grid, cell(0, 1, 0), cell(6, 2, 0)).expect("a route up the rise");
        let down = reach(&grid, cell(6, 2, 0), cell(0, 1, 0)).expect("a route down the rise");

        let heights: Vec<i32> = up.iter().map(|c| c.y).collect();
        assert_eq!(heights, [1, 1, 1, 2, 2, 2, 2]);
        let reversed: Vec<BlockPos> = up.iter().rev().copied().collect();
        assert_eq!(down, reversed);
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = floor(-1..=8, -2..=2);
        for y in 1..=2 {
            for z in -2..=2 {
                place(&mut grid, 3, y, z);
            }
        }

        let found = reach(&grid, cell(0, 1, 0), cell(6, 1, 0));

        assert_eq!(found, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = floor(-1..=8, 0..=0);
        for x in 3..=8 {
            place(&mut grid, x, 1, 0);
        }
        let open = reach(&grid, cell(0, 1, 0), cell(5, 2, 0));
        place(&mut grid, 2, 4, 0);

        let low_ceiling = reach(&grid, cell(0, 1, 0), cell(5, 2, 0));

        assert!(open.is_some(), "the rise is climbable without the ceiling");
        assert_eq!(low_ceiling, None);
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = floor(-1..=8, -3..=3);
        for (x, z) in [(5, 0), (7, 0), (6, 1), (6, -1)] {
            for y in 1..=3 {
                place(&mut grid, x, y, z);
            }
        }

        let found = reach(&grid, cell(0, 1, 0), cell(6, 1, 0));

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = floor(-1..=30, -2..=2);
        let goal = cell(20, 1, 0);

        let starved = route(&grid, cell(0, 1, 0), |c| c == goal, goal, 5);
        let funded = reach(&grid, cell(0, 1, 0), goal);

        assert_eq!(starved, None);
        assert!(funded.is_some());
    }

    #[test]
    fn a_start_one_cell_off_the_ground_still_routes() {
        let grid = floor(-1..=8, -1..=1);

        let above = reach(&grid, cell(0, 2, 0), cell(4, 1, 0)).expect("start one above ground");
        let inside = reach(&grid, cell(0, 0, 0), cell(4, 1, 0)).expect("start one below ground");

        assert_eq!(above.first(), Some(&cell(0, 1, 0)));
        assert_eq!(inside.first(), Some(&cell(0, 1, 0)));
        assert_eq!(above.last(), Some(&cell(4, 1, 0)));
        assert_eq!(inside.last(), Some(&cell(4, 1, 0)));
    }

    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (i, &h) in heights.iter().enumerate() {
            let (x, z) = (i as i32 % 5, i as i32 / 5);
            for y in 0..h {
                place(&mut grid, x, y, z);
            }
        }
        grid
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1i32..=2, 25),
            (sx, sz, gx, gz) in (0i32..5, 0i32..5, 0i32..5, 0i32..5),
        ) {
            let grid = terrain(&heights);
            let at = |x: i32, z: i32| cell(x, heights[(z * 5 + x) as usize], z);
            let (start, goal) = (at(sx, sz), at(gx, gz));

            let found = route(&grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET);

            let cells = found.expect("one-block steps over open ground always connect");
            prop_assert_eq!(cells.first(), Some(&start));
            prop_assert_eq!(cells.last(), Some(&goal));
            for c in &cells {
                prop_assert!(standable(&grid, *c), "{c:?} is not standable");
            }
            for pair in cells.windows(2) {
                prop_assert!(adjacent(pair[0], pair[1]), "{:?} -> {:?}", pair[0], pair[1]);
            }
        }

        #[test]
        fn a_route_over_legal_steps_is_as_short_as_the_manhattan_distance(
            heights in proptest::collection::vec(1i32..=2, 25),
            (sx, sz, gx, gz) in (0i32..5, 0i32..5, 0i32..5, 0i32..5),
        ) {
            let grid = terrain(&heights);
            let at = |x: i32, z: i32| cell(x, heights[(z * 5 + x) as usize], z);
            let (start, goal) = (at(sx, sz), at(gx, gz));

            let found = route(&grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET);

            let cells = found.expect("every move is a legal one-block step");
            let manhattan = ((sx - gx).abs() + (sz - gz).abs()) as usize;
            prop_assert_eq!(cells.len(), manhattan + 1);
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1i32..=3, 25),
            (sx, sz, gx, gz) in (0i32..5, 0i32..5, 0i32..5, 0i32..5),
        ) {
            let grid = terrain(&heights);
            let at = |x: i32, z: i32| cell(x, heights[(z * 5 + x) as usize], z);
            let goal = at(gx, gz);

            let first = route(&grid, at(sx, sz), |c| c == goal, goal, ROUTE_NODE_BUDGET);
            let second = route(&grid, at(sx, sz), |c| c == goal, goal, ROUTE_NODE_BUDGET);

            prop_assert_eq!(first, second);
        }
    }
}
