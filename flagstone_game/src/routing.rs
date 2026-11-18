//! Routes over the voxel ground: which cells a pawn can stand on and the way between them.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Cells a search may expand before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground below and three cells of air
/// from the feet up (the capsule is 2.3 m tall).
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..CLEARANCE).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell holding `feet`, flooring each axis.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// Cells of air a pawn needs from its feet up.
const CLEARANCE: i32 = 3;

/// The four horizontal neighbours, in the fixed order the search tries them.
const HORIZONTAL: [BlockPos; 4] = [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z];

/// The standable cells one step from `from`: level, one block up (with a fourth air cell
/// above `from` to rise into) or one block down.
fn neighbours(grid: &VoxelGrid, from: BlockPos) -> impl Iterator<Item = BlockPos> {
    let can_rise = !grid.is_solid_at(from + BlockPos::Y * CLEARANCE);
    HORIZONTAL.into_iter().filter_map(move |d| {
        let level = from + d;
        let up = level + BlockPos::Y;
        let down = level - BlockPos::Y;
        if standable(grid, level) {
            Some(level)
        } else if can_rise && standable(grid, up) {
            Some(up)
        } else if standable(grid, down) {
            Some(down)
        } else {
            None
        }
    })
}

/// Ground-plane Manhattan distance; vertical steps cost the same as level ones, so height
/// stays out of the estimate to keep it admissible.
fn estimate(from: BlockPos, toward: BlockPos) -> u32 {
    from.x.abs_diff(toward.x) + from.z.abs_diff(toward.z)
}

/// The cells to walk from `start` to the nearest cell satisfying `is_goal`, excluding
/// `start` and ending on the goal cell; empty when `start` is itself a goal. `None` when
/// no goal is reachable within `budget` expansions. `toward` steers the heuristic.
///
/// A `start` that is not standable falls back to the cell one above, then one below, for a
/// pawn standing on a slope; `None` if neither stands either.
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

    // Nodes in insertion order: (cell, index of the node it was reached from). The heap
    // orders by (f, g, insertion index), so the search never depends on hash order.
    let mut nodes: Vec<(BlockPos, Option<usize>)> = vec![(start, None)];
    let mut open = BinaryHeap::new();
    open.push(Reverse((estimate(start, toward), 0u32, 0usize)));
    let mut best: HashMap<BlockPos, u32> = HashMap::from([(start, 0)]);
    let mut closed: HashSet<BlockPos> = HashSet::new();
    let mut expanded = 0usize;

    while let Some(Reverse((_, g, index))) = open.pop() {
        let (cell, _) = nodes[index];
        if !closed.insert(cell) {
            continue;
        }
        if is_goal(cell) {
            let mut cells = Vec::new();
            let mut at = index;
            while let (cell, Some(parent)) = nodes[at] {
                cells.push(cell);
                at = parent;
            }
            cells.reverse();
            return Some(cells);
        }
        if expanded == budget {
            return None;
        }
        expanded += 1;

        for next in neighbours(grid, cell) {
            let cost = g + 1;
            if closed.contains(&next) || best.get(&next).is_some_and(|&known| known <= cost) {
                continue;
            }
            best.insert(next, cost);
            nodes.push((next, Some(index)));
            open.push(Reverse((
                cost + estimate(next, toward),
                cost,
                nodes.len() - 1,
            )));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const GROUND: u32 = 1;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn fill(grid: &mut VoxelGrid, min: [i32; 3], max: [i32; 3]) {
        grid.mutator()
            .fill_region(BlockPos::from_array(min), BlockPos::from_array(max), GROUND);
    }

    /// Ground one block thick (y = 0) over the given x and z ranges: feet cells sit at y = 1.
    fn flat(x: i32, z: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        fill(&mut grid, [0, 0, 0], [x, 0, z]);
        grid
    }

    fn find(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = VoxelGrid::new(16);
        grid.mutator().place(cell(0, 0, 0), GROUND, None);

        assert!(standable(&grid, cell(0, 1, 0)), "ground below, air above");
        assert!(!standable(&grid, cell(0, 2, 0)), "nothing below");
        assert!(!standable(&grid, cell(0, 0, 0)), "inside a solid block");
        assert!(!standable(&grid, cell(1, 1, 0)), "no ground under it");

        grid.mutator().place(cell(0, 4, 0), GROUND, None);
        assert!(
            standable(&grid, cell(0, 1, 0)),
            "a ceiling above three cells of air"
        );

        grid.mutator().place(cell(0, 3, 0), GROUND, None);
        assert!(
            !standable(&grid, cell(0, 1, 0)),
            "a ceiling two cells above the feet"
        );

        let blocked = |at: BlockPos| {
            let mut grid = VoxelGrid::new(16);
            grid.mutator().place(cell(0, 0, 0), GROUND, None);
            grid.mutator().place(at, GROUND, None);
            grid
        };
        assert!(
            !standable(&blocked(cell(0, 1, 0)), cell(0, 1, 0)),
            "a solid block in the feet cell"
        );
        assert!(
            !standable(&blocked(cell(0, 2, 0)), cell(0, 1, 0)),
            "a block at feet + 1"
        );
        assert!(
            !standable(&blocked(cell(0, 3, 0)), cell(0, 1, 0)),
            "a block at feet + 2"
        );
    }

    #[test]
    fn a_start_off_the_standable_cells_tries_one_above_and_one_below() {
        let grid = flat(8, 2);
        let goal = cell(4, 1, 0);

        let from_below = find(&grid, cell(0, 0, 0), goal);
        let from_above = find(&grid, cell(0, 2, 0), goal);

        assert!(
            from_below.is_some_and(|r| r.last() == Some(&goal)),
            "a start one cell below a standable cell"
        );
        assert!(
            from_above.is_some_and(|r| r.last() == Some(&goal)),
            "a start one cell above a standable cell"
        );
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        assert_eq!(feet_cell(Vec3::new(1.9, 2.0, -0.1)), cell(1, 2, -1));
        assert_eq!(feet_cell(Vec3::new(0.5, 1.0, 0.5)), cell(0, 1, 0));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = flat(8, 2);

        let found = find(&grid, cell(0, 1, 1), cell(4, 1, 1));

        assert_eq!(
            found,
            Some(vec![
                cell(1, 1, 1),
                cell(2, 1, 1),
                cell(3, 1, 1),
                cell(4, 1, 1)
            ])
        );
    }

    #[test]
    fn a_route_from_a_goal_cell_is_empty() {
        let grid = flat(8, 2);

        let found = find(&grid, cell(2, 1, 1), cell(2, 1, 1));

        assert_eq!(found, Some(vec![]));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = flat(6, 2);
        fill(&mut grid, [3, 1, 0], [4, 1, 2]);

        let found = find(&grid, cell(0, 1, 1), cell(6, 1, 1));

        assert_eq!(
            found,
            Some(vec![
                cell(1, 1, 1),
                cell(2, 1, 1),
                cell(3, 2, 1),
                cell(4, 2, 1),
                cell(5, 1, 1),
                cell(6, 1, 1),
            ])
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = flat(6, 2);
        fill(&mut grid, [3, 1, 0], [3, 2, 2]);

        let found = find(&grid, cell(0, 1, 1), cell(6, 1, 1));

        assert_eq!(found, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = flat(6, 2);
        fill(&mut grid, [3, 1, 0], [3, 1, 2]);
        let open = find(&grid, cell(0, 1, 1), cell(6, 1, 1));
        fill(&mut grid, [2, 4, 0], [2, 4, 2]);

        let capped = find(&grid, cell(0, 1, 1), cell(6, 1, 1));

        assert!(open.is_some(), "the rise is walkable without the ceiling");
        assert!(
            standable(&grid, cell(2, 1, 1)),
            "the cell before the rise is still standable"
        );
        assert_eq!(capped, None, "a fourth air cell is needed to step up");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = flat(8, 8);
        fill(&mut grid, [4, 1, 4], [6, 2, 6]);
        grid.mutator().remove(cell(5, 1, 5));
        grid.mutator().remove(cell(5, 2, 5));

        let found = find(&grid, cell(0, 1, 0), cell(5, 1, 5));

        assert!(
            standable(&grid, cell(5, 1, 5)),
            "the goal cell itself is fine"
        );
        assert_eq!(found, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = flat(20, 0);
        let goal = cell(20, 1, 0);

        let starved = route(&grid, cell(0, 1, 0), |c| c == goal, goal, 5);
        let funded = route(&grid, cell(0, 1, 0), |c| c == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None);
        assert_eq!(funded.map(|r| r.len()), Some(20));
    }

    #[test]
    fn the_heuristic_keeps_an_open_field_search_inside_a_small_budget() {
        let grid = flat(30, 30);
        let start = cell(2, 1, 15);
        let goal = cell(22, 1, 15);

        let found = route(&grid, start, |c| c == goal, goal, 25);

        assert_eq!(
            found.map(|r| r.len()),
            Some(20),
            "a Manhattan-guided search walks the line; a blind one floods the field"
        );
    }

    #[test]
    fn a_route_around_a_concave_wall_is_the_shortest_one() {
        let mut grid = flat(12, 12);
        fill(&mut grid, [6, 1, 3], [6, 2, 7]);
        fill(&mut grid, [3, 1, 3], [6, 2, 3]);
        fill(&mut grid, [3, 1, 7], [6, 2, 7]);

        let found = find(&grid, cell(0, 1, 5), cell(10, 1, 5));

        assert_eq!(
            found.map(|r| r.len()),
            Some(16),
            "3 down to z = 2, 10 across, 3 back up"
        );
    }

    #[test]
    fn the_search_stops_at_the_nearest_of_several_goals() {
        let grid = flat(10, 0);

        let found = route(
            &grid,
            cell(5, 1, 0),
            |c| c == cell(2, 1, 0) || c == cell(9, 1, 0),
            cell(5, 1, 0),
            ROUTE_NODE_BUDGET,
        );

        assert_eq!(
            found,
            Some(vec![cell(4, 1, 0), cell(3, 1, 0), cell(2, 1, 0)])
        );
    }

    fn terrain(heights: &[u8]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (i, &h) in heights.iter().enumerate() {
            let (x, z) = ((i % 5) as i32, (i / 5) as i32);
            fill(&mut grid, [x, 0, z], [x, i32::from(h) - 1, z]);
        }
        grid
    }

    fn column_feet(heights: &[u8], index: usize) -> BlockPos {
        cell(
            (index % 5) as i32,
            i32::from(heights[index]),
            (index / 5) as i32,
        )
    }

    const SIDE: usize = 8;

    fn square_terrain(heights: &[u8]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (i, &h) in heights.iter().enumerate() {
            let (x, z) = ((i % SIDE) as i32, (i / SIDE) as i32);
            fill(&mut grid, [x, 0, z], [x, i32::from(h) - 1, z]);
        }
        grid
    }

    fn square_feet(heights: &[u8], index: usize) -> BlockPos {
        cell(
            (index % SIDE) as i32,
            i32::from(heights[index]),
            (index / SIDE) as i32,
        )
    }

    /// The moves the spec allows, written out independently of the search.
    fn steps(grid: &VoxelGrid, from: BlockPos) -> Vec<BlockPos> {
        let headroom = !grid.is_solid_at(from + BlockPos::Y * 3);
        let mut out = Vec::new();
        for d in [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z] {
            let level = from + d;
            if standable(grid, level) {
                out.push(level);
            } else if headroom && standable(grid, level + BlockPos::Y) {
                out.push(level + BlockPos::Y);
            } else if standable(grid, level - BlockPos::Y) {
                out.push(level - BlockPos::Y);
            }
        }
        out
    }

    fn breadth_first_length(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<usize> {
        let mut seen: HashMap<BlockPos, usize> = HashMap::from([(start, 0)]);
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(at) = queue.pop_front() {
            let dist = seen[&at];
            if at == goal {
                return Some(dist);
            }
            for next in steps(grid, at) {
                if let std::collections::hash_map::Entry::Vacant(slot) = seen.entry(next) {
                    slot.insert(dist + 1);
                    queue.push_back(next);
                }
            }
        }
        None
    }

    proptest! {
        #[test]
        fn routes_are_as_short_as_a_breadth_first_search(
            heights in proptest::collection::vec(1u8..=4, SIDE * SIDE),
            from in 0usize..SIDE * SIDE,
            to in 0usize..SIDE * SIDE,
        ) {
            let grid = square_terrain(&heights);
            let start = square_feet(&heights, from);
            let goal = square_feet(&heights, to);

            let found = find(&grid, start, goal).map(|r| r.len());

            prop_assert_eq!(found, breadth_first_length(&grid, start, goal));
        }

        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1u8..=3, 25),
            from in 0usize..25,
            to in 0usize..25,
        ) {
            let grid = terrain(&heights);
            let start = column_feet(&heights, from);
            let goal = column_feet(&heights, to);

            if let Some(found) = find(&grid, start, goal) {
                prop_assert_eq!(found.last().copied().unwrap_or(start), goal);
                let mut previous = start;
                for step in found {
                    prop_assert!(standable(&grid, step), "{step:?} is not standable");
                    let d = step - previous;
                    prop_assert_eq!(d.x.abs() + d.z.abs(), 1, "one horizontal step: {:?}", d);
                    prop_assert!(d.y.abs() <= 1, "at most one block up or down: {d:?}");
                    previous = step;
                }
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1u8..=3, 25),
            from in 0usize..25,
            to in 0usize..25,
        ) {
            let grid = terrain(&heights);
            let start = column_feet(&heights, from);
            let goal = column_feet(&heights, to);

            let first = find(&grid, start, goal);
            let second = find(&grid, start, goal);

            prop_assert_eq!(first, second);
        }
    }
}
