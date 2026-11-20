//! Ground routing over the voxel grid: where a pawn can stand and how it walks between cells.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Node expansions a single [`route`] search may spend before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn's feet can rest in `cell`: solid ground directly below and three cells of
/// air from the feet up (the capsule is 2.3 m tall).
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    todo!("SG-F4-03: standable")
}

/// The cell holding a pawn's feet: each coordinate floored.
pub fn feet_cell(_feet: Vec3) -> BlockPos {
    todo!("SG-F4-03: feet_cell")
}

/// A* over standable cells from `start` to the nearest cell satisfying `is_goal`.
///
/// Moves are the four horizontal neighbours, each at the same height or one block up or down;
/// a step up needs a fourth air cell above the current feet. `toward` steers the Manhattan
/// heuristic. The returned cells exclude `start` and end at the goal cell (empty when `start`
/// is already a goal). A `start` that is not standable tries the cell one above and one below.
/// Returns `None` when no route exists or `budget` expansions are spent first.
pub fn route(
    _grid: &VoxelGrid,
    _start: BlockPos,
    _is_goal: impl Fn(BlockPos) -> bool,
    _toward: BlockPos,
    _budget: usize,
) -> Option<Vec<BlockPos>> {
    todo!("SG-F4-03: route")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const GROUND: u32 = 1;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// Ground one block thick (`y == 0`) over `x` in `0..=9`, `z` in `0..=4`.
    fn flat_ground() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(9, 0, 4), GROUND);
        grid
    }

    fn route_to(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = flat_ground();
        let feet = cell(2, 1, 2);
        assert!(standable(&grid, feet), "open ground");
        assert!(!standable(&grid, cell(2, 0, 2)), "inside the ground block");
        assert!(!standable(&grid, cell(2, 2, 2)), "air with no ground below");

        grid.mutator().place(cell(2, 4, 2), GROUND, None);
        assert!(
            standable(&grid, feet),
            "a block above the third air cell is clear"
        );

        grid.mutator().place(cell(2, 3, 2), GROUND, None);
        assert!(!standable(&grid, feet), "the third air cell is blocked");
    }

    #[test]
    fn feet_cell_floors_each_coordinate() {
        assert_eq!(feet_cell(Vec3::new(2.9, 1.0, 3.1)), cell(2, 1, 3));
        assert_eq!(feet_cell(Vec3::new(-0.1, 0.99, -1.5)), cell(-1, 0, -2));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(0, 1, 2), cell(5, 1, 2));

        assert_eq!(
            path,
            Some(vec![
                cell(1, 1, 2),
                cell(2, 1, 2),
                cell(3, 1, 2),
                cell(4, 1, 2),
                cell(5, 1, 2)
            ])
        );
    }

    #[test]
    fn a_start_that_is_already_a_goal_needs_no_steps() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(2, 1, 2), cell(2, 1, 2));

        assert_eq!(path, Some(vec![]));
    }

    #[test]
    fn a_start_one_cell_off_the_ground_still_routes() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(0, 2, 2), cell(2, 1, 2));

        assert_eq!(path.and_then(|p| p.last().copied()), Some(cell(2, 1, 2)));
    }

    #[test]
    fn a_start_inside_the_ground_routes_from_the_cell_above() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(0, 0, 2), cell(2, 1, 2));

        assert_eq!(path.and_then(|p| p.last().copied()), Some(cell(2, 1, 2)));
    }

    /// Flat ground with a plateau one block higher for `x >= 3`.
    fn rise() -> VoxelGrid {
        let mut grid = flat_ground();
        grid.mutator()
            .fill_region(cell(3, 1, 0), cell(9, 1, 4), GROUND);
        grid
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let grid = rise();

        let up = route_to(&grid, cell(0, 1, 2), cell(6, 2, 2));
        let down = route_to(&grid, cell(6, 2, 2), cell(0, 1, 2));

        assert_eq!(
            up,
            Some(vec![
                cell(1, 1, 2),
                cell(2, 1, 2),
                cell(3, 2, 2),
                cell(4, 2, 2),
                cell(5, 2, 2),
                cell(6, 2, 2)
            ])
        );
        assert_eq!(
            down,
            Some(vec![
                cell(5, 2, 2),
                cell(4, 2, 2),
                cell(3, 2, 2),
                cell(2, 1, 2),
                cell(1, 1, 2),
                cell(0, 1, 2)
            ])
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = flat_ground();
        grid.mutator()
            .fill_region(cell(3, 1, 0), cell(3, 2, 4), GROUND);

        let path = route_to(&grid, cell(0, 1, 2), cell(5, 1, 2));

        assert_eq!(path, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = rise();
        grid.mutator()
            .fill_region(cell(2, 4, 0), cell(2, 4, 4), GROUND);

        let path = route_to(&grid, cell(0, 1, 2), cell(6, 2, 2));

        assert_eq!(
            path, None,
            "no fourth air cell above the cell before the step"
        );
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = flat_ground();
        for around in [cell(4, 1, 2), cell(6, 1, 2), cell(5, 1, 1), cell(5, 1, 3)] {
            grid.mutator()
                .fill_region(around, cell(around.x, 2, around.z), GROUND);
        }

        let path = route_to(&grid, cell(0, 1, 2), cell(5, 1, 2));

        assert_eq!(path, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = flat_ground();
        let goal = cell(5, 1, 2);

        let starved = route(&grid, cell(0, 1, 2), |c| c == goal, goal, 3);
        let funded = route(&grid, cell(0, 1, 2), |c| c == goal, goal, 100);

        assert_eq!(starved, None);
        assert_eq!(funded.map(|p| p.len()), Some(5));
        assert_eq!(ROUTE_NODE_BUDGET, 4096);
    }

    /// A 8x8 heightmap: column `(x, z)` is solid for `y` in `0..height`.
    fn terrain(heights: &[u8]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..8 {
            for z in 0..8 {
                let h = i32::from(heights[(x * 8 + z) as usize]);
                grid.mutator()
                    .fill_region(cell(x, 0, z), cell(x, h - 1, z), GROUND);
            }
        }
        grid
    }

    fn feet_on(heights: &[u8], x: i32, z: i32) -> BlockPos {
        cell(x, i32::from(heights[(x * 8 + z) as usize]), z)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1u8..=2, 64),
            sx in 0i32..8, sz in 0i32..8, gx in 0i32..8, gz in 0i32..8,
        ) {
            let grid = terrain(&heights);
            let start = feet_on(&heights, sx, sz);
            let goal = feet_on(&heights, gx, gz);

            let path = route_to(&grid, start, goal);

            prop_assert!(path.is_some(), "heights of one or two blocks are always walkable");
            let path = path.unwrap_or_default();
            prop_assert_eq!(path.last().copied().unwrap_or(start), goal);
            let mut previous = start;
            for step in path {
                prop_assert!(standable(&grid, step), "{step} is not standable");
                let horizontal = (step.x - previous.x).abs() + (step.z - previous.z).abs();
                prop_assert_eq!(horizontal, 1, "{} to {} is not one horizontal move", previous, step);
                prop_assert!((step.y - previous.y).abs() <= 1, "{} to {} climbs too far", previous, step);
                previous = step;
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1u8..=2, 64),
            sx in 0i32..8, sz in 0i32..8, gx in 0i32..8, gz in 0i32..8,
        ) {
            let start = feet_on(&heights, sx, sz);
            let goal = feet_on(&heights, gx, gz);
            let first_grid = terrain(&heights);
            let second_grid = terrain(&heights);

            let first = route_to(&first_grid, start, goal);
            let again = route_to(&first_grid, start, goal);
            let rebuilt = route_to(&second_grid, start, goal);

            prop_assert!(first.is_some(), "heights of one or two blocks are always walkable");
            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &rebuilt);
        }
    }
}
