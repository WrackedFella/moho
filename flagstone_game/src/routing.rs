//! Where a pawn can stand and how it gets from one standable cell to another.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Node expansions one search may spend before it gives up with `None`.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn's feet can be in `cell`: ground below (`cell - Y` solid) and three cells
/// of air from the feet up (the capsule is 2.3 m tall).
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..3).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell containing `feet`, flooring each axis.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// A* over standable cells from `start` to the nearest cell satisfying `is_goal`.
///
/// Moves go to the four horizontal neighbours, up or down at most one block; a step up
/// needs a fourth air cell above the current feet. The Manhattan distance to `toward`
/// guides the search; ties break on (f, g, insertion order). A start that is not
/// standable tries the cell one above and one below. The route runs from the start cell
/// to the goal cell, both included. `None` when no goal is reachable within `budget`
/// node expansions.
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
    let heuristic = |cell: BlockPos| (toward - cell).abs().element_sum();

    // `nodes[i]` is a cell and the index of the node it was reached from.
    let mut nodes = vec![(start, usize::MAX)];
    let mut best_g: HashMap<BlockPos, i32> = HashMap::from([(start, 0)]);
    let mut closed: HashSet<BlockPos> = HashSet::new();
    let mut open = BinaryHeap::new();
    open.push(Reverse((heuristic(start), 0, 0usize)));
    let mut expansions = 0;

    while let Some(Reverse((_, g, index))) = open.pop() {
        let cell = nodes[index].0;
        if !closed.insert(cell) {
            continue;
        }
        if is_goal(cell) {
            let mut path = vec![cell];
            let mut at = index;
            while nodes[at].1 != usize::MAX {
                at = nodes[at].1;
                path.push(nodes[at].0);
            }
            path.reverse();
            return Some(path);
        }
        expansions += 1;
        if expansions > budget {
            return None;
        }
        for next in neighbours(grid, cell) {
            let next_g = g + 1;
            if closed.contains(&next) || best_g.get(&next).is_some_and(|&known| known <= next_g) {
                continue;
            }
            best_g.insert(next, next_g);
            nodes.push((next, index));
            open.push(Reverse((next_g + heuristic(next), next_g, nodes.len() - 1)));
        }
    }
    None
}

/// The cells a pawn standing in `cell` can step to, in a fixed order.
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> Vec<BlockPos> {
    let can_climb = !grid.is_solid_at(cell + BlockPos::Y * 3);
    let mut found = Vec::new();
    for side in [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z] {
        let level = cell + side;
        if can_climb && standable(grid, level + BlockPos::Y) {
            found.push(level + BlockPos::Y);
        }
        if standable(grid, level) {
            found.push(level);
        }
        if standable(grid, level - BlockPos::Y) {
            found.push(level - BlockPos::Y);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const GROUND: i32 = 0;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// A one-block-wide strip of ground along x at z = 0, top block at y = 0.
    fn strip(len: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..len {
            grid.mutator().place(cell(x, GROUND, 0), 1, None);
        }
        grid
    }

    fn plane(size: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..size {
            for z in 0..size {
                grid.mutator().place(cell(x, GROUND, z), 1, None);
            }
        }
        grid
    }

    fn to(goal: BlockPos) -> impl Fn(BlockPos) -> bool {
        move |c| c == goal
    }

    fn find(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, to(goal), goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = plane(4);

        assert!(standable(&grid, cell(1, 1, 1)), "ground below, air above");
        assert!(!standable(&grid, cell(1, 2, 1)), "air below");
        assert!(!standable(&grid, cell(1, 0, 1)), "inside the ground");
        assert!(!standable(&grid, cell(9, 1, 9)), "no ground at all");

        grid.mutator().place(cell(1, 3, 1), 1, None);
        assert!(!standable(&grid, cell(1, 1, 1)), "a block at the head");
        assert!(standable(&grid, cell(2, 1, 1)), "the neighbour is clear");

        let mut grid = plane(4);
        grid.mutator().place(cell(1, 2, 1), 1, None);
        assert!(!standable(&grid, cell(1, 1, 1)), "a block at the chest");

        let mut grid = plane(4);
        grid.mutator().place(cell(1, 4, 1), 1, None);
        assert!(
            standable(&grid, cell(1, 1, 1)),
            "the fourth cell may be solid"
        );
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        assert_eq!(feet_cell(Vec3::new(2.9, 1.0, 0.1)), cell(2, 1, 0));
        assert_eq!(feet_cell(Vec3::new(-0.1, 0.99, -2.5)), cell(-1, 0, -3));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = strip(8);

        let found = find(&grid, cell(0, 1, 0), cell(6, 1, 0));

        let expected: Vec<BlockPos> = (0..=6).map(|x| cell(x, 1, 0)).collect();
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn a_start_that_is_already_a_goal_routes_to_itself() {
        let grid = strip(4);

        let found = find(&grid, cell(2, 1, 0), cell(2, 1, 0));

        assert_eq!(found, Some(vec![cell(2, 1, 0)]));
    }

    fn strip_with_a_rise() -> VoxelGrid {
        let mut grid = strip(9);
        for x in 4..9 {
            grid.mutator().place(cell(x, 1, 0), 1, None);
        }
        grid
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let grid = strip_with_a_rise();

        let up = find(&grid, cell(0, 1, 0), cell(8, 2, 0));
        let down = find(&grid, cell(8, 2, 0), cell(0, 1, 0));

        let mut expected: Vec<BlockPos> = (0..4).map(|x| cell(x, 1, 0)).collect();
        expected.extend((4..9).map(|x| cell(x, 2, 0)));
        assert_eq!(up, Some(expected.clone()));
        expected.reverse();
        assert_eq!(down, Some(expected));
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = strip(9);
        grid.mutator().place(cell(4, 1, 0), 1, None);
        grid.mutator().place(cell(4, 2, 0), 1, None);

        let found = find(&grid, cell(0, 1, 0), cell(8, 1, 0));

        assert_eq!(found, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = strip_with_a_rise();
        assert!(
            find(&grid, cell(0, 1, 0), cell(8, 2, 0)).is_some(),
            "open air: the climb works"
        );
        grid.mutator().place(cell(3, 4, 0), 1, None);

        let found = find(&grid, cell(0, 1, 0), cell(8, 2, 0));

        assert!(standable(&grid, cell(3, 1, 0)), "walking under it is fine");
        assert_eq!(found, None, "no fourth air cell above the feet to climb");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = plane(11);
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            for y in 1..=2 {
                grid.mutator().place(cell(5 + dx, y, 5 + dz), 1, None);
            }
        }

        let found = find(&grid, cell(0, 1, 0), cell(5, 1, 5));

        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = strip(40);
        let goal = cell(30, 1, 0);

        let starved = route(&grid, cell(0, 1, 0), to(goal), goal, 10);
        let funded = route(&grid, cell(0, 1, 0), to(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None);
        assert_eq!(funded.map(|r| r.len()), Some(31));
    }

    #[test]
    fn the_heuristic_guides_the_search_toward_the_goal_within_a_tight_budget() {
        let grid = plane(24);
        let goal = cell(20, 1, 2);

        let found = route(&grid, cell(0, 1, 0), to(goal), goal, 100);

        assert_eq!(found.map(|r| r.len()), Some(23));
    }

    #[test]
    fn a_start_one_above_or_below_the_ground_is_corrected() {
        let grid = strip(6);
        let goal = cell(5, 1, 0);

        let from_above = find(&grid, cell(0, 2, 0), goal).expect("one above is tried");
        let mut sunk = strip(6);
        sunk.mutator().place(cell(0, 1, 0), 1, None);
        let from_below = find(&sunk, cell(0, 1, 0), goal).expect("one above is tried");

        assert_eq!(from_above.last(), Some(&goal));
        assert_eq!(from_above.first(), Some(&cell(0, 1, 0)));
        assert_eq!(from_below.last(), Some(&goal));
        assert!(standable(&sunk, from_below[0]));
    }

    #[test]
    fn a_start_two_cells_off_the_ground_has_no_route() {
        let grid = strip(6);

        let found = find(&grid, cell(0, 3, 0), cell(5, 1, 0));

        assert_eq!(found, None);
    }

    /// Columns of solid blocks from y = 0 up to the given height, on an 8 x 8 footprint.
    fn terrain(heights: &[i32], reversed: bool) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        let mut columns: Vec<(i32, i32, i32)> = heights
            .iter()
            .enumerate()
            .map(|(i, h)| ((i % 8) as i32, (i / 8) as i32, *h))
            .collect();
        if reversed {
            columns.reverse();
        }
        for (x, z, h) in columns {
            for y in 0..=h {
                grid.mutator().place(cell(x, y, z), 1, None);
            }
        }
        grid
    }

    fn top(heights: &[i32], x: i32, z: i32) -> BlockPos {
        cell(x, heights[(z * 8 + x) as usize] + 1, z)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(0i32..2, 64),
            (sx, sz, gx, gz) in (0i32..8, 0i32..8, 0i32..8, 0i32..8),
        ) {
            let grid = terrain(&heights, false);
            let start = top(&heights, sx, sz);
            let goal = top(&heights, gx, gz);

            let found = find(&grid, start, goal);

            prop_assert!(found.is_some(), "steps of at most one block always have a route");
            let found = found.unwrap();
            prop_assert_eq!(found.first(), Some(&start));
            prop_assert_eq!(found.last(), Some(&goal));
            for c in &found {
                prop_assert!(standable(&grid, *c), "{:?} is not standable", c);
            }
            for pair in found.windows(2) {
                let step = pair[1] - pair[0];
                prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "one horizontal step");
                prop_assert!(step.y.abs() <= 1, "at most one block up or down");
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(0i32..4, 64),
            (sx, sz, gx, gz) in (0i32..8, 0i32..8, 0i32..8, 0i32..8),
        ) {
            let forward = terrain(&heights, false);
            let backward = terrain(&heights, true);
            let start = top(&heights, sx, sz);
            let goal = top(&heights, gx, gz);

            let first = find(&forward, start, goal);
            let again = find(&forward, start, goal);
            let rebuilt = find(&backward, start, goal);

            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &rebuilt);
        }
    }
}
