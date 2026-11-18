//! Where a pawn can walk: standable cells and the shortest way between them.
//!
//! A cell is the block position of a pawn's feet. Reading the grid through `is_solid_at`
//! keeps this a pawn rule on the strategy line rather than a property of the voxel store.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Node expansions a search may spend before it gives up with `None`.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground below and three cells of air
/// from the feet up (the capsule is 2.3 m tall).
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..CAPSULE_CELLS).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// Cells of air a standing pawn occupies, from the feet up.
const CAPSULE_CELLS: i32 = 3;

const HORIZONTAL_STEPS: [BlockPos; 4] =
    [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z];

/// A search node's ordering key: lowest cost estimate, then lowest cost so far, then the
/// order it was queued in.
type Queued = Reverse<(i32, i32, u32)>;

fn manhattan(a: BlockPos, b: BlockPos) -> i32 {
    (a.x - b.x).abs() + (a.z - b.z).abs()
}

/// The standable cells one step from `from`: level, one up (with a fourth cell of air above
/// the current feet) or one down.
fn neighbours(grid: &VoxelGrid, from: BlockPos) -> impl Iterator<Item = BlockPos> {
    let can_climb = !grid.is_solid_at(from + BlockPos::Y * CAPSULE_CELLS);
    HORIZONTAL_STEPS.into_iter().flat_map(move |step| {
        let level = from + step;
        [
            (level, true),
            (level + BlockPos::Y, can_climb),
            (level - BlockPos::Y, true),
        ]
        .into_iter()
        .filter_map(|(cell, allowed)| (allowed && standable(grid, cell)).then_some(cell))
    })
}

/// The cell a pawn's feet are in.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// The cells to walk through, in order, from `start` to the nearest cell that satisfies
/// `is_goal`; empty when `start` already does.
///
/// A* over standable cells with four horizontal neighbours, stepping up or down one block
/// (up needs a fourth cell of air above the current feet), a Manhattan heuristic towards
/// `toward`, and ties broken by cost then insertion order so a grid always gives the same
/// route. A `start` that is not standable tries the cell above and the cell below it.
/// `None` when no route exists or `budget` expansions are spent first.
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

    // The cell of each queue entry, indexed by its insertion order.
    let mut queued_cells = vec![start];
    let mut open: BinaryHeap<Queued> = BinaryHeap::new();
    open.push(Reverse((manhattan(start, toward), 0, 0)));
    // Cheapest known cost to each cell and the cell it was reached from.
    let mut reached: HashMap<BlockPos, (i32, Option<BlockPos>)> = HashMap::new();
    reached.insert(start, (0, None));
    let mut expansions = 0;

    while let Some(Reverse((_, cost, order))) = open.pop() {
        let cell = queued_cells[order as usize];
        if reached[&cell].0 < cost {
            continue;
        }
        if is_goal(cell) {
            return Some(path_to(&reached, cell));
        }
        if expansions == budget {
            return None;
        }
        expansions += 1;
        for next in neighbours(grid, cell) {
            let next_cost = cost + 1;
            if reached
                .get(&next)
                .is_some_and(|&(known, _)| known <= next_cost)
            {
                continue;
            }
            reached.insert(next, (next_cost, Some(cell)));
            let order = u32::try_from(queued_cells.len())
                .expect("the node budget keeps the queue far below u32::MAX entries");
            queued_cells.push(next);
            open.push(Reverse((
                next_cost + manhattan(next, toward),
                next_cost,
                order,
            )));
        }
    }
    None
}

/// The cells from the one after the start up to `end`.
fn path_to(reached: &HashMap<BlockPos, (i32, Option<BlockPos>)>, end: BlockPos) -> Vec<BlockPos> {
    let mut path = Vec::new();
    let mut cell = end;
    while let Some(from) = reached[&cell].1 {
        path.push(cell);
        cell = from;
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(BlockPos::new(x, y, z), 1, None);
    }

    /// A floor of blocks at y = 0 spanning `0..width` by `0..depth`, so feet cells are y = 1.
    fn floor(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    fn goal_at(target: BlockPos) -> impl Fn(BlockPos) -> bool {
        move |cell| cell == target
    }

    fn cells(list: &[(i32, i32, i32)]) -> Vec<BlockPos> {
        list.iter()
            .map(|&(x, y, z)| BlockPos::new(x, y, z))
            .collect()
    }

    fn run(
        grid: &VoxelGrid,
        start: (i32, i32, i32),
        goal: (i32, i32, i32),
    ) -> Option<Vec<BlockPos>> {
        let goal = BlockPos::new(goal.0, goal.1, goal.2);
        route(
            grid,
            BlockPos::new(start.0, start.1, start.2),
            goal_at(goal),
            goal,
            ROUTE_NODE_BUDGET,
        )
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = floor(5, 5);
        place(&mut grid, 1, 1, 1);
        place(&mut grid, 3, 3, 3);
        place(&mut grid, 0, 2, 4);
        place(&mut grid, 4, 4, 4);

        let open = standable(&grid, BlockPos::new(2, 1, 2));
        let air_below = standable(&grid, BlockPos::new(2, 2, 2));
        let inside_ground = standable(&grid, BlockPos::new(2, 0, 2));
        let on_a_block = standable(&grid, BlockPos::new(1, 2, 1));
        let inside_a_block = standable(&grid, BlockPos::new(1, 1, 1));
        let low_ceiling = standable(&grid, BlockPos::new(3, 1, 3));
        let ceiling_above_the_capsule = standable(&grid, BlockPos::new(4, 1, 4));
        let block_at_the_chest = standable(&grid, BlockPos::new(0, 1, 4));

        assert!(open, "air from the feet up over solid ground");
        assert!(!block_at_the_chest, "a block one cell above the feet");
        assert!(!air_below, "no ground under the feet");
        assert!(!inside_ground, "the feet cell must be air");
        assert!(on_a_block, "a placed block is ground too");
        assert!(!inside_a_block, "the feet cell must be air");
        assert!(
            !low_ceiling,
            "a block two cells above the feet touches the head"
        );
        assert!(
            ceiling_above_the_capsule,
            "three cells of air is enough; a block above that is not in the way"
        );
        assert!(
            !standable(&grid, BlockPos::new(2, 1, 9)),
            "off the floor there is nothing to stand on"
        );
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = floor(8, 3);

        let found = run(&grid, (0, 1, 0), (4, 1, 0));

        assert_eq!(
            found,
            Some(cells(&[(1, 1, 0), (2, 1, 0), (3, 1, 0), (4, 1, 0)]))
        );
    }

    #[test]
    fn a_route_from_the_goal_is_empty() {
        let grid = floor(4, 4);

        let found = run(&grid, (2, 1, 2), (2, 1, 2));

        assert_eq!(found, Some(Vec::new()));
    }

    /// A floor three wide with a one-block rise across x = 2..=4.
    fn rise() -> VoxelGrid {
        let mut grid = floor(8, 3);
        for x in 2..=4 {
            for z in 0..3 {
                place(&mut grid, x, 1, z);
            }
        }
        grid
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let grid = rise();

        let found = run(&grid, (0, 1, 0), (6, 1, 0));

        assert_eq!(
            found,
            Some(cells(&[
                (1, 1, 0),
                (2, 2, 0),
                (3, 2, 0),
                (4, 2, 0),
                (5, 1, 0),
                (6, 1, 0),
            ]))
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = floor(8, 3);
        for z in 0..3 {
            place(&mut grid, 3, 1, z);
        }
        assert!(
            run(&grid, (0, 1, 1), (6, 1, 1)).is_some(),
            "a one-block wall is climbed"
        );
        for z in 0..3 {
            place(&mut grid, 3, 2, z);
        }

        let found = run(&grid, (0, 1, 1), (6, 1, 1));

        assert_eq!(found, None);
    }

    #[test]
    fn a_start_inside_the_ground_or_one_above_standable_still_routes() {
        let grid = floor(8, 3);

        let from_the_ground = run(&grid, (2, 0, 1), (5, 1, 1));
        let from_the_air = run(&grid, (2, 2, 1), (5, 1, 1));

        for (found, why) in [
            (from_the_ground, "feet floored into the ground"),
            (from_the_air, "one cell above a standable cell"),
        ] {
            let path = found.unwrap_or_else(|| panic!("{why}: no route"));
            assert_eq!(path.last(), Some(&BlockPos::new(5, 1, 1)), "{why}");
        }
    }

    #[test]
    fn a_drop_of_two_blocks_is_not_taken() {
        // A platform two blocks high at x = 0..=2 whose only way down is off its east edge.
        let mut grid = floor(8, 3);
        for x in 0..=2 {
            for z in 0..3 {
                place(&mut grid, x, 1, z);
                place(&mut grid, x, 2, z);
            }
        }

        let two_down = run(&grid, (0, 3, 1), (6, 1, 1));

        assert_eq!(two_down, None, "the ledge is two blocks above the floor");
        for z in 0..3 {
            place(&mut grid, 3, 1, z);
        }

        let with_a_step = run(&grid, (0, 3, 1), (6, 1, 1));

        assert!(with_a_step.is_some(), "two drops of one block each");
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = rise();
        assert!(
            run(&grid, (0, 1, 0), (6, 1, 0)).is_some(),
            "the rise is passable while the air above the foot of it is open"
        );
        for z in 0..3 {
            place(&mut grid, 1, 4, z);
        }

        let found = run(&grid, (0, 1, 0), (6, 1, 0));

        assert_eq!(found, None, "a fourth cell of air is needed to step up");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = floor(9, 9);
        assert!(run(&grid, (0, 1, 0), (4, 1, 4)).is_some());
        for x in 3..=5 {
            for z in 3..=5 {
                if (x, z) != (4, 4) {
                    place(&mut grid, x, 1, z);
                    place(&mut grid, x, 2, z);
                }
            }
        }

        let found = run(&grid, (0, 1, 0), (4, 1, 4));

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = floor(100, 100);
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(99, 1, 0);
        // A heuristic that pulls away from the goal makes the search cover the floor.
        let misleading = BlockPos::new(0, 1, 99);

        let within_budget = route(&grid, start, goal_at(goal), misleading, ROUTE_NODE_BUDGET);
        let unlimited = route(&grid, start, goal_at(goal), misleading, 100_000);
        let tiny = route(
            &grid,
            BlockPos::new(0, 1, 0),
            goal_at(BlockPos::new(6, 1, 0)),
            BlockPos::new(6, 1, 0),
            3,
        );
        let enough = route(
            &grid,
            BlockPos::new(0, 1, 0),
            goal_at(BlockPos::new(6, 1, 0)),
            BlockPos::new(6, 1, 0),
            50,
        );

        assert_eq!(
            within_budget, None,
            "4096 expansions do not cover the floor"
        );
        assert_eq!(
            unlimited.map(|r| r.len()),
            Some(99),
            "the goal is reachable"
        );
        assert_eq!(tiny, None, "three expansions cannot reach six cells away");
        assert_eq!(enough.map(|r| r.len()), Some(6));
    }

    /// Columns of height 1 or 2 over a 6 by 6 board: neighbours never differ by more than one.
    fn terrain(heights: &[bool]) -> (VoxelGrid, Vec<i32>) {
        let mut grid = VoxelGrid::new(16);
        let mut tops = Vec::new();
        for (i, &high) in heights.iter().enumerate() {
            let (x, z) = ((i % 6) as i32, (i / 6) as i32);
            let top = if high { 2 } else { 1 };
            for y in 0..top {
                place(&mut grid, x, y, z);
            }
            tops.push(top);
        }
        (grid, tops)
    }

    /// Columns of height 1 to 4 over a 6 by 6 board, so walls and steps make detours.
    fn rough_terrain(heights: &[u8]) -> (VoxelGrid, Vec<i32>) {
        let mut grid = VoxelGrid::new(16);
        let mut tops = Vec::new();
        for (i, &top) in heights.iter().enumerate() {
            let (x, z) = ((i % 6) as i32, (i / 6) as i32);
            for y in 0..i32::from(top) {
                place(&mut grid, x, y, z);
            }
            tops.push(i32::from(top));
        }
        (grid, tops)
    }

    /// The fewest steps from `start` to a cell satisfying `is_goal`, by plain breadth-first
    /// search over the documented move rule.
    fn shortest_route_len(
        grid: &VoxelGrid,
        start: BlockPos,
        is_goal: impl Fn(BlockPos) -> bool,
    ) -> Option<usize> {
        let mut seen = std::collections::HashSet::from([start]);
        let mut frontier = vec![start];
        let mut steps = 0;
        while !frontier.is_empty() {
            if frontier.iter().any(|&cell| is_goal(cell)) {
                return Some(steps);
            }
            let mut next = Vec::new();
            for cell in frontier {
                let can_climb = !grid.is_solid_at(cell + BlockPos::Y * 3);
                for step in [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z] {
                    let level = cell + step;
                    let options = [
                        (level, true),
                        (level + BlockPos::Y, can_climb),
                        (level - BlockPos::Y, true),
                    ];
                    for (candidate, allowed) in options {
                        if allowed && standable(grid, candidate) && seen.insert(candidate) {
                            next.push(candidate);
                        }
                    }
                }
            }
            frontier = next;
            steps += 1;
        }
        None
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn a_route_to_a_goal_set_is_as_short_as_any(
            heights in proptest::collection::vec(1u8..=4, 36),
            from in (0..6i32, 0..6i32),
            to in (0..6i32, 0..6i32),
            depth in 0..4i32,
        ) {
            let (grid, tops) = rough_terrain(&heights);
            let top = |(x, z): (i32, i32)| tops[(z * 6 + x) as usize];
            let start = BlockPos::new(from.0, top(from), from.1);
            let block = BlockPos::new(to.0, (top(to) - 1 - depth).max(0), to.1);
            let is_goal = |cell: BlockPos| crate::jobs::is_stand_cell(block, cell);

            let found = route(&grid, start, is_goal, block, ROUTE_NODE_BUDGET);

            let shortest = shortest_route_len(&grid, start, is_goal);
            prop_assert_eq!(found.map(|path| path.len()), shortest, "block {}, start {}", block, start);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(any::<bool>(), 36),
            from in (0..6i32, 0..6i32),
            to in (0..6i32, 0..6i32),
        ) {
            let (grid, tops) = terrain(&heights);
            let top = |(x, z): (i32, i32)| tops[(z * 6 + x) as usize];
            let start = BlockPos::new(from.0, top(from), from.1);
            let goal = BlockPos::new(to.0, top(to), to.1);

            let found = route(
                &grid,
                start,
                |cell| cell.x == goal.x && cell.z == goal.z,
                goal,
                ROUTE_NODE_BUDGET,
            );

            let path = found.expect("every column is within a step of its neighbours");
            let mut previous = start;
            for cell in &path {
                prop_assert!(standable(&grid, *cell), "{cell} is not standable");
                let step = *cell - previous;
                prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "{} to {}", previous, cell);
                prop_assert!(step.y.abs() <= 1, "{} to {}", previous, cell);
                previous = *cell;
            }
            prop_assert_eq!((previous.x, previous.z), (goal.x, goal.z));
            prop_assert!(path.len() as i32 >= (to.0 - from.0).abs() + (to.1 - from.1).abs());
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(any::<bool>(), 36),
            from in (0..6i32, 0..6i32),
            to in (0..6i32, 0..6i32),
        ) {
            let (first, tops) = terrain(&heights);
            let (second, _) = terrain(&heights);
            let top = |(x, z): (i32, i32)| tops[(z * 6 + x) as usize];
            let start = BlockPos::new(from.0, top(from), from.1);
            let goal = BlockPos::new(to.0, top(to), to.1);
            let search = |grid: &VoxelGrid| {
                route(grid, start, |c| c.x == goal.x && c.z == goal.z, goal, ROUTE_NODE_BUDGET)
            };

            let a = search(&first);
            let b = search(&second);

            prop_assert!(a.is_some(), "a route exists");
            prop_assert_eq!(a, b);
        }
    }
}
