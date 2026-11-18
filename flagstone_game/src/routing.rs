//! Walking routes over the voxel grid: which cells a pawn can stand in and how it gets between them.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Cells of air a standing pawn needs from its feet up: the 2.3 m capsule.
const CAPSULE_CELLS: i32 = 3;

const HORIZONTAL: [BlockPos; 4] = [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z];

/// The most cells one search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn's feet can be in `cell`: ground below and air for the capsule's height.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..CAPSULE_CELLS).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell holding a pawn's feet at `feet`.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// The cheapest walkable route from `start` to the first cell `is_goal` accepts, both ends
/// included, or `None` when there is none within `budget` expansions.
///
/// A start that is not standable is moved one cell up, then one down, before giving up. The
/// search is A* with a Manhattan heuristic toward `toward`; ties break on cost so far, then
/// insertion order, so the same grid always gives the same route.
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
    let heuristic = |cell: BlockPos| {
        let gap = (cell - toward).abs();
        gap.x + gap.y + gap.z
    };

    let mut best: BTreeMap<[i32; 3], (i32, [i32; 3])> = BTreeMap::new();
    let mut open = BinaryHeap::new();
    let mut inserted = 0u64;
    best.insert(start.to_array(), (0, start.to_array()));
    open.push(Reverse((heuristic(start), 0, inserted, start.to_array())));

    let mut expansions = 0;
    while let Some(Reverse((_, cost, _, key))) = open.pop() {
        let cell = BlockPos::from_array(key);
        if best.get(&key).is_some_and(|&(known, _)| cost > known) {
            continue;
        }
        if is_goal(cell) {
            return Some(walk_back(&best, key));
        }
        expansions += 1;
        if expansions > budget {
            return None;
        }
        for next in neighbours(grid, cell) {
            let next_key = next.to_array();
            let next_cost = cost + 1;
            if best
                .get(&next_key)
                .is_some_and(|&(known, _)| known <= next_cost)
            {
                continue;
            }
            best.insert(next_key, (next_cost, key));
            inserted += 1;
            open.push(Reverse((
                next_cost + heuristic(next),
                next_cost,
                inserted,
                next_key,
            )));
        }
    }
    None
}

/// The standable cells one step from `cell`: level, one up or one down. A step up needs air
/// one cell above the capsule at `cell`; a step down needs air at the capsule's old top in
/// the destination column, so the body clears an overhang there.
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    let headroom = !grid.is_solid_at(cell + BlockPos::Y * CAPSULE_CELLS);
    HORIZONTAL.into_iter().flat_map(move |side| {
        let level = cell + side;
        let drop_clear = !grid.is_solid_at(level + BlockPos::Y * (CAPSULE_CELLS - 1));
        [level, level + BlockPos::Y, level - BlockPos::Y]
            .into_iter()
            .filter(move |&next| {
                let allowed = match next.y.cmp(&cell.y) {
                    std::cmp::Ordering::Greater => headroom,
                    std::cmp::Ordering::Less => drop_clear,
                    std::cmp::Ordering::Equal => true,
                };
                allowed && standable(grid, next)
            })
    })
}

fn walk_back(best: &BTreeMap<[i32; 3], (i32, [i32; 3])>, goal: [i32; 3]) -> Vec<BlockPos> {
    let mut path = vec![BlockPos::from_array(goal)];
    let mut key = goal;
    while let Some(&(_, parent)) = best.get(&key) {
        if parent == key {
            break;
        }
        path.push(BlockPos::from_array(parent));
        key = parent;
    }
    path.reverse();
    path
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
    fn stepping_down_needs_headroom_over_the_lower_cell() {
        let mut grid = floor(7, 3);
        for x in 0..=2 {
            for z in 0..3 {
                place(&mut grid, x, 1, z);
            }
        }
        let upper = BlockPos::new(2, 2, 1);
        let lower = BlockPos::new(3, 1, 1);
        assert!(standable(&grid, upper) && standable(&grid, lower));
        assert!(
            find(&grid, upper, lower).is_some(),
            "an open drop can be taken"
        );
        for z in 0..3 {
            place(&mut grid, 3, 4, z);
        }
        assert!(standable(&grid, lower), "the overhang is above the capsule");

        let found = find(&grid, upper, lower);

        assert_eq!(found, None, "the capsule's top cannot pass the overhang");
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
    fn the_budget_counts_expansions_exactly() {
        let length = 10;
        let grid = floor(length, 1);
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(length - 1, 1, 0);
        let with_budget = |budget: usize| route(&grid, start, |cell| cell == goal, goal, budget);

        let smallest = (0..100)
            .find(|&budget| with_budget(budget).is_some())
            .expect("some budget reaches the end of the corridor");

        assert_eq!(
            smallest,
            usize::try_from(length - 1).expect("positive"),
            "every cell but the goal is expanded once"
        );
        assert_eq!(with_budget(smallest - 1), None);
        assert!(with_budget(smallest).is_some());
    }

    /// The smallest budget with which `route` finds `goal` from `start`.
    fn expansions_needed(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> usize {
        (0..4096)
            .find(|&budget| route(grid, start, |cell| cell == goal, goal, budget).is_some())
            .expect("the goal is reachable")
    }

    #[test]
    fn the_heuristic_keeps_the_search_on_the_way_to_the_goal() {
        let grid = floor(12, 12);
        let along_x = expansions_needed(&grid, BlockPos::new(1, 1, 5), BlockPos::new(10, 1, 5));
        let along_z = expansions_needed(&grid, BlockPos::new(5, 1, 1), BlockPos::new(5, 1, 10));
        let diagonal = expansions_needed(&grid, BlockPos::new(2, 1, 2), BlockPos::new(8, 1, 6));

        assert_eq!(
            (along_x, along_z),
            (9, 9),
            "a straight line expands each cell once"
        );
        assert_eq!(
            diagonal, 34,
            "every cell of the 7 by 5 box between start and goal but the goal, none outside it"
        );
    }

    #[test]
    fn the_heuristic_counts_the_climb_too() {
        let mut grid = floor(12, 12);
        for x in 6..12 {
            for z in 0..12 {
                place(&mut grid, x, 1, z);
            }
        }
        let start = BlockPos::new(1, 1, 5);
        let goal = BlockPos::new(10, 2, 7);

        let expansions = expansions_needed(&grid, start, goal);

        assert_eq!(
            expansions, 24,
            "the search is guided by the height gap as well as the horizontal one"
        );
    }

    #[test]
    fn equal_routes_break_ties_by_insertion_order() {
        let grid = floor(8, 8);
        let start = BlockPos::new(3, 1, 3);
        let goal = BlockPos::new(5, 1, 5);

        let found = find(&grid, start, goal);

        assert_eq!(
            found,
            Some(cells(&[
                (3, 1, 3),
                (4, 1, 3),
                (5, 1, 3),
                (5, 1, 4),
                (5, 1, 5)
            ])),
            "+X is generated before +Z, so the first shortest route found goes along x"
        );
    }

    /// The length of the shortest walk from `start` to `goal` by the movement rules, found by
    /// breadth first search.
    fn shortest_walk(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<usize> {
        let mut seen = std::collections::HashSet::from([start]);
        let mut frontier = std::collections::VecDeque::from([(start, 1usize)]);
        while let Some((cell, length)) = frontier.pop_front() {
            if cell == goal {
                return Some(length);
            }
            for side in [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z] {
                let level = cell + side;
                let up = level + BlockPos::Y;
                let down = level - BlockPos::Y;
                let can_go_up = !grid.is_solid_at(cell + BlockPos::Y * 3);
                let can_go_down = !grid.is_solid_at(level + BlockPos::Y * 2);
                for (next, allowed) in [(level, true), (up, can_go_up), (down, can_go_down)] {
                    if allowed && standable(grid, next) && seen.insert(next) {
                        frontier.push_back((next, length + 1));
                    }
                }
            }
        }
        None
    }

    proptest! {
        /// Heights run one to three, so some neighbours differ by two and wall each other off.
        #[test]
        fn a_route_is_as_short_as_a_breadth_first_search(
            heights in proptest::collection::vec(1i32..=3, 36),
            sx in 0..6, sz in 0..6, gx in 0..6, gz in 0..6,
            shift in (-10i32..10, -3i32..3, -10i32..10),
        ) {
            let shift = BlockPos::new(shift.0, shift.1, shift.2);
            let mut grid = VoxelGrid::new(16);
            for x in 0..6 {
                for z in 0..6 {
                    let top = heights[usize::try_from(x * 6 + z).expect("small index")];
                    for y in 0..top {
                        grid.mutator().place(BlockPos::new(x, y, z) + shift, 1, None);
                    }
                }
            }
            let start = surface(&heights, sx, sz) + shift;
            let goal = surface(&heights, gx, gz) + shift;

            let found = find(&grid, start, goal);

            prop_assert_eq!(found.map(|path| path.len()), shortest_walk(&grid, start, goal));
        }
    }

    #[test]
    fn a_start_off_the_surface_is_moved_one_up_or_down() {
        let grid = floor(6, 3);
        let goal = BlockPos::new(4, 1, 1);
        let sunk = BlockPos::new(1, 0, 1);
        let floating = BlockPos::new(1, 2, 1);

        let from_below = find(&grid, sunk, goal);
        let from_above = find(&grid, floating, goal);

        for (found, why) in [
            (
                from_below,
                "a start one below the surface is tried one above",
            ),
            (
                from_above,
                "a start one above the surface is tried one below",
            ),
        ] {
            let path = found.expect(why);
            assert_eq!(path.last(), Some(&goal));
            assert!(path.iter().all(|&cell| standable(&grid, cell)));
        }
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
