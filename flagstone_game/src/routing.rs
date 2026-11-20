//! Ground routing over the voxel grid: where a pawn can stand and how it walks between cells.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Node expansions a single [`route`] search may spend before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn's feet can rest in `cell`: solid ground directly below and three cells of
/// air from the feet up (the capsule is 2.3 m tall).
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..3).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell holding a pawn's feet: each coordinate floored.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// A* over standable cells from `start` to the nearest cell satisfying `is_goal`.
///
/// Moves are the four horizontal neighbours, each at the same height or one block up or down;
/// a step up needs a fourth air cell above the current feet. `toward` steers the Manhattan
/// heuristic. The returned cells exclude `start` and end at the goal cell (empty when `start`
/// is already a goal). A `start` that is not standable tries the cell one above and one below.
/// Returns `None` when no route exists or `budget` expansions are spent first.
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
        let delta = (cell - toward).abs();
        delta.x + delta.y + delta.z
    };

    // Entries order by (f, g, insertion order); the key makes the pop order deterministic.
    let mut open = BinaryHeap::new();
    let mut seen: HashMap<[i32; 3], (i32, Option<BlockPos>)> = HashMap::new();
    let mut closed: HashSet<[i32; 3]> = HashSet::new();
    let mut inserted = 0_usize;
    open.push(Reverse((heuristic(start), 0, inserted, start.to_array())));
    seen.insert(start.to_array(), (0, None));

    let mut expansions = 0;
    while let Some(Reverse((_, g, _, key))) = open.pop() {
        if !closed.insert(key) {
            continue;
        }
        let cell = BlockPos::from_array(key);
        if is_goal(cell) {
            return Some(path_to(&seen, cell, start));
        }
        if expansions >= budget {
            return None;
        }
        expansions += 1;

        for neighbour in neighbours(grid, cell) {
            let next_key = neighbour.to_array();
            let next_g = g + 1;
            if closed.contains(&next_key)
                || seen.get(&next_key).is_some_and(|&(best, _)| best <= next_g)
            {
                continue;
            }
            seen.insert(next_key, (next_g, Some(cell)));
            inserted += 1;
            open.push(Reverse((
                next_g + heuristic(neighbour),
                next_g,
                inserted,
                next_key,
            )));
        }
    }
    None
}

/// The cells reachable from `cell` in one move, in a fixed order.
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    let can_climb = !grid.is_solid_at(cell + BlockPos::Y * 3);
    [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z]
        .into_iter()
        .flat_map(move |side| {
            let level = cell + side;
            [level, level + BlockPos::Y, level - BlockPos::Y]
        })
        .filter(move |&next| (next.y <= cell.y || can_climb) && standable(grid, next))
}

/// The cells from `start` (exclusive) to `goal`, following the recorded parents.
fn path_to(
    seen: &HashMap<[i32; 3], (i32, Option<BlockPos>)>,
    goal: BlockPos,
    start: BlockPos,
) -> Vec<BlockPos> {
    let mut path = Vec::new();
    let mut cell = goal;
    while cell != start {
        path.push(cell);
        cell = seen[&cell.to_array()]
            .1
            .expect("every cell but the start was reached from a parent");
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const GROUND: u32 = 1;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// Ground one block thick (`y == 0`) over `x` in `0..=9`, `z` in `0..=4`.
    fn flat_ground() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(9, 0, 4), GROUND);
        grid
    }

    fn route_to(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let mut grid = flat_ground();
        let feet = cell(2, 1, 2);
        assert!(standable(&grid, feet), "open ground");
        assert!(!standable(&grid, cell(2, 0, 2)), "inside the ground block");
        assert!(!standable(&grid, cell(2, 2, 2)), "air with no ground below");

        grid.mutator().place(cell(2, 4, 2), GROUND, None);
        assert!(
            standable(&grid, feet),
            "a block above the third air cell is clear"
        );

        grid.mutator().place(cell(2, 3, 2), GROUND, None);
        assert!(!standable(&grid, feet), "the third air cell is blocked");
    }

    #[test]
    fn feet_cell_floors_each_coordinate() {
        assert_eq!(feet_cell(Vec3::new(2.9, 1.0, 3.1)), cell(2, 1, 3));
        assert_eq!(feet_cell(Vec3::new(-0.1, 0.99, -1.5)), cell(-1, 0, -2));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(0, 1, 2), cell(5, 1, 2));

        assert_eq!(
            path,
            Some(vec![
                cell(1, 1, 2),
                cell(2, 1, 2),
                cell(3, 1, 2),
                cell(4, 1, 2),
                cell(5, 1, 2)
            ])
        );
    }

    #[test]
    fn a_start_that_is_already_a_goal_needs_no_steps() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(2, 1, 2), cell(2, 1, 2));

        assert_eq!(path, Some(vec![]));
    }

    #[test]
    fn a_start_one_cell_off_the_ground_still_routes() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(0, 2, 2), cell(2, 1, 2));

        assert_eq!(path.and_then(|p| p.last().copied()), Some(cell(2, 1, 2)));
    }

    #[test]
    fn a_start_inside_the_ground_routes_from_the_cell_above() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(0, 0, 2), cell(2, 1, 2));

        assert_eq!(path.and_then(|p| p.last().copied()), Some(cell(2, 1, 2)));
    }

    /// Flat ground with a plateau one block higher for `x >= 3`.
    fn rise() -> VoxelGrid {
        let mut grid = flat_ground();
        grid.mutator()
            .fill_region(cell(3, 1, 0), cell(9, 1, 4), GROUND);
        grid
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let grid = rise();

        let up = route_to(&grid, cell(0, 1, 2), cell(6, 2, 2));
        let down = route_to(&grid, cell(6, 2, 2), cell(0, 1, 2));

        assert_eq!(
            up,
            Some(vec![
                cell(1, 1, 2),
                cell(2, 1, 2),
                cell(3, 2, 2),
                cell(4, 2, 2),
                cell(5, 2, 2),
                cell(6, 2, 2)
            ])
        );
        assert_eq!(
            down,
            Some(vec![
                cell(5, 2, 2),
                cell(4, 2, 2),
                cell(3, 2, 2),
                cell(2, 1, 2),
                cell(1, 1, 2),
                cell(0, 1, 2)
            ])
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = flat_ground();
        grid.mutator()
            .fill_region(cell(3, 1, 0), cell(3, 2, 4), GROUND);

        let path = route_to(&grid, cell(0, 1, 2), cell(5, 1, 2));

        assert_eq!(path, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut grid = rise();
        grid.mutator()
            .fill_region(cell(2, 4, 0), cell(2, 4, 4), GROUND);

        let path = route_to(&grid, cell(0, 1, 2), cell(6, 2, 2));

        assert_eq!(
            path, None,
            "no fourth air cell above the cell before the step"
        );
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = flat_ground();
        for around in [cell(4, 1, 2), cell(6, 1, 2), cell(5, 1, 1), cell(5, 1, 3)] {
            grid.mutator()
                .fill_region(around, cell(around.x, 2, around.z), GROUND);
        }

        let path = route_to(&grid, cell(0, 1, 2), cell(5, 1, 2));

        assert_eq!(path, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = flat_ground();
        let goal = cell(5, 1, 2);

        let starved = route(&grid, cell(0, 1, 2), |c| c == goal, goal, 3);
        let funded = route(&grid, cell(0, 1, 2), |c| c == goal, goal, 100);

        assert_eq!(starved, None);
        assert_eq!(funded.map(|p| p.len()), Some(5));
        assert_eq!(ROUTE_NODE_BUDGET, 4096);
    }

    /// Single columns `(x, z, height)`, each solid for `y` in `0..height`.
    fn strip(columns: &[(i32, i32, i32)]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for &(x, z, height) in columns {
            grid.mutator()
                .fill_region(cell(x, 0, z), cell(x, height - 1, z), GROUND);
        }
        grid
    }

    /// A route of `steps` cells needs exactly `steps` expansions when the heuristic never lures the
    /// search off the line: one budget short of that gives up.
    fn assert_needs_exactly(grid: &VoxelGrid, start: BlockPos, goal: BlockPos, steps: usize) {
        let enough = route(grid, start, |c| c == goal, goal, steps);
        let one_short = route(grid, start, |c| c == goal, goal, steps - 1);

        assert_eq!(enough.map(|p| p.len()), Some(steps));
        assert_eq!(one_short, None);
    }

    #[test]
    fn the_search_expands_only_the_line_toward_a_goal_along_x() {
        let columns: Vec<_> = (0..=14).map(|x| (x, 2, 1)).collect();
        let grid = strip(&columns);

        assert_needs_exactly(&grid, cell(5, 1, 2), cell(12, 1, 2), 7);
    }

    #[test]
    fn the_search_expands_only_the_line_toward_a_goal_along_z() {
        let columns: Vec<_> = (0..=14).map(|z| (2, z, 1)).collect();
        let grid = strip(&columns);

        assert_needs_exactly(&grid, cell(2, 1, 5), cell(2, 1, 12), 7);
    }

    #[test]
    fn the_search_expands_only_the_line_toward_a_goal_up_a_staircase() {
        let columns: Vec<_> = (0..=14).map(|x| (x, 2, (x - 4).max(1))).collect();
        let grid = strip(&columns);

        assert_needs_exactly(&grid, cell(5, 1, 2), cell(12, 8, 2), 7);
    }

    #[test]
    fn equally_short_routes_take_the_first_neighbour_in_move_order() {
        let grid = flat_ground();

        let path = route_to(&grid, cell(3, 1, 2), cell(4, 1, 3));

        assert_eq!(path, Some(vec![cell(4, 1, 2), cell(4, 1, 3)]));
    }

    /// Wall columns two blocks high, which a pawn cannot climb.
    fn wall(grid: &mut VoxelGrid, x: i32, zs: std::ops::RangeInclusive<i32>) {
        for z in zs {
            grid.mutator()
                .fill_region(cell(x, 1, z), cell(x, 2, z), GROUND);
        }
    }

    fn shortest_len(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<usize> {
        let mut dist: HashMap<[i32; 3], usize> = HashMap::from([(start.to_array(), 0)]);
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            let here = dist[&current.to_array()];
            if current == goal {
                return Some(here);
            }
            for next in neighbours(grid, current) {
                if let std::collections::hash_map::Entry::Vacant(slot) = dist.entry(next.to_array())
                {
                    slot.insert(here + 1);
                    queue.push_back(next);
                }
            }
        }
        None
    }

    #[test]
    fn routes_around_walls_are_shortest() {
        type Wall = (i32, std::ops::RangeInclusive<i32>);
        let layouts: [(&str, &[Wall]); 4] = [
            ("one wall, far gap", &[(4, 0..=3)]),
            ("serpentine", &[(2, 0..=3), (5, 1..=4), (7, 0..=3)]),
            (
                "cup facing the start",
                &[(6, 1..=3), (4, 1..=1), (5, 1..=1), (4, 3..=3), (5, 3..=3)],
            ),
            ("two gaps, near one first", &[(3, 1..=4), (6, 0..=3)]),
        ];
        let goals = [cell(9, 1, 0), cell(9, 1, 4), cell(9, 1, 2)];

        for (name, walls) in layouts {
            let mut grid = flat_ground();
            for (x, zs) in walls {
                wall(&mut grid, *x, zs.clone());
            }
            for goal in goals {
                for start in [cell(0, 1, 0), cell(0, 1, 2), cell(0, 1, 4)] {
                    let found = route_to(&grid, start, goal).map(|p| p.len());

                    assert_eq!(
                        found,
                        shortest_len(&grid, start, goal),
                        "{name}: {start} to {goal}"
                    );
                }
            }
        }
    }

    /// A 8x8 heightmap: column `(x, z)` is solid for `y` in `0..height`.
    fn terrain(heights: &[u8]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..8 {
            for z in 0..8 {
                let h = i32::from(heights[(x * 8 + z) as usize]);
                grid.mutator()
                    .fill_region(cell(x, 0, z), cell(x, h - 1, z), GROUND);
            }
        }
        grid
    }

    fn feet_on(heights: &[u8], x: i32, z: i32) -> BlockPos {
        cell(x, i32::from(heights[(x * 8 + z) as usize]), z)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1u8..=2, 64),
            sx in 0i32..8, sz in 0i32..8, gx in 0i32..8, gz in 0i32..8,
        ) {
            let grid = terrain(&heights);
            let start = feet_on(&heights, sx, sz);
            let goal = feet_on(&heights, gx, gz);

            let path = route_to(&grid, start, goal);

            prop_assert!(path.is_some(), "heights of one or two blocks are always walkable");
            let path = path.unwrap_or_default();
            prop_assert_eq!(path.last().copied().unwrap_or(start), goal);
            let mut previous = start;
            for step in path {
                prop_assert!(standable(&grid, step), "{step} is not standable");
                let horizontal = (step.x - previous.x).abs() + (step.z - previous.z).abs();
                prop_assert_eq!(horizontal, 1, "{} to {} is not one horizontal move", previous, step);
                prop_assert!((step.y - previous.y).abs() <= 1, "{} to {} climbs too far", previous, step);
                previous = step;
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1u8..=2, 64),
            sx in 0i32..8, sz in 0i32..8, gx in 0i32..8, gz in 0i32..8,
        ) {
            let start = feet_on(&heights, sx, sz);
            let goal = feet_on(&heights, gx, gz);
            let first_grid = terrain(&heights);
            let second_grid = terrain(&heights);

            let first = route_to(&first_grid, start, goal);
            let again = route_to(&first_grid, start, goal);
            let rebuilt = route_to(&second_grid, start, goal);

            prop_assert!(first.is_some(), "heights of one or two blocks are always walkable");
            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &rebuilt);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        #[test]
        fn a_route_over_flat_ground_with_walls_is_a_shortest_one(
            walled in proptest::collection::vec(prop::bool::weighted(0.35), 64),
            sx in 0i32..8, sz in 0i32..8, gx in 0i32..8, gz in 0i32..8,
        ) {
            prop_assume!(!walled[(sx * 8 + sz) as usize] && !walled[(gx * 8 + gz) as usize]);
            let mut grid = terrain(&[1; 64]);
            for x in 0..8 {
                for z in (0..8).filter(|z| walled[(x * 8 + z) as usize]) {
                    wall(&mut grid, x, z..=z);
                }
            }
            let start = cell(sx, 1, sz);
            let goal = cell(gx, 1, gz);

            let found = route_to(&grid, start, goal).map(|p| p.len());

            prop_assert_eq!(found, shortest_len(&grid, start, goal));
        }
    }
}
