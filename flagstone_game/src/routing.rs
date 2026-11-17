//! Routing for the strategy line's pawns: where a pawn can stand, and the cells it walks to
//! reach a goal over voxel ground.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// The most cells one search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: solid ground below and three cells of
/// air from the feet up (the capsule is 2.3 m tall).
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    false
}

/// The cell holding a pawn's feet.
pub fn feet_cell(_feet: Vec3) -> BlockPos {
    BlockPos::ZERO
}

/// The cells to walk from `start` to the nearest cell satisfying `is_goal`, both ends
/// included, or `None` when no goal is reachable within `budget` expansions. `toward` steers
/// the search; it does not have to be a goal.
///
/// Moves are one cell north, south, east or west, up or down at most one block; climbing needs
/// a fourth cell of air above the current feet. A `start` that is not standable tries the cell
/// above and below it. The same grid always gives the same route.
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

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// A floor of blocks at y = 0 over `-half..=half` in x and z; feet cells are at y = 1.
    fn flat_ground(half: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in -half..=half {
            for z in -half..=half {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    /// Stacks blocks on y = 1..=height across every column in `xs`, a wall or rise spanning
    /// the whole floor.
    fn raise(grid: &mut VoxelGrid, xs: std::ops::RangeInclusive<i32>, half: i32, height: i32) {
        for x in xs {
            for z in -half..=half {
                for y in 1..=height {
                    place(grid, x, y, z);
                }
            }
        }
    }

    fn go(
        grid: &VoxelGrid,
        start: BlockPos,
        goal: BlockPos,
        budget: usize,
    ) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, budget)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let grid = flat_ground(3);

        assert!(standable(&grid, cell(0, 1, 0)));
        assert!(!standable(&grid, cell(0, 0, 0)), "inside the ground");
        assert!(!standable(&grid, cell(0, 2, 0)), "nothing underfoot");
        assert!(
            !standable(&grid, cell(9, 1, 0)),
            "off the edge of the ground"
        );

        for blocked in [1, 2] {
            let mut low = flat_ground(3);
            place(&mut low, 0, 1 + blocked, 0);
            assert!(!standable(&low, cell(0, 1, 0)), "air needed at +{blocked}");
        }
        let mut head = flat_ground(3);
        place(&mut head, 0, 1, 0);
        assert!(
            !standable(&head, cell(0, 1, 0)),
            "the feet cell must be air"
        );
        let mut high = flat_ground(3);
        place(&mut high, 0, 4, 0);
        assert!(
            standable(&high, cell(0, 1, 0)),
            "three cells of air are enough"
        );
    }

    #[test]
    fn feet_cell_floors_the_position() {
        assert_eq!(feet_cell(Vec3::new(0.5, 1.0, 0.5)), cell(0, 1, 0));
        assert_eq!(feet_cell(Vec3::new(-0.2, 1.9, 2.99)), cell(-1, 1, 2));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = flat_ground(10);

        let path = go(&grid, cell(0, 1, 0), cell(4, 1, 0), ROUTE_NODE_BUDGET);

        assert_eq!(
            path,
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
        let mut grid = flat_ground(10);
        raise(&mut grid, 2..=10, 10, 1);

        let up = go(&grid, cell(0, 1, 0), cell(4, 2, 0), ROUTE_NODE_BUDGET);
        let down = go(&grid, cell(4, 2, 0), cell(0, 1, 0), ROUTE_NODE_BUDGET);

        assert_eq!(
            up,
            Some(vec![
                cell(0, 1, 0),
                cell(1, 1, 0),
                cell(2, 2, 0),
                cell(3, 2, 0),
                cell(4, 2, 0)
            ])
        );
        assert_eq!(
            down,
            Some(vec![
                cell(4, 2, 0),
                cell(3, 2, 0),
                cell(2, 2, 0),
                cell(1, 1, 0),
                cell(0, 1, 0)
            ])
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut wall = flat_ground(10);
        raise(&mut wall, 2..=10, 10, 2);
        let mut step = flat_ground(10);
        raise(&mut step, 2..=10, 10, 1);

        let over_the_wall = go(&wall, cell(0, 1, 0), cell(4, 3, 0), ROUTE_NODE_BUDGET);
        let over_the_step = go(&step, cell(0, 1, 0), cell(4, 2, 0), ROUTE_NODE_BUDGET);

        let down_the_wall = go(&wall, cell(4, 3, 0), cell(0, 1, 0), ROUTE_NODE_BUDGET);
        let down_the_step = go(&step, cell(4, 2, 0), cell(0, 1, 0), ROUTE_NODE_BUDGET);

        assert_eq!(over_the_wall, None);
        assert!(
            over_the_step.is_some(),
            "the same rise at one block is climbable"
        );
        assert_eq!(down_the_wall, None, "a drop of two blocks is too far");
        assert!(down_the_step.is_some(), "a drop of one block is fine");
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut low_ceiling = flat_ground(10);
        raise(&mut low_ceiling, 1..=10, 10, 1);
        let mut open = flat_ground(10);
        raise(&mut open, 1..=10, 10, 1);
        for z in -10..=10 {
            place(&mut low_ceiling, 0, 4, z);
        }

        let blocked = go(
            &low_ceiling,
            cell(0, 1, 0),
            cell(3, 2, 0),
            ROUTE_NODE_BUDGET,
        );
        let free = go(&open, cell(0, 1, 0), cell(3, 2, 0), ROUTE_NODE_BUDGET);

        assert!(
            standable(&low_ceiling, cell(0, 1, 0)),
            "the pawn fits where it starts"
        );
        assert_eq!(blocked, None);
        assert!(free.is_some(), "without the ceiling the climb is fine");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let ring = [(3, 0), (5, 0), (4, 1), (4, -1)];
        let mut enclosed = flat_ground(10);
        let mut gapped = flat_ground(10);
        for (i, &(x, z)) in ring.iter().enumerate() {
            for y in [1, 2] {
                place(&mut enclosed, x, y, z);
                if i != 0 {
                    place(&mut gapped, x, y, z);
                }
            }
        }

        let sealed = go(&enclosed, cell(0, 1, 0), cell(4, 1, 0), ROUTE_NODE_BUDGET);
        let with_gap = go(&gapped, cell(0, 1, 0), cell(4, 1, 0), ROUTE_NODE_BUDGET);

        assert!(
            standable(&enclosed, cell(4, 1, 0)),
            "the goal itself is open"
        );
        assert_eq!(sealed, None);
        assert!(with_gap.is_some(), "one open side lets the route in");
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = flat_ground(35);
        let goal = cell(30, 1, 0);

        let starved = go(&grid, cell(0, 1, 0), goal, 10);
        let funded = go(&grid, cell(0, 1, 0), goal, ROUTE_NODE_BUDGET);
        let unreachable = route(
            &grid,
            cell(0, 1, 0),
            |_| false,
            cell(30, 1, 0),
            ROUTE_NODE_BUDGET,
        );

        assert_eq!(starved, None);
        assert_eq!(funded.map(|path| path.len()), Some(31));
        assert_eq!(unreachable, None);
    }

    #[test]
    fn the_node_budget_counts_expansions_not_route_length() {
        let grid = flat_ground(35);
        let goal = cell(5, 1, 0);
        let away = cell(-30, 1, 0);

        let starved = route(&grid, cell(0, 1, 0), |c| c == goal, away, 10);
        let funded = route(&grid, cell(0, 1, 0), |c| c == goal, away, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None, "a short route found by a long search");
        assert_eq!(funded.map(|path| path.len()), Some(6));
    }

    #[test]
    fn a_start_that_is_not_standable_still_routes_from_the_ground_cell() {
        let grid = flat_ground(10);
        let goal = cell(3, 1, 0);

        let from_above = go(&grid, cell(0, 2, 0), goal, ROUTE_NODE_BUDGET);
        let from_inside = go(&grid, cell(0, 0, 0), goal, ROUTE_NODE_BUDGET);

        assert!(!standable(&grid, cell(0, 2, 0)));
        assert!(!standable(&grid, cell(0, 0, 0)));
        let expected = Some(vec![
            cell(0, 1, 0),
            cell(1, 1, 0),
            cell(2, 1, 0),
            cell(3, 1, 0),
        ]);
        assert_eq!(from_above, expected);
        assert_eq!(from_inside, expected);
    }

    const SIDE: usize = 9;

    /// A 9 x 9 patch of ground whose column heights are 0 or 1 above the floor, centred on the
    /// origin. Steps are at most one block with no ceiling, so any two columns connect.
    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (i, &height) in heights.iter().enumerate() {
            let (x, z) = (
                i32::try_from(i % SIDE).unwrap() - 4,
                i32::try_from(i / SIDE).unwrap() - 4,
            );
            for y in 0..=height {
                place(&mut grid, x, y, z);
            }
        }
        grid
    }

    fn standing_cell(heights: &[i32], (col, row): (usize, usize)) -> BlockPos {
        cell(
            i32::try_from(col).unwrap() - 4,
            heights[row * SIDE + col] + 1,
            i32::try_from(row).unwrap() - 4,
        )
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(0..=1i32, SIDE * SIDE),
            start in (0..SIDE, 0..SIDE),
            goal in (0..SIDE, 0..SIDE),
        ) {
            let grid = terrain(&heights);
            let (from, to) = (standing_cell(&heights, start), standing_cell(&heights, goal));

            let path = go(&grid, from, to, ROUTE_NODE_BUDGET)
                .ok_or_else(|| TestCaseError::fail("one-block steps always connect"))?;

            prop_assert_eq!(path.first(), Some(&from));
            prop_assert_eq!(path.last(), Some(&to));
            for &c in &path {
                prop_assert!(standable(&grid, c), "{c} is not standable");
            }
            for pair in path.windows(2) {
                let d = pair[1] - pair[0];
                prop_assert_eq!(d.x.abs() + d.z.abs(), 1, "{} to {}", pair[0], pair[1]);
                prop_assert!(d.y.abs() <= 1, "{} to {}", pair[0], pair[1]);
            }
            let flat_distance = (to - from).abs();
            prop_assert_eq!(
                path.len(),
                usize::try_from(flat_distance.x + flat_distance.z).unwrap() + 1,
                "a shortest route takes one move per column crossed"
            );
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(0..=1i32, SIDE * SIDE),
            start in (0..SIDE, 0..SIDE),
            goal in (0..SIDE, 0..SIDE),
        ) {
            let (from, to) = (standing_cell(&heights, start), standing_cell(&heights, goal));

            let first = go(&terrain(&heights), from, to, ROUTE_NODE_BUDGET);
            let second = go(&terrain(&heights), from, to, ROUTE_NODE_BUDGET);

            prop_assert!(first.is_some(), "one-block steps always connect");
            prop_assert_eq!(first, second);
        }
    }
}
