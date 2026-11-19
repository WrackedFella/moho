//! Where a pawn can stand and how it walks between standing places, read from the voxel grid.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// The most cells a search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn's feet can be in `cell`: ground below and the capsule's height of air from
/// the feet up.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..AIR_CELLS).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell a pawn with its feet at `feet` is standing in.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// Air cells a standing pawn needs from the feet up (the capsule is 2.3 m tall).
const AIR_CELLS: i32 = 3;

const HORIZONTAL: [BlockPos; 4] = [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z];

/// The standable cells one move from `cell`: level, one up (with a fourth cell of air above
/// the current feet) or one down.
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    let can_climb = !grid.is_solid_at(cell + BlockPos::Y * AIR_CELLS);
    HORIZONTAL.into_iter().flat_map(move |step| {
        let level = cell + step;
        let up = level + BlockPos::Y;
        let down = level - BlockPos::Y;
        [
            standable(grid, level).then_some(level),
            (can_climb && standable(grid, up)).then_some(up),
            standable(grid, down).then_some(down),
        ]
        .into_iter()
        .flatten()
    })
}

fn manhattan(a: BlockPos, b: BlockPos) -> u32 {
    a.x.abs_diff(b.x) + a.y.abs_diff(b.y) + a.z.abs_diff(b.z)
}

/// The cells to walk from `start` to the first cell where `is_goal` holds, found by A* over
/// standable cells with `toward` steering the heuristic; `None` when no goal cell is reached
/// within `budget` expansions.
pub fn route(
    grid: &VoxelGrid,
    start: BlockPos,
    is_goal: impl Fn(BlockPos) -> bool,
    toward: BlockPos,
    budget: usize,
) -> Option<Vec<BlockPos>> {
    let start = [start, start + BlockPos::Y, start - BlockPos::Y]
        .into_iter()
        .find(|cell| standable(grid, *cell))?;

    // Nodes are numbered in insertion order, which is the final tie-break of the queue.
    let mut nodes = vec![(start, None::<usize>)];
    let mut best_g = HashMap::from([(start, 0u32)]);
    let mut open = BinaryHeap::from([Reverse((manhattan(start, toward), 0u32, 0usize))]);
    let mut expansions = 0;

    while let Some(Reverse((_, g, index))) = open.pop() {
        let (cell, _) = nodes[index];
        if best_g.get(&cell).is_some_and(|best| *best < g) {
            continue;
        }
        if is_goal(cell) {
            return Some(path_to(&nodes, index));
        }
        if expansions >= budget {
            return None;
        }
        expansions += 1;
        for next in neighbours(grid, cell) {
            let next_g = g + 1;
            if best_g.get(&next).is_some_and(|best| *best <= next_g) {
                continue;
            }
            best_g.insert(next, next_g);
            nodes.push((next, Some(index)));
            open.push(Reverse((
                next_g + manhattan(next, toward),
                next_g,
                nodes.len() - 1,
            )));
        }
    }
    None
}

/// The cells from the one after the search's start to `nodes[last]`.
fn path_to(nodes: &[(BlockPos, Option<usize>)], last: usize) -> Vec<BlockPos> {
    let mut path = Vec::new();
    let mut at = last;
    while let (cell, Some(parent)) = nodes[at] {
        path.push(cell);
        at = parent;
    }
    path.reverse();
    path
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

    /// A one-block-wide walkway of ground along x at z = 2, from x = 0 to x = 10.
    fn corridor() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..=10 {
            place(&mut grid, x, 0, 2);
        }
        grid
    }

    /// Open ground, `size` blocks a side, its surface blocks at y = 0.
    fn field(size: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..size {
            for z in 0..size {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    /// The route without its starting cell, which a route may or may not list first.
    fn steps(path: &[BlockPos], start: BlockPos) -> Vec<BlockPos> {
        path.iter().copied().skip_while(|c| *c == start).collect()
    }

    fn to(goal: BlockPos) -> impl Fn(BlockPos) -> bool {
        move |c| c == goal
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = VoxelGrid::new(16);
        place(&mut grid, 0, 0, 0);
        place(&mut grid, 4, 0, 0);
        place(&mut grid, 4, 3, 0);
        place(&mut grid, 8, 0, 0);
        place(&mut grid, 8, 2, 0);
        place(&mut grid, 12, 0, 0);
        place(&mut grid, 12, 1, 0);

        let open = standable(&grid, cell(0, 1, 0));
        let ceiling_a_block_too_low = standable(&grid, cell(4, 1, 0));
        let ceiling_two_too_low = standable(&grid, cell(8, 1, 0));
        let inside_ground = standable(&grid, cell(0, 0, 0));
        let in_the_air = standable(&grid, cell(0, 5, 0));
        let beside_the_ground = standable(&grid, cell(2, 1, 0));
        let on_a_raised_block = standable(&grid, cell(12, 2, 0));

        assert!(open, "ground below and air above");
        assert!(
            !ceiling_a_block_too_low,
            "a block at the third air cell leaves too little room"
        );
        assert!(!ceiling_two_too_low, "a block two cells up is in the way");
        assert!(!inside_ground, "the cell is solid");
        assert!(!in_the_air, "nothing to stand on");
        assert!(!beside_the_ground, "the cell below is empty");
        assert!(on_a_raised_block, "ground may be any solid block");
    }

    #[test]
    fn a_block_four_up_leaves_room_to_stand() {
        let mut grid = VoxelGrid::new(16);
        place(&mut grid, 0, 0, 0);
        place(&mut grid, 0, 4, 0);

        assert!(standable(&grid, cell(0, 1, 0)));
    }

    #[test]
    fn feet_cell_is_the_cell_the_feet_are_in() {
        assert_eq!(feet_cell(Vec3::new(2.5, 1.0, 3.9)), cell(2, 1, 3));
        assert_eq!(feet_cell(Vec3::new(-0.1, 4.99, -2.0)), cell(-1, 4, -2));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = corridor();
        let start = cell(1, 1, 2);
        let goal = cell(7, 1, 2);

        let path = route(&grid, start, to(goal), goal, ROUTE_NODE_BUDGET)
            .expect("the corridor leads to the goal");

        let expected: Vec<BlockPos> = (2..=7).map(|x| cell(x, 1, 2)).collect();
        assert_eq!(steps(&path, start), expected);
    }

    #[test]
    fn a_route_to_where_the_pawn_stands_needs_no_steps() {
        let grid = corridor();
        let start = cell(3, 1, 2);

        let path = route(&grid, start, to(start), start, ROUTE_NODE_BUDGET)
            .expect("the start is the goal");

        assert!(steps(&path, start).is_empty(), "{path:?}");
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = corridor();
        for x in 4..=6 {
            place(&mut grid, x, 1, 2);
        }
        let start = cell(1, 1, 2);
        let goal = cell(9, 1, 2);

        let path = route(&grid, start, to(goal), goal, ROUTE_NODE_BUDGET)
            .expect("a rise of one block can be walked");

        let expected = [
            cell(2, 1, 2),
            cell(3, 1, 2),
            cell(4, 2, 2),
            cell(5, 2, 2),
            cell(6, 2, 2),
            cell(7, 1, 2),
            cell(8, 1, 2),
            cell(9, 1, 2),
        ];
        assert_eq!(steps(&path, start), expected);
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = corridor();
        place(&mut grid, 5, 1, 2);
        place(&mut grid, 5, 2, 2);
        let start = cell(1, 1, 2);
        let goal = cell(9, 1, 2);

        let path = route(&grid, start, to(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(path, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut open = corridor();
        place(&mut open, 5, 1, 2);
        let mut low_ceiling = corridor();
        place(&mut low_ceiling, 5, 1, 2);
        place(&mut low_ceiling, 4, 4, 2);
        let start = cell(1, 1, 2);
        let goal = cell(9, 1, 2);

        let without_ceiling = route(&open, start, to(goal), goal, ROUTE_NODE_BUDGET);
        let with_ceiling = route(&low_ceiling, start, to(goal), goal, ROUTE_NODE_BUDGET);

        assert!(
            standable(&low_ceiling, cell(4, 1, 2)),
            "the pawn can stand where it would jump from"
        );
        assert!(without_ceiling.is_some(), "the step is fine in the open");
        assert_eq!(
            with_ceiling, None,
            "stepping up needs a fourth cell of air above the feet"
        );
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = field(11);
        for x in 4..=6 {
            for z in 4..=6 {
                if (x, z) != (5, 5) {
                    place(&mut grid, x, 1, z);
                    place(&mut grid, x, 2, z);
                }
            }
        }
        let start = cell(1, 1, 1);
        let goal = cell(5, 1, 5);

        let path = route(&grid, start, to(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(path, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = field(40);
        let start = cell(1, 1, 1);
        let goal = cell(38, 1, 38);

        let starved = route(&grid, start, to(goal), goal, 5);
        let funded = route(&grid, start, to(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None, "five expansions cannot cross the field");
        assert!(funded.is_some(), "the full budget can");
    }

    #[test]
    fn a_start_on_a_slope_tries_the_cells_one_above_and_below() {
        let grid = corridor();
        let goal = cell(7, 1, 2);
        let one_below = cell(1, 0, 2);
        let one_above = cell(1, 2, 2);

        let from_below = route(&grid, one_below, to(goal), goal, ROUTE_NODE_BUDGET);
        let from_above = route(&grid, one_above, to(goal), goal, ROUTE_NODE_BUDGET);

        assert!(from_below.is_some(), "a start sunk into the ground");
        assert!(from_above.is_some(), "a start floating over the ground");
    }

    #[test]
    fn a_start_far_from_any_ground_has_no_route() {
        let grid = corridor();
        let goal = cell(7, 1, 2);

        let path = route(&grid, cell(1, 9, 2), to(goal), goal, ROUTE_NODE_BUDGET);

        assert_eq!(path, None);
    }

    /// Terrain of `heights` columns of ground (1 to 2 blocks), 8 by 8.
    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (i, &height) in heights.iter().enumerate() {
            let (x, z) = ((i % 8) as i32, (i / 8) as i32);
            for y in 0..height {
                place(&mut grid, x, y, z);
            }
        }
        grid
    }

    fn on_top(heights: &[i32], x: i32, z: i32) -> BlockPos {
        cell(x, heights[(z * 8 + x) as usize], z)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1..=2i32, 64),
            sx in 0..8i32, sz in 0..8i32, gx in 0..8i32, gz in 0..8i32,
        ) {
            let grid = terrain(&heights);
            let start = on_top(&heights, sx, sz);
            let goal = on_top(&heights, gx, gz);

            let path = route(&grid, start, to(goal), goal, ROUTE_NODE_BUDGET);

            let path = path.expect("terrain that never rises more than a block can be crossed");
            let walked = steps(&path, start);
            prop_assert_eq!(walked.last().copied().unwrap_or(start), goal);
            let mut previous = start;
            for next in walked {
                prop_assert!(standable(&grid, next), "{next:?} cannot be stood on");
                let d = next - previous;
                prop_assert!(d.x.abs() + d.z.abs() == 1, "{previous:?} to {next:?}");
                prop_assert!(d.y.abs() <= 1, "{previous:?} to {next:?}");
                previous = next;
            }
        }
    }

    /// Open ground offers many equal-length routes; the same grid must always give the same one.
    #[test]
    fn the_same_grid_gives_the_same_route() {
        let start = cell(1, 1, 1);
        let goal = cell(8, 1, 8);

        let first = route(&field(10), start, to(goal), goal, ROUTE_NODE_BUDGET);
        let second = route(&field(10), start, to(goal), goal, ROUTE_NODE_BUDGET);

        assert!(first.is_some(), "open ground has a route");
        assert_eq!(first, second);
    }

    /// A straight walk on open ground costs about one expansion a step, so a budget of a few
    /// more than the steps is enough only when the search is steered at the goal on every axis.
    #[test]
    fn a_straight_route_on_open_ground_is_found_within_a_few_expansions_of_its_length() {
        let grid = field(40);
        let along_x = (cell(10, 1, 20), cell(25, 1, 20));
        let along_z = (cell(20, 1, 10), cell(20, 1, 25));

        for (start, goal) in [along_x, along_z] {
            let path = route(&grid, start, to(goal), goal, 20);

            let path = path.unwrap_or_else(|| panic!("{start:?} to {goal:?} needs few expansions"));
            assert_eq!(path.len(), 15, "{start:?} to {goal:?}");
        }
    }

    /// Climbing makes a cell reachable again by a shorter way after it was first queued; the
    /// longer queue entry must not be expanded a second time.
    #[test]
    fn a_cell_reached_again_by_a_shorter_way_is_expanded_once() {
        let heights = [
            2, 1, 2, 1, 2, 1, 1, 1, 2, 1, 2, 1, 2, 1, 1, 2, 1, 1, 1, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2,
            2, 2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 1, 1, 2, 1, 1, 1, 2, 1, 2, 2, 2, 2, 1, 2, 1, 2, 2, 1,
            1, 2, 1, 1, 2, 2,
        ];
        let grid = terrain(&heights);
        let start = on_top(&heights, 4, 0);
        let goal = on_top(&heights, 7, 3);

        let path = route(&grid, start, to(goal), cell(7, 2, 2), 18);

        assert!(path.is_some(), "18 expansions reach the goal");
    }

    /// Breadth-first shortest step count from `start` to `goal`.
    fn fewest_steps(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<usize> {
        let mut seen = HashMap::from([(start, 0usize)]);
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(at) = queue.pop_front() {
            if at == goal {
                return seen.get(&at).copied();
            }
            for next in neighbours(grid, at) {
                if !seen.contains_key(&next) {
                    seen.insert(next, seen[&at] + 1);
                    queue.push_back(next);
                }
            }
        }
        None
    }

    #[test]
    fn a_route_around_walls_is_as_short_as_any_route() {
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        let mut random_percent = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) % 100
        };
        let start = cell(0, 1, 0);
        let goal = cell(11, 1, 11);
        let mut routed = 0;

        for layout in 0..80 {
            let mut grid = field(12);
            for x in 0..12 {
                for z in 0..12 {
                    if cell(x, 1, z) != start && cell(x, 1, z) != goal && random_percent() < 35 {
                        place(&mut grid, x, 1, z);
                        place(&mut grid, x, 2, z);
                    }
                }
            }

            let path = route(&grid, start, to(goal), goal, ROUTE_NODE_BUDGET);

            assert_eq!(
                path.as_ref().map(Vec::len),
                fewest_steps(&grid, start, goal),
                "layout {layout}"
            );
            routed += usize::from(path.is_some());
        }
        assert!(routed >= 20, "only {routed} layouts could be crossed");
    }
}
