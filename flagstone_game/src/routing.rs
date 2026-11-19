//! Ground routing for pawns: which cells a pawn can stand on and the way between them.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// The most cells one search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground below and three cells of air
/// from the feet up.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y) && clear_above(grid, cell, 0)
}

/// Whether the three cells of the capsule's height from `cell + lift` up are air.
fn clear_above(grid: &VoxelGrid, cell: BlockPos, lift: i32) -> bool {
    (lift..lift + 3).all(|dy| !grid.is_solid_at(cell + BlockPos::new(0, dy, 0)))
}

/// The cell a pawn's feet are in.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// The horizontal neighbours a pawn steps to, in the fixed order that makes ties repeatable.
const STEPS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The standable cells one step from `cell`: level, one block up (with a fourth cell of
/// air over the current feet), or one block down (with the capsule's height clear at the
/// current level).
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    let head_clear = !grid.is_solid_at(cell + BlockPos::new(0, 3, 0));
    STEPS.into_iter().flat_map(move |(dx, dz)| {
        let level = cell + BlockPos::new(dx, 0, dz);
        let up = level + BlockPos::Y;
        let down = level - BlockPos::Y;
        let on_level = standable(grid, level).then_some(level);
        let climbing = (head_clear && standable(grid, up)).then_some(up);
        let dropping = (standable(grid, down) && clear_above(grid, level, 0)).then_some(down);
        [on_level, climbing, dropping].into_iter().flatten()
    })
}

fn key(cell: BlockPos) -> (i32, i32, i32) {
    (cell.x, cell.y, cell.z)
}

fn horizontal_distance(a: BlockPos, b: BlockPos) -> i32 {
    (a.x - b.x).abs() + (a.z - b.z).abs()
}

/// The cheapest walkable route from `start` to the nearest cell satisfying `is_goal`,
/// both ends included, or `None` when there is none or the search spends `budget`
/// expansions first. `toward` steers the search.
///
/// A `start` that is not standable tries the cells one above and one below it.
pub fn route(
    grid: &VoxelGrid,
    start: BlockPos,
    is_goal: impl Fn(BlockPos) -> bool,
    toward: BlockPos,
    budget: usize,
) -> Option<Vec<BlockPos>> {
    let start = [start, start + BlockPos::Y, start - BlockPos::Y]
        .into_iter()
        .find(|&cell| standable(grid, cell))?;

    // Ordered by (f, larger g first, insertion order); `Reverse` makes the heap a min-heap.
    let mut open = BinaryHeap::new();
    let mut best_cost = HashMap::from([(key(start), 0_i32)]);
    let mut came_from: HashMap<(i32, i32, i32), BlockPos> = HashMap::new();
    let mut inserted = 0_u32;
    open.push(Reverse((
        horizontal_distance(start, toward),
        Reverse(0_i32),
        inserted,
        key(start),
    )));

    let mut expansions = 0;
    while let Some(Reverse((_, Reverse(cost), _, current))) = open.pop() {
        let cell = BlockPos::new(current.0, current.1, current.2);
        if best_cost.get(&current).is_some_and(|&best| cost > best) {
            continue;
        }
        if is_goal(cell) {
            let mut path = vec![cell];
            let mut at = cell;
            while let Some(&previous) = came_from.get(&key(at)) {
                path.push(previous);
                at = previous;
            }
            path.reverse();
            return Some(path);
        }
        if expansions >= budget {
            return None;
        }
        expansions += 1;

        for next in neighbours(grid, cell) {
            let next_cost = cost + 1;
            if best_cost
                .get(&key(next))
                .is_some_and(|&best| best <= next_cost)
            {
                continue;
            }
            best_cost.insert(key(next), next_cost);
            came_from.insert(key(next), cell);
            inserted += 1;
            open.push(Reverse((
                next_cost + horizontal_distance(next, toward),
                Reverse(next_cost),
                inserted,
                key(next),
            )));
        }
    }
    None
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

    /// A one-block-thick floor at y = 0 over the given inclusive ranges.
    fn floor(xs: std::ops::RangeInclusive<i32>, zs: std::ops::RangeInclusive<i32>) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in xs {
            for z in zs.clone() {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    fn to(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        struct Case {
            name: &'static str,
            solid: &'static [(i32, i32, i32)],
            cell: (i32, i32, i32),
            standable: bool,
        }
        let cases = [
            Case {
                name: "on the floor",
                solid: &[],
                cell: (2, 1, 2),
                standable: true,
            },
            Case {
                name: "in the air above the floor",
                solid: &[],
                cell: (2, 2, 2),
                standable: false,
            },
            Case {
                name: "over a hole in the floor",
                solid: &[],
                cell: (9, 1, 9),
                standable: false,
            },
            Case {
                name: "feet inside a block",
                solid: &[(2, 1, 2)],
                cell: (2, 1, 2),
                standable: false,
            },
            Case {
                name: "block one above the feet",
                solid: &[(2, 2, 2)],
                cell: (2, 1, 2),
                standable: false,
            },
            Case {
                name: "block two above the feet",
                solid: &[(2, 3, 2)],
                cell: (2, 1, 2),
                standable: false,
            },
            Case {
                name: "block three above the feet is clear of the capsule",
                solid: &[(2, 4, 2)],
                cell: (2, 1, 2),
                standable: true,
            },
            Case {
                name: "on top of a block",
                solid: &[(2, 1, 2)],
                cell: (2, 2, 2),
                standable: true,
            },
        ];

        for case in cases {
            let mut grid = floor(0..=4, 0..=4);
            for &(x, y, z) in case.solid {
                place(&mut grid, x, y, z);
            }

            let result = standable(&grid, cell(case.cell.0, case.cell.1, case.cell.2));

            assert_eq!(result, case.standable, "{}", case.name);
        }
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        assert_eq!(feet_cell(Vec3::new(2.5, 1.0, 2.5)), cell(2, 1, 2));
        assert_eq!(feet_cell(Vec3::new(3.0, 1.2, 3.99)), cell(3, 1, 3));
        assert_eq!(feet_cell(Vec3::new(-0.1, 0.99, -1.5)), cell(-1, 0, -2));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = floor(0..=8, 0..=8);

        let found = to(&grid, cell(0, 1, 0), cell(5, 1, 0));

        let expected: Vec<_> = (0..=5).map(|x| cell(x, 1, 0)).collect();
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = floor(0..=8, 0..=0);
        for x in 3..=5 {
            place(&mut grid, x, 1, 0);
        }

        let found = to(&grid, cell(0, 1, 0), cell(7, 1, 0));

        let expected = vec![
            cell(0, 1, 0),
            cell(1, 1, 0),
            cell(2, 1, 0),
            cell(3, 2, 0),
            cell(4, 2, 0),
            cell(5, 2, 0),
            cell(6, 1, 0),
            cell(7, 1, 0),
        ];
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = floor(0..=8, 0..=2);
        for z in 0..=2 {
            place(&mut grid, 4, 1, z);
            place(&mut grid, 4, 2, z);
        }

        let found = to(&grid, cell(0, 1, 1), cell(8, 1, 1));

        assert_eq!(found, None);
    }

    #[test]
    fn a_gap_in_a_two_block_wall_lets_the_route_through() {
        let mut grid = floor(0..=8, 0..=2);
        for z in [0, 2] {
            place(&mut grid, 4, 1, z);
            place(&mut grid, 4, 2, z);
        }

        let found = to(&grid, cell(0, 1, 1), cell(8, 1, 1)).expect("the gap is a way through");

        assert!(found.contains(&cell(4, 1, 1)));
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = floor(0..=8, 0..=0);
        for x in 3..=5 {
            place(&mut grid, x, 1, 0);
        }
        let start = cell(0, 1, 0);
        let goal = cell(7, 1, 0);
        assert!(to(&grid, start, goal).is_some(), "open sky lets it climb");

        place(&mut grid, 2, 4, 0);
        let found = to(&grid, start, goal);

        assert_eq!(found, None);
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = floor(0..=10, 0..=10);
        for x in 4..=6 {
            for z in 4..=6 {
                if (x, z) != (5, 5) {
                    place(&mut grid, x, 1, z);
                    place(&mut grid, x, 2, z);
                }
            }
        }
        let goal = cell(5, 1, 5);
        assert!(standable(&grid, goal), "the goal itself is a stand cell");

        let found = to(&grid, cell(0, 1, 0), goal);

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = floor(0..=19, 0..=19);
        let start = cell(0, 1, 0);
        let goal = cell(19, 1, 19);

        let starved = route(&grid, start, |c| c == goal, goal, 5);
        let funded = route(&grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None);
        assert_eq!(funded.expect("within budget").len(), 39);
    }

    #[test]
    fn a_start_inside_the_ground_snaps_up_one_cell() {
        let mut grid = floor(0..=8, 0..=0);
        place(&mut grid, 3, 1, 0);

        let found = to(&grid, cell(3, 1, 0), cell(6, 1, 0)).expect("the cell above is standable");

        assert_eq!(found.first(), Some(&cell(3, 2, 0)));
        assert_eq!(found.last(), Some(&cell(6, 1, 0)));
    }

    #[test]
    fn a_start_one_cell_in_the_air_snaps_down_one_cell() {
        let grid = floor(0..=8, 0..=0);

        let found = to(&grid, cell(3, 2, 0), cell(6, 1, 0)).expect("the cell below is standable");

        assert_eq!(found.first(), Some(&cell(3, 1, 0)));
    }

    #[test]
    fn a_start_two_cells_from_standable_has_no_route() {
        let grid = floor(0..=8, 0..=0);

        let found = to(&grid, cell(3, 3, 0), cell(6, 1, 0));

        assert_eq!(found, None);
    }

    #[test]
    fn a_goal_that_is_the_start_is_a_one_cell_route() {
        let grid = floor(0..=4, 0..=4);

        let found = to(&grid, cell(2, 1, 2), cell(2, 1, 2));

        assert_eq!(found, Some(vec![cell(2, 1, 2)]));
    }

    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (i, &h) in heights.iter().enumerate() {
            let (x, z) = (i as i32 % 6, i as i32 / 6);
            for y in 0..h {
                place(&mut grid, x, y, z);
            }
        }
        grid
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1i32..=3, 36),
            from in 0usize..36,
            goal in 0usize..36,
        ) {
            let grid = terrain(&heights);
            let top = |i: usize| cell(i as i32 % 6, heights[i], i as i32 / 6);
            let (start, goal) = (top(from), top(goal));

            let found = to(&grid, start, goal);

            if let Some(path) = found {
                prop_assert_eq!(path.first(), Some(&start));
                prop_assert_eq!(path.last(), Some(&goal));
                for c in &path {
                    prop_assert!(standable(&grid, *c), "{c} is not standable");
                }
                for pair in path.windows(2) {
                    let d = pair[1] - pair[0];
                    prop_assert_eq!(d.x.abs() + d.z.abs(), 1, "one horizontal step");
                    prop_assert!(d.y.abs() <= 1, "at most one block up or down");
                }
            }
        }

        /// Heights differ by at most one between neighbours and nothing roofs the walk,
        /// so every pair of tops is connected and a search must find the route.
        #[test]
        fn open_terrain_always_has_a_route_of_standable_adjacent_cells(
            heights in proptest::collection::vec(1i32..=2, 36),
            from in 0usize..36,
            goal in 0usize..36,
        ) {
            let grid = terrain(&heights);
            let top = |i: usize| cell(i as i32 % 6, heights[i], i as i32 / 6);
            let (start, goal) = (top(from), top(goal));

            let found = to(&grid, start, goal);

            prop_assert!(found.is_some(), "no route from {start} to {goal}");
            let path = found.expect("checked above");
            prop_assert_eq!(path.first(), Some(&start));
            prop_assert_eq!(path.last(), Some(&goal));
            for c in &path {
                prop_assert!(standable(&grid, *c), "{c} is not standable");
            }
            for pair in path.windows(2) {
                let d = pair[1] - pair[0];
                prop_assert_eq!(d.x.abs() + d.z.abs(), 1, "one horizontal step");
                prop_assert!(d.y.abs() <= 1, "at most one block up or down");
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1i32..=2, 36),
            from in 0usize..36,
            goal in 0usize..36,
        ) {
            let top = |i: usize| cell(i as i32 % 6, heights[i], i as i32 / 6);
            let (start, goal) = (top(from), top(goal));
            let first_grid = terrain(&heights);
            // The same world with the blocks placed in the opposite order.
            let mut second_grid = VoxelGrid::new(16);
            for (i, &h) in heights.iter().enumerate().rev() {
                for y in (0..h).rev() {
                    place(&mut second_grid, i as i32 % 6, y, i as i32 / 6);
                }
            }

            let first = to(&first_grid, start, goal);
            let again = to(&first_grid, start, goal);
            let other = to(&second_grid, start, goal);

            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &other);
        }
    }
}
