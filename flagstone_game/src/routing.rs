//! Strategy-line routing over the voxel grid: which cells a pawn can stand on and the
//! shortest walk between them.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Most cells one search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground below and three cells of air
/// from the feet up.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..3).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell a pawn with its feet at `feet` stands in.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// The four horizontal neighbours, in the fixed order that makes ties deterministic.
const SIDEWAYS: [BlockPos; 4] = [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z];

/// The standable cell a pawn whose feet are in `start` walks from: `start`, else the cell
/// above, else the one below (a pawn on a slope).
fn walkable_start(grid: &VoxelGrid, start: BlockPos) -> Option<BlockPos> {
    [start, start + BlockPos::Y, start - BlockPos::Y]
        .into_iter()
        .find(|cell| standable(grid, *cell))
}

/// The cells one step from `cell`: sideways on the level, up one block (with a fourth cell
/// of air above the current feet) or down one block.
fn steps(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    let can_climb = !grid.is_solid_at(cell + BlockPos::Y * 3);
    SIDEWAYS.into_iter().filter_map(move |side| {
        let next = cell + side;
        [next, next + BlockPos::Y, next - BlockPos::Y]
            .into_iter()
            .enumerate()
            .find(|(i, c)| (*i != 1 || can_climb) && standable(grid, *c))
            .map(|(_, c)| c)
    })
}

/// Horizontal Manhattan distance, less the two a goal window can sit from `toward`, so the
/// estimate never exceeds the true route length.
fn estimate(cell: BlockPos, toward: BlockPos) -> u32 {
    ((cell.x - toward.x).unsigned_abs() + (cell.z - toward.z).unsigned_abs()).saturating_sub(2)
}

/// The cells to walk from `start` to the nearest cell satisfying `is_goal`, `start`
/// excluded and the goal cell last; `None` when no goal is reachable within `budget`
/// expansions. `toward` steers the search.
pub fn route(
    grid: &VoxelGrid,
    start: BlockPos,
    is_goal: impl Fn(BlockPos) -> bool,
    toward: BlockPos,
    budget: usize,
) -> Option<Vec<BlockPos>> {
    let start = walkable_start(grid, start)?;
    // Nodes in insertion order: the cell and the index of the node it was reached from.
    let mut nodes = vec![(start, usize::MAX)];
    let mut best_cost: HashMap<BlockPos, u32> = HashMap::from([(start, 0)]);
    let mut open = BinaryHeap::from([Reverse((estimate(start, toward), 0u32, 0usize))]);
    let mut expanded = 0;

    while let Some(Reverse((_, cost, index))) = open.pop() {
        let cell = nodes[index].0;
        if best_cost.get(&cell).is_some_and(|best| *best < cost) {
            continue;
        }
        if is_goal(cell) {
            let mut path = Vec::new();
            let mut at = index;
            while nodes[at].1 != usize::MAX {
                path.push(nodes[at].0);
                at = nodes[at].1;
            }
            path.reverse();
            return Some(path);
        }
        if expanded == budget {
            return None;
        }
        expanded += 1;
        for next in steps(grid, cell) {
            let next_cost = cost + 1;
            if best_cost.get(&next).is_some_and(|best| *best <= next_cost) {
                continue;
            }
            best_cost.insert(next, next_cost);
            nodes.push((next, index));
            open.push(Reverse((
                next_cost + estimate(next, toward),
                next_cost,
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

    const GROUND_MATERIAL: u32 = 0;

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator()
            .place(BlockPos::new(x, y, z), GROUND_MATERIAL, None);
    }

    /// Ground one block thick at `y = 0` over `x` in `0..width`, `z` in `0..depth`.
    fn flat_ground(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    /// Ground along the single row `z = 1` only, `x` in `0..length`, so every walk stays on it.
    fn corridor(length: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..length {
            place(&mut grid, x, 0, 1);
        }
        grid
    }

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn reach(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = flat_ground(5, 5);

        assert!(standable(&grid, cell(2, 1, 2)), "ground below, open air");
        assert!(!standable(&grid, cell(2, 2, 2)), "nothing below the feet");
        assert!(!standable(&grid, cell(2, 0, 2)), "the cell is itself solid");
        place(&mut grid, 2, 4, 2);
        assert!(
            standable(&grid, cell(2, 1, 2)),
            "a block just above three cells of air does not matter"
        );
        place(&mut grid, 3, 3, 3);
        assert!(!standable(&grid, cell(3, 1, 3)), "the head cell is solid");
        place(&mut grid, 1, 2, 1);
        assert!(!standable(&grid, cell(1, 1, 1)), "the middle cell is solid");
        place(&mut grid, 0, 1, 0);
        assert!(!standable(&grid, cell(0, 1, 0)), "the feet cell is solid");
    }

    #[test]
    fn feet_cell_floors_the_position() {
        assert_eq!(feet_cell(Vec3::new(3.5, 2.0, -0.2)), cell(3, 2, -1));
        assert_eq!(feet_cell(Vec3::new(-0.01, 1.99, 7.99)), cell(-1, 1, 7));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = flat_ground(10, 3);

        let path = reach(&grid, cell(1, 1, 1), cell(6, 1, 1));

        let expected: Vec<BlockPos> = (2..=6).map(|x| cell(x, 1, 1)).collect();
        assert_eq!(path, Some(expected));
    }

    #[test]
    fn a_route_to_the_start_is_empty() {
        let grid = flat_ground(4, 4);

        let path = reach(&grid, cell(1, 1, 1), cell(1, 1, 1));

        assert_eq!(path, Some(Vec::new()));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = corridor(10);
        place(&mut grid, 4, 1, 1);
        place(&mut grid, 5, 1, 1);

        let path = reach(&grid, cell(1, 1, 1), cell(8, 1, 1));

        let expected = vec![
            cell(2, 1, 1),
            cell(3, 1, 1),
            cell(4, 2, 1),
            cell(5, 2, 1),
            cell(6, 1, 1),
            cell(7, 1, 1),
            cell(8, 1, 1),
        ];
        assert_eq!(path, Some(expected));
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = corridor(10);
        place(&mut grid, 4, 1, 1);
        place(&mut grid, 4, 2, 1);

        let path = reach(&grid, cell(1, 1, 1), cell(8, 1, 1));

        assert_eq!(path, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = corridor(10);
        place(&mut grid, 4, 1, 1);
        let open = reach(&grid, cell(1, 1, 1), cell(7, 1, 1));
        place(&mut grid, 3, 4, 1);

        let low_ceiling = reach(&grid, cell(1, 1, 1), cell(7, 1, 1));

        assert!(open.is_some(), "the rise is climbable with headroom");
        assert_eq!(low_ceiling, None, "no fourth air cell, no climb");
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = flat_ground(9, 9);
        for y in 1..=3 {
            for (x, z) in [(3, 4), (5, 4), (4, 3), (4, 5)] {
                place(&mut grid, x, y, z);
            }
        }
        let goal = cell(4, 1, 4);
        assert!(standable(&grid, goal), "setup: the goal cell is standable");

        let path = reach(&grid, cell(1, 1, 1), goal);

        assert_eq!(path, None);
    }

    #[test]
    fn a_start_sunk_into_a_slope_routes_from_the_cell_above() {
        let mut grid = corridor(10);
        place(&mut grid, 3, 1, 1);
        let sunk = cell(3, 1, 1);
        assert!(
            !standable(&grid, sunk),
            "setup: the feet are inside a block"
        );

        let path = reach(&grid, sunk, cell(7, 1, 1)).expect("the cell above is tried");

        assert_eq!(path.last(), Some(&cell(7, 1, 1)));
        assert!(path.iter().all(|c| standable(&grid, *c)));
    }

    #[test]
    fn a_start_floating_over_a_slope_routes_from_the_cell_below() {
        let mut grid = corridor(10);
        place(&mut grid, 3, 1, 1);
        let floating = cell(3, 3, 1);
        assert!(!standable(&grid, floating), "setup: nothing under the feet");

        let path = reach(&grid, floating, cell(7, 1, 1)).expect("the cell below is tried");

        assert_eq!(path.last(), Some(&cell(7, 1, 1)));
        assert!(path.iter().all(|c| standable(&grid, *c)));
    }

    #[test]
    fn a_start_two_cells_from_standable_has_no_route() {
        let grid = corridor(10);

        let path = reach(&grid, cell(3, 4, 1), cell(7, 1, 1));

        assert_eq!(path, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = flat_ground(24, 24);
        let (start, goal) = (cell(1, 1, 1), cell(20, 1, 20));

        let starved = route(&grid, start, |c| c == goal, goal, 3);
        let funded = route(&grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(ROUTE_NODE_BUDGET, 4096);
        assert_eq!(starved, None, "three expansions cannot cross 38 cells");
        assert_eq!(funded.expect("the default budget suffices").len(), 38);
    }

    /// A grid whose column `(x, z)` is solid from `y = 0` up to `heights[x][z]` exclusive.
    fn terrain(heights: &[Vec<i32>], reversed: bool) -> VoxelGrid {
        let mut columns: Vec<(i32, i32, i32)> = Vec::new();
        for (x, row) in heights.iter().enumerate() {
            for (z, &height) in row.iter().enumerate() {
                for y in 0..height {
                    columns.push((x as i32, y, z as i32));
                }
            }
        }
        if reversed {
            columns.reverse();
        }
        let mut grid = VoxelGrid::new(16);
        for (x, y, z) in columns {
            place(&mut grid, x, y, z);
        }
        grid
    }

    fn heightmap() -> impl Strategy<Value = Vec<Vec<i32>>> {
        proptest::collection::vec(proptest::collection::vec(1..=2i32, 6), 6)
    }

    fn column_cell(heights: &[Vec<i32>], x: usize, z: usize) -> BlockPos {
        cell(x as i32, heights[x][z], z as i32)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in heightmap(),
            from in (0..6usize, 0..6usize),
            to in (0..6usize, 0..6usize),
        ) {
            let grid = terrain(&heights, false);
            let start = column_cell(&heights, from.0, from.1);
            let goal = column_cell(&heights, to.0, to.1);

            let path = reach(&grid, start, goal);

            prop_assert!(path.is_some(), "steps of one block always connect {} to {}", start, goal);
            let path = path.expect("checked above");
            let mut previous = start;
            for step in &path {
                prop_assert!(standable(&grid, *step), "{step} is not standable");
                let horizontal = (step.x - previous.x).abs() + (step.z - previous.z).abs();
                prop_assert_eq!(horizontal, 1, "{} to {} is not one block sideways", previous, step);
                prop_assert!((step.y - previous.y).abs() <= 1, "{} to {} climbs too far", previous, step);
                previous = *step;
            }
            prop_assert_eq!(previous, goal, "the route ends at the goal");
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in heightmap(),
            from in (0..6usize, 0..6usize),
            to in (0..6usize, 0..6usize),
        ) {
            let start = column_cell(&heights, from.0, from.1);
            let goal = column_cell(&heights, to.0, to.1);

            let forward = reach(&terrain(&heights, false), start, goal);
            let backward = reach(&terrain(&heights, true), start, goal);
            let again = reach(&terrain(&heights, false), start, goal);

            prop_assert_eq!(&forward, &backward, "the order blocks were placed in changed the route");
            prop_assert_eq!(&forward, &again);
        }
    }
}
