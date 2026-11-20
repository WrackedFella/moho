//! Where a pawn can stand and the way between two such places over the voxel ground.
//!
//! What a pawn can stand on is a pawn rule, so it lives on the strategy line rather than
//! with the voxel grid.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Cells of air a standing pawn needs from its feet up: the capsule is 2.3 m tall.
const CAPSULE_CELLS: i32 = 3;

/// The four horizontal steps, in the order they are tried.
const HORIZONTAL: [BlockPos; 4] = [
    BlockPos::new(1, 0, 0),
    BlockPos::new(-1, 0, 0),
    BlockPos::new(0, 0, 1),
    BlockPos::new(0, 0, -1),
];

/// Most cells a search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// The cell whose volume holds `feet`: each axis floored.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// Whether a pawn can stand with its feet in `cell`: solid ground under it and the capsule's
/// height of air, three cells, from the feet up.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..CAPSULE_CELLS).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// A short walk from `start` to a cell `is_goal` accepts, over standable cells. The estimate
/// aims at `toward` and can overshoot when the goal is a window around it, so the walk is
/// not always the shortest.
///
/// A walk moves to one of the four horizontal neighbours, one block up or down at most; a step
/// up needs a fourth cell of air above the current feet. A `start` that is not standable
/// tries the cells one above and one below it before giving up. `toward` is where the goal
/// lies, for the search's estimate. The result lists the cells after the one the walk began
/// in, ending in the goal, so a `start` that is already a goal gives `Some` of an empty route.
/// Ties between equal routes break the same way every time. `None` when there is no route, or
/// the search expands `budget` cells without reaching a goal.
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
    let estimate = |cell: BlockPos| {
        let gap = (cell - toward).abs();
        gap.x + gap.y + gap.z
    };

    // Each node remembers where it was reached from. The queue orders by estimated total cost,
    // then cost so far, then insertion, so equal routes always break the same way.
    let mut nodes = vec![(start, usize::MAX)];
    let mut cheapest: HashMap<BlockPos, i32> = HashMap::from([(start, 0)]);
    let mut queue = BinaryHeap::from([Reverse((estimate(start), 0, 0usize))]);
    let mut expanded = 0;
    while let Some(Reverse((_, cost, index))) = queue.pop() {
        let (cell, _) = nodes[index];
        if cheapest.get(&cell).is_some_and(|&best| best < cost) {
            continue;
        }
        if is_goal(cell) {
            let mut cells = Vec::new();
            let mut at = index;
            while nodes[at].1 != usize::MAX {
                cells.push(nodes[at].0);
                at = nodes[at].1;
            }
            cells.reverse();
            return Some(cells);
        }
        if expanded >= budget {
            return None;
        }
        expanded += 1;
        for next in neighbours(grid, cell) {
            let next_cost = cost + 1;
            if cheapest.get(&next).is_some_and(|&best| best <= next_cost) {
                continue;
            }
            cheapest.insert(next, next_cost);
            nodes.push((next, index));
            queue.push(Reverse((
                next_cost + estimate(next),
                next_cost,
                nodes.len() - 1,
            )));
        }
    }
    None
}

/// The standable cells a pawn can step to from `cell`, in a fixed order.
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    // A step up needs a fourth cell of air over the current feet.
    let headroom = !grid.is_solid_at(cell + BlockPos::Y * CAPSULE_CELLS);
    HORIZONTAL.into_iter().flat_map(move |side| {
        [0, 1, -1]
            .into_iter()
            .filter(move |&rise| rise != 1 || headroom)
            .map(move |rise| cell + side + BlockPos::Y * rise)
            .filter(|&next| standable(grid, next))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// A one-block-wide floor along +X: blocks at y 0, x `0..=last`, z 0.
    fn corridor(last: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(last, 0, 0), 1);
        grid
    }

    /// A flat floor of blocks at y 0 over x and z `0..=last`.
    fn plain(last: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(last, 0, last), 1);
        grid
    }

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(cell(x, y, z), 1, None);
    }

    fn route_to(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        let cases = [
            (Vec3::new(3.7, 1.0, 5.2), cell(3, 1, 5)),
            (Vec3::new(-0.2, 4.05, -3.9), cell(-1, 4, -4)),
            (Vec3::new(2.0, 0.999, -0.0), cell(2, 0, 0)),
        ];

        for (feet, expected) in cases {
            assert_eq!(feet_cell(feet), expected, "feet {feet}");
        }
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = VoxelGrid::new(16);
        place(&mut grid, 0, 0, 0);

        assert!(standable(&grid, cell(0, 1, 0)), "ground and open air");
        assert!(!standable(&grid, cell(1, 1, 0)), "no ground below");
        assert!(!standable(&grid, cell(0, 2, 0)), "the ground is two down");
        assert!(!standable(&grid, cell(0, 0, 0)), "the cell itself is solid");
        for blocked in [1, 2, 3] {
            let mut roofed = VoxelGrid::new(16);
            place(&mut roofed, 0, 0, 0);
            place(&mut roofed, 0, blocked, 0);

            assert!(
                !standable(&roofed, cell(0, 1, 0)),
                "a block at y {blocked} is inside the capsule"
            );
        }
        let mut high_roof = VoxelGrid::new(16);
        place(&mut high_roof, 0, 0, 0);
        place(&mut high_roof, 0, 4, 0);
        assert!(
            standable(&high_roof, cell(0, 1, 0)),
            "a roof above the capsule is clear of it"
        );
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = corridor(9);

        let found = route_to(&grid, cell(0, 1, 0), cell(5, 1, 0));

        assert_eq!(
            found,
            Some((1..=5).map(|x| cell(x, 1, 0)).collect::<Vec<_>>())
        );
    }

    #[test]
    fn a_start_that_is_the_goal_needs_no_steps() {
        let grid = corridor(9);

        let found = route_to(&grid, cell(3, 1, 0), cell(3, 1, 0));

        assert_eq!(found, Some(Vec::new()));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = corridor(9);
        place(&mut grid, 4, 1, 0);
        place(&mut grid, 5, 1, 0);

        let found = route_to(&grid, cell(0, 1, 0), cell(9, 1, 0));

        let heights: Vec<i32> = found
            .expect("a one-block rise can be walked over")
            .iter()
            .map(|c| c.y)
            .collect();
        assert_eq!(heights, [1, 1, 1, 2, 2, 1, 1, 1, 1]);
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = corridor(9);
        place(&mut grid, 4, 1, 0);
        place(&mut grid, 4, 2, 0);

        let found = route_to(&grid, cell(0, 1, 0), cell(9, 1, 0));

        assert_eq!(found, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = corridor(9);
        place(&mut grid, 4, 1, 0);
        let clear = route_to(&grid, cell(0, 1, 0), cell(9, 1, 0));
        // Four cells above the feet at x 3, where the step up begins.
        place(&mut grid, 3, 4, 0);

        let roofed = route_to(&grid, cell(0, 1, 0), cell(9, 1, 0));

        assert!(clear.is_some(), "the rise is walkable without the roof");
        assert_eq!(roofed, None, "the step up would hit the roof");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = plain(8);
        for (x, z) in [(3, 4), (5, 4), (4, 3), (4, 5)] {
            place(&mut grid, x, 1, z);
            place(&mut grid, x, 2, z);
        }

        let found = route_to(&grid, cell(0, 1, 0), cell(4, 1, 4));

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = corridor(30);
        let goal = cell(25, 1, 0);

        let starved = route(&grid, cell(0, 1, 0), |c| c == goal, goal, 5);
        let funded = route(&grid, cell(0, 1, 0), |c| c == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None, "25 steps cannot be found in 5 expansions");
        assert_eq!(funded.map(|r| r.len()), Some(25));
    }

    #[test]
    fn a_start_on_a_slope_tries_the_cells_above_and_below() {
        let mut grid = corridor(9);
        place(&mut grid, 4, 1, 0);

        // Feet a block too high, and a block too low: inside the ground at x 4.
        let above = route_to(&grid, cell(0, 2, 0), cell(2, 1, 0));
        let below = route_to(&grid, cell(4, 1, 0), cell(6, 1, 0));

        assert_eq!(above.map(|r| r.last().copied()), Some(Some(cell(2, 1, 0))));
        assert_eq!(below.map(|r| r.last().copied()), Some(Some(cell(6, 1, 0))));
    }

    #[test]
    fn a_start_inside_the_ground_steps_up_to_the_cell_above_it() {
        let mut grid = corridor(9);
        place(&mut grid, 4, 1, 0);

        let found = route_to(&grid, cell(4, 1, 0), cell(6, 1, 0));

        assert_eq!(found, Some(vec![cell(5, 1, 0), cell(6, 1, 0)]));
    }

    #[test]
    fn an_open_search_expands_only_the_cells_on_the_way() {
        let grid = plain(20);
        let start = cell(2, 1, 10);
        let goal = cell(10, 1, 10);

        // The eight cells from the start to the one before the goal, and no others.
        let exact = route(&grid, start, |c| c == goal, goal, 8);
        let short = route(&grid, start, |c| c == goal, goal, 7);

        assert_eq!(exact.map(|r| r.len()), Some(8));
        assert_eq!(short, None);
    }

    /// The fewest steps from `start` to `goal`, by breadth-first search over the same moves.
    fn fewest_steps(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<usize> {
        let sides = [cell(1, 0, 0), cell(-1, 0, 0), cell(0, 0, 1), cell(0, 0, -1)];
        let mut steps = std::collections::HashMap::from([(start, 0usize)]);
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(at) = queue.pop_front() {
            if at == goal {
                return steps.get(&at).copied();
            }
            let headroom = !grid.is_solid_at(at + cell(0, 3, 0));
            for side in sides {
                for rise in [0, 1, -1] {
                    let next = at + side + cell(0, rise, 0);
                    if (rise == 1 && !headroom)
                        || steps.contains_key(&next)
                        || !standable(grid, next)
                    {
                        continue;
                    }
                    steps.insert(next, steps[&at] + 1);
                    queue.push_back(next);
                }
            }
        }
        None
    }

    /// A 6 by 6 patch of columns, each `heights[x * 6 + z]` blocks tall.
    fn hills(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..6 {
            for z in 0..6 {
                for y in 0..heights[(x * 6 + z) as usize] {
                    place(&mut grid, x, y, z);
                }
            }
        }
        grid
    }

    fn top_of(heights: &[i32], x: i32, z: i32) -> BlockPos {
        cell(x, heights[(x * 6 + z) as usize], z)
    }

    /// Column heights of a 6 by 6 patch that climbs and falls by at most a block per step along
    /// each row and column, so a goal can lie several blocks above or below the start.
    fn terraces(steps: &[i32]) -> Vec<i32> {
        let mut heights = vec![1; 36];
        for x in 0..6usize {
            for z in 0..6usize {
                let before = match (x, z) {
                    (0, 0) => 4,
                    (_, 0) => heights[(x - 1) * 6],
                    _ => heights[x * 6 + z - 1],
                };
                heights[x * 6 + z] = (before + steps[x * 6 + z]).clamp(1, 9);
            }
        }
        heights
    }

    proptest! {
        #[test]
        fn a_route_over_terraces_is_as_short_as_any_walk(
            steps in proptest::collection::vec(-1..=1i32, 36),
            start in (0..6i32, 0..6i32),
            goal in (0..6i32, 0..6i32),
        ) {
            let heights = terraces(&steps);
            let grid = hills(&heights);
            let start = top_of(&heights, start.0, start.1);
            let goal = top_of(&heights, goal.0, goal.1);

            let found = route_to(&grid, start, goal);

            prop_assert_eq!(found.map(|cells| cells.len()), fewest_steps(&grid, start, goal));
        }

        #[test]
        fn a_route_is_as_short_as_any_walk_and_exists_whenever_a_walk_does(
            heights in proptest::collection::vec(1..=4i32, 36),
            start in (0..6i32, 0..6i32),
            goal in (0..6i32, 0..6i32),
        ) {
            let grid = hills(&heights);
            let start = top_of(&heights, start.0, start.1);
            let goal = top_of(&heights, goal.0, goal.1);

            let found = route_to(&grid, start, goal);

            prop_assert_eq!(found.map(|cells| cells.len()), fewest_steps(&grid, start, goal));
        }

        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1..=4i32, 36),
            start in (0..6i32, 0..6i32),
            goal in (0..6i32, 0..6i32),
        ) {
            let grid = hills(&heights);
            let start = top_of(&heights, start.0, start.1);
            let goal = top_of(&heights, goal.0, goal.1);

            let found = route_to(&grid, start, goal);

            if start == goal {
                prop_assert_eq!(&found, &Some(Vec::new()));
            }
            if let Some(cells) = found {
                let mut previous = start;
                for next in &cells {
                    prop_assert!(standable(&grid, *next), "{next} is not standable");
                    let step = *next - previous;
                    prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "{} to {}", previous, next);
                    prop_assert!(step.y.abs() <= 1, "{} to {}", previous, next);
                    previous = *next;
                }
                prop_assert_eq!(previous, goal);
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1..=3i32, 36),
            start in (0..6i32, 0..6i32),
            goal in (0..6i32, 0..6i32),
        ) {
            let first_grid = hills(&heights);
            let second_grid = hills(&heights);
            let start = top_of(&heights, start.0, start.1);
            let goal = top_of(&heights, goal.0, goal.1);

            let first = route_to(&first_grid, start, goal);
            let again = route_to(&first_grid, start, goal);
            let rebuilt = route_to(&second_grid, start, goal);

            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &rebuilt);
        }
    }
}
