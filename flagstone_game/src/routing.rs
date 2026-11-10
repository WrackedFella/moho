//! Pathing for pawns over the voxel ground: which cells a pawn can stand in and the way
//! between two of them.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Cells a search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Cells a body needs free from the feet up, and so the air a standing pawn needs.
const BODY_CELLS: i32 = 3;

/// Whether a pawn can stand with its feet in `cell`: ground below and the capsule's height
/// of air from the feet up.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..BODY_CELLS).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// The standable cells a pawn in `from` can step to: one block up (with a fourth cell of air
/// above the current feet), level, or one block down, in each of the four horizontal
/// directions.
fn steps(grid: &VoxelGrid, from: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    const HORIZONTAL: [BlockPos; 4] = [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z];
    let headroom = !grid.is_solid_at(from + BlockPos::Y * BODY_CELLS);
    HORIZONTAL.into_iter().filter_map(move |d| {
        let level = from + d;
        [
            (headroom, level + BlockPos::Y),
            (true, level),
            (true, level - BlockPos::Y),
        ]
        .into_iter()
        .find(|&(allowed, cell)| allowed && standable(grid, cell))
        .map(|(_, cell)| cell)
    })
}

fn manhattan_xz(a: BlockPos, b: BlockPos) -> i32 {
    (a.x - b.x).abs() + (a.z - b.z).abs()
}

/// The cheapest walk from `start` to a cell `is_goal` accepts, as the chain of cells from
/// `start` to the goal inclusive, or `None` when there is none within `budget` expansions.
/// `toward` steers the search; it does not have to be a goal.
///
/// A `start` that is not standable falls back to the cell above, then the one below, for a
/// pawn whose feet sit slightly in or off a slope.
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

    // Ordered by (f, g, insertion order) for a deterministic tie-break.
    let mut open = BinaryHeap::new();
    // Best known cost to each reached cell, and the cell it was reached from.
    let mut reached: HashMap<BlockPos, (i32, Option<BlockPos>)> = HashMap::with_capacity(budget);
    reached.insert(start, (0, None));
    let mut inserted = 0_usize;
    open.push(Reverse((
        manhattan_xz(start, toward),
        0,
        inserted,
        start.to_array(),
    )));

    let mut expansions = 0;
    while let Some(Reverse((_, g, _, cell))) = open.pop() {
        let cell = BlockPos::from_array(cell);
        if reached.get(&cell).is_some_and(|&(best, _)| best < g) {
            continue;
        }
        if is_goal(cell) {
            let mut walk = vec![cell];
            let mut at = cell;
            while let Some(&(_, Some(previous))) = reached.get(&at) {
                walk.push(previous);
                at = previous;
            }
            walk.reverse();
            return Some(walk);
        }
        if expansions >= budget {
            return None;
        }
        expansions += 1;
        for next in steps(grid, cell) {
            let next_g = g + 1;
            if reached.get(&next).is_some_and(|&(best, _)| best <= next_g) {
                continue;
            }
            reached.insert(next, (next_g, Some(cell)));
            inserted += 1;
            open.push(Reverse((
                next_g + manhattan_xz(next, toward),
                next_g,
                inserted,
                next.to_array(),
            )));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const STONE: u32 = 1;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn fill(grid: &mut VoxelGrid, min: (i32, i32, i32), max: (i32, i32, i32)) {
        grid.mutator()
            .fill_region(cell(min.0, min.1, min.2), cell(max.0, max.1, max.2), STONE);
    }

    /// One block of ground (y 0) over x, z in `-extent..=extent`; feet cells are at y 1.
    fn flat_ground(extent: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        fill(&mut grid, (-extent, 0, -extent), (extent, 0, extent));
        grid
    }

    fn reaching_x(x: i32) -> impl Fn(BlockPos) -> bool {
        move |c| c.x == x
    }

    fn go(grid: &VoxelGrid, start: BlockPos, goal_x: i32) -> Option<Vec<BlockPos>> {
        route(
            grid,
            start,
            reaching_x(goal_x),
            cell(goal_x, 1, 0),
            ROUTE_NODE_BUDGET,
        )
    }

    mod standing {
        use super::*;

        #[test]
        fn standable_needs_ground_below_and_the_capsules_height_of_air() {
            let mut grid = flat_ground(3);

            let on_ground = standable(&grid, cell(0, 1, 0));
            let inside_ground = standable(&grid, cell(0, 0, 0));
            let floating = standable(&grid, cell(0, 2, 0));
            grid.mutator().place(cell(2, 3, 0), STONE, None);
            let ceiling_two_above_feet = standable(&grid, cell(2, 1, 0));
            grid.mutator().place(cell(-2, 4, 0), STONE, None);
            let ceiling_three_above_feet = standable(&grid, cell(-2, 1, 0));
            grid.mutator().place(cell(1, 2, 0), STONE, None);
            let ceiling_one_above_feet = standable(&grid, cell(1, 1, 0));
            let beyond_the_ground = standable(&grid, cell(9, 1, 0));

            assert!(on_ground, "open ground is standable");
            assert!(!inside_ground, "a solid cell is not");
            assert!(!floating, "no ground below");
            assert!(!ceiling_two_above_feet, "the head needs the third cell");
            assert!(
                ceiling_three_above_feet,
                "a block four cells up is above the head"
            );
            assert!(!ceiling_one_above_feet, "the torso needs the second cell");
            assert!(!beyond_the_ground, "no ground there");
        }

        #[test]
        fn feet_cell_floors_each_axis() {
            assert_eq!(feet_cell(Vec3::new(0.5, 1.0, 0.5)), cell(0, 1, 0));
            assert_eq!(feet_cell(Vec3::new(3.99, 2.5, -0.01)), cell(3, 2, -1));
            assert_eq!(feet_cell(Vec3::new(-1.0, 0.0, -2.5)), cell(-1, 0, -3));
        }
    }

    mod walking {
        use super::*;

        #[test]
        fn a_route_on_flat_ground_is_a_straight_line() {
            let grid = flat_ground(8);

            let walk = go(&grid, cell(0, 1, 0), 5).expect("open ground routes");

            let expected: Vec<BlockPos> = (0..=5).map(|x| cell(x, 1, 0)).collect();
            assert_eq!(walk, expected);
        }

        #[test]
        fn a_start_that_is_a_goal_routes_to_itself() {
            let grid = flat_ground(3);

            let walk = go(&grid, cell(2, 1, 0), 2);

            assert_eq!(walk, Some(vec![cell(2, 1, 0)]));
        }

        #[test]
        fn a_route_climbs_a_one_block_rise_and_descends_it() {
            let mut grid = flat_ground(6);
            fill(&mut grid, (3, 1, -6), (3, 1, 6));

            let walk = go(&grid, cell(0, 1, 0), 6).expect("a one-block rise is walkable");

            let expected = vec![
                cell(0, 1, 0),
                cell(1, 1, 0),
                cell(2, 1, 0),
                cell(3, 2, 0),
                cell(4, 1, 0),
                cell(5, 1, 0),
                cell(6, 1, 0),
            ];
            assert_eq!(walk, expected);
        }

        #[test]
        fn a_two_block_wall_blocks_the_route() {
            let mut grid = flat_ground(6);
            fill(&mut grid, (3, 1, -6), (3, 2, 6));

            let walk = go(&grid, cell(0, 1, 0), 6);

            assert_eq!(walk, None);
        }

        #[test]
        fn climbing_needs_headroom() {
            let mut open = flat_ground(6);
            fill(&mut open, (3, 1, -6), (3, 1, 6));
            let mut low_ceiling = flat_ground(6);
            fill(&mut low_ceiling, (3, 1, -6), (3, 1, 6));
            // The fourth cell above the feet at x 2: the cell is standable, the climb is not.
            fill(&mut low_ceiling, (2, 4, -6), (2, 4, 6));

            let with_headroom = go(&open, cell(0, 1, 0), 6);
            let without = go(&low_ceiling, cell(0, 1, 0), 6);

            assert!(standable(&low_ceiling, cell(2, 1, 0)));
            assert!(with_headroom.is_some(), "the rise is walkable when open");
            assert_eq!(without, None);
        }

        #[test]
        fn a_drop_of_two_blocks_is_not_a_step_down() {
            let mut grid = flat_ground(6);
            fill(&mut grid, (-6, 1, -6), (2, 2, 6));

            let walk = go(&grid, cell(0, 3, 0), 6);

            assert_eq!(walk, None, "x 3 and beyond lie two blocks lower");
        }

        #[test]
        fn a_start_in_the_ground_routes_from_the_cell_above() {
            let grid = flat_ground(6);

            let walk = go(&grid, cell(0, 0, 0), 4).expect("a slope start is corrected");

            assert_eq!(walk.first(), Some(&cell(0, 1, 0)));
            assert_eq!(walk.last(), Some(&cell(4, 1, 0)));
        }

        #[test]
        fn a_start_floating_one_cell_up_routes_from_the_cell_below() {
            let grid = flat_ground(6);

            let walk = go(&grid, cell(0, 2, 0), 4).expect("a slope start is corrected");

            assert_eq!(walk.first(), Some(&cell(0, 1, 0)));
        }

        #[test]
        fn a_start_far_from_any_ground_has_no_route() {
            let grid = flat_ground(6);

            assert_eq!(go(&grid, cell(0, 5, 0), 4), None);
        }
    }

    mod unreachable_goals {
        use super::*;

        #[test]
        fn an_enclosed_goal_has_no_route() {
            let walls = [(4, 0), (6, 0), (5, -1), (5, 1)];
            let into_the_ring = |c: BlockPos| c.x == 5 && c.z == 0;
            let routed = |walls: &[(i32, i32)]| {
                let mut grid = flat_ground(6);
                for &(x, z) in walls {
                    fill(&mut grid, (x, 1, z), (x, 2, z));
                }
                let walk = route(
                    &grid,
                    cell(0, 1, 0),
                    into_the_ring,
                    cell(5, 1, 0),
                    ROUTE_NODE_BUDGET,
                );
                (standable(&grid, cell(5, 1, 0)), walk)
            };

            let (open_goal, with_a_gap) = routed(&walls[..3]);
            let (still_open, closed) = routed(&walls);

            assert!(open_goal && still_open, "the goal itself is open");
            assert_eq!(
                with_a_gap.and_then(|w| w.last().copied()),
                Some(cell(5, 1, 0))
            );
            assert_eq!(closed, None);
        }

        #[test]
        fn the_node_budget_ends_a_search_with_none() {
            let grid = flat_ground(20);
            let far = |c: BlockPos| c.x == 18;

            let starved = route(&grid, cell(0, 1, 0), far, cell(18, 1, 0), 10);
            let funded = route(&grid, cell(0, 1, 0), far, cell(18, 1, 0), ROUTE_NODE_BUDGET);

            assert_eq!(starved, None, "18 steps need more than 10 expansions");
            assert_eq!(funded.map(|walk| walk.len()), Some(19));
        }
    }

    /// A 7 x 7 field of columns, each solid from y 0 up to its height (0 to 2).
    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (i, height) in heights.iter().enumerate() {
            let (x, z) = (i as i32 % 7, i as i32 / 7);
            fill(&mut grid, (x, 0, z), (x, *height, z));
        }
        grid
    }

    fn column_top(heights: &[i32], x: i32, z: i32) -> i32 {
        heights[(z * 7 + x) as usize] + 1
    }

    fn terrain_route(
        grid: &VoxelGrid,
        heights: &[i32],
        goal: (i32, i32),
    ) -> (BlockPos, Option<Vec<BlockPos>>) {
        let start = cell(0, column_top(heights, 0, 0), 0);
        let top = column_top(heights, goal.0, goal.1);
        let walk = route(
            grid,
            start,
            |c| c.x == goal.0 && c.z == goal.1,
            cell(goal.0, top, goal.1),
            ROUTE_NODE_BUDGET,
        );
        (start, walk)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(0..=2i32, 49),
            gx in 0..7i32,
            gz in 0..7i32,
        ) {
            let grid = terrain(&heights);

            let (start, walk) = terrain_route(&grid, &heights, (gx, gz));

            if let Some(walk) = walk {
                prop_assert_eq!(walk.first(), Some(&start));
                prop_assert_eq!(walk.last().map(|c| (c.x, c.z)), Some((gx, gz)));
                for c in &walk {
                    prop_assert!(standable(&grid, *c), "{c} is not standable");
                }
                for pair in walk.windows(2) {
                    let step = pair[1] - pair[0];
                    prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "{:?}", pair);
                    prop_assert!(step.y.abs() <= 1, "{:?}", pair);
                }
            }
        }

        #[test]
        fn a_flat_field_always_routes_in_manhattan_steps(
            gx in 0..7i32,
            gz in 0..7i32,
        ) {
            let heights = vec![0; 49];
            let grid = terrain(&heights);

            let (_, walk) = terrain_route(&grid, &heights, (gx, gz));

            let walk = walk.expect("open ground always routes");
            prop_assert_eq!(walk.len() as i32, gx + gz + 1);
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(0..=2i32, 49),
            gx in 0..7i32,
            gz in 0..7i32,
        ) {
            let grid = terrain(&heights);
            let rebuilt = terrain(&heights);

            let (_, first) = terrain_route(&grid, &heights, (gx, gz));
            let (_, again) = terrain_route(&grid, &heights, (gx, gz));
            let (_, from_a_copy) = terrain_route(&rebuilt, &heights, (gx, gz));

            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &from_a_copy);
        }
    }
}
