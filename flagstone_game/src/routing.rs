//! Walking routes over the voxel grid: what a pawn can stand on and the cheapest way between
//! two standable cells.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Cells a search may expand before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// The four horizontal moves, in the order a search tries them.
const HORIZONTAL_STEPS: [BlockPos; 4] = [
    BlockPos::new(1, 0, 0),
    BlockPos::new(-1, 0, 0),
    BlockPos::new(0, 0, 1),
    BlockPos::new(0, 0, -1),
];

/// Whether a pawn can stand with its feet in `cell`: ground below and three cells of air from
/// the feet up.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..3).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell a pawn whose feet are at `feet` stands in.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// The standable cells one move from `cell`: level, one up (which needs a fourth cell of air
/// above the current feet) or one down.
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    let can_climb = !grid.is_solid_at(cell + BlockPos::Y * 3);
    HORIZONTAL_STEPS.into_iter().filter_map(move |step| {
        let level = cell + step;
        [
            Some(level),
            can_climb.then_some(level + BlockPos::Y),
            Some(level - BlockPos::Y),
        ]
        .into_iter()
        .flatten()
        .find(|&target| standable(grid, target))
    })
}

/// `start` if a pawn can stand there, else the cell above or below it (a pawn on a slope).
fn standing_start(grid: &VoxelGrid, start: BlockPos) -> Option<BlockPos> {
    [start, start + BlockPos::Y, start - BlockPos::Y]
        .into_iter()
        .find(|&cell| standable(grid, cell))
}

fn manhattan(a: BlockPos, b: BlockPos) -> u32 {
    (a - b).abs().element_sum().unsigned_abs()
}

/// The cheapest walk from `start` to the nearest cell satisfying `is_goal`, as the cells to
/// step through in order: `start` itself is not listed, the goal cell is last, and the list is
/// empty when `start` already is a goal. Moves are to the four horizontal neighbours, up or
/// down one block. `toward` is the point the search heads for.
///
/// `None` when no goal is reachable, or when `budget` cells were expanded without finding one.
pub fn route(
    grid: &VoxelGrid,
    start: BlockPos,
    is_goal: impl Fn(BlockPos) -> bool,
    toward: BlockPos,
    budget: usize,
) -> Option<Vec<BlockPos>> {
    let start = standing_start(grid, start)?;
    // Ordered by estimate, then cost so far, then insertion: the same grid, the same route.
    let mut open = BinaryHeap::new();
    let mut inserted = 0usize;
    let mut cost: HashMap<BlockPos, u32> = HashMap::from([(start, 0)]);
    let mut parent: HashMap<BlockPos, BlockPos> = HashMap::new();
    let mut closed: HashSet<BlockPos> = HashSet::new();
    open.push(Reverse((
        manhattan(start, toward),
        0u32,
        inserted,
        start.to_array(),
    )));

    let mut expanded = 0;
    while let Some(Reverse((_, so_far, _, cell))) = open.pop() {
        let cell = BlockPos::from_array(cell);
        if !closed.insert(cell) {
            continue;
        }
        if is_goal(cell) {
            let mut cells = vec![cell];
            while let Some(&previous) = parent.get(cells.last()?) {
                cells.push(previous);
            }
            cells.pop();
            cells.reverse();
            return Some(cells);
        }
        if expanded >= budget {
            return None;
        }
        expanded += 1;
        for next in neighbours(grid, cell) {
            let next_cost = so_far + 1;
            if cost.get(&next).is_some_and(|&known| known <= next_cost) {
                continue;
            }
            cost.insert(next, next_cost);
            parent.insert(next, cell);
            inserted += 1;
            open.push(Reverse((
                next_cost + manhattan(next, toward),
                next_cost,
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

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn put(grid: &mut VoxelGrid, cells: impl IntoIterator<Item = BlockPos>) {
        for cell in cells {
            grid.mutator().place(cell, 1, None);
        }
    }

    /// Ground whose top blocks are at y = 0 over x in `0..width`, z in `0..depth`; the cell
    /// above it, at y = 1, is where a pawn's feet go.
    fn ground(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        put(
            &mut grid,
            (0..width).flat_map(|x| (0..depth).map(move |z| at(x, 0, z))),
        );
        grid
    }

    fn walk(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |cell| cell == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        let cases = [
            (Vec3::new(0.5, 1.0, 0.5), at(0, 1, 0)),
            (Vec3::new(3.99, 7.2, 2.0), at(3, 7, 2)),
            (Vec3::new(-0.5, 0.0, -1.0), at(-1, 0, -1)),
            (Vec3::new(-3.01, -0.01, 9.9), at(-4, -1, 9)),
        ];

        for (feet, expected) in cases {
            assert_eq!(feet_cell(feet), expected, "feet {feet:?}");
        }
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let open = ground(6, 6);
        let capped_at = |y: i32| {
            let mut grid = ground(6, 6);
            put(&mut grid, [at(2, y, 2)]);
            standable(&grid, at(2, 1, 2))
        };

        assert!(standable(&open, at(2, 1, 2)), "ground below, air above");
        assert!(!standable(&open, at(2, 2, 2)), "nothing to stand on");
        assert!(!standable(&open, at(2, 0, 2)), "inside the ground");
        assert!(!standable(&open, at(2, 1, 9)), "no ground at all");
        assert!(!capped_at(1), "a block in the feet cell");
        assert!(!capped_at(2), "a block at the knees");
        assert!(
            !capped_at(3),
            "a block where the head is: the capsule is 2.3 m tall"
        );
        assert!(
            capped_at(4),
            "the fourth cell up is not part of the capsule"
        );
        assert!(capped_at(5), "a ceiling well above");
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = ground(9, 3);

        let route = walk(&grid, at(0, 1, 1), at(5, 1, 1));

        assert_eq!(
            route,
            Some(vec![
                at(1, 1, 1),
                at(2, 1, 1),
                at(3, 1, 1),
                at(4, 1, 1),
                at(5, 1, 1)
            ])
        );
    }

    #[test]
    fn a_route_from_a_goal_cell_is_empty() {
        let grid = ground(4, 4);

        let route = walk(&grid, at(1, 1, 1), at(1, 1, 1));

        assert_eq!(route, Some(Vec::new()));
    }

    #[test]
    fn a_route_goes_to_the_nearest_cell_the_goal_accepts() {
        let grid = ground(12, 3);

        let route = route(
            &grid,
            at(0, 1, 1),
            |cell| cell.x >= 4,
            at(11, 1, 1),
            ROUTE_NODE_BUDGET,
        );

        assert_eq!(route.map(|r| r.last().copied()), Some(Some(at(4, 1, 1))));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = ground(9, 1);
        put(&mut grid, [at(3, 1, 0), at(4, 1, 0), at(5, 1, 0)]);

        let route = walk(&grid, at(0, 1, 0), at(8, 1, 0));

        assert_eq!(
            route,
            Some(vec![
                at(1, 1, 0),
                at(2, 1, 0),
                at(3, 2, 0),
                at(4, 2, 0),
                at(5, 2, 0),
                at(6, 1, 0),
                at(7, 1, 0),
                at(8, 1, 0)
            ])
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut grid = ground(8, 1);
        put(&mut grid, [at(3, 1, 0), at(3, 2, 0)]);

        let route = walk(&grid, at(0, 1, 0), at(6, 1, 0));

        assert_eq!(route, None);
    }

    #[test]
    fn a_route_goes_around_a_wall_when_there_is_a_way() {
        let mut grid = ground(8, 3);
        put(
            &mut grid,
            [at(3, 1, 0), at(3, 2, 0), at(3, 1, 1), at(3, 2, 1)],
        );

        let route = walk(&grid, at(0, 1, 0), at(6, 1, 0)).expect("the gap at z = 2 is open");

        assert!(route.contains(&at(3, 1, 2)), "through the gap: {route:?}");
        assert_eq!(
            route.len(),
            6 + 2 * 2,
            "the detour costs two steps each way"
        );
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut open = ground(8, 1);
        put(&mut open, (3..8).map(|x| at(x, 1, 0)));
        let mut capped = ground(8, 1);
        put(&mut capped, (3..8).map(|x| at(x, 1, 0)));
        put(&mut capped, [at(2, 4, 0)]);

        let free = walk(&open, at(2, 1, 0), at(4, 2, 0));
        let blocked = walk(&capped, at(2, 1, 0), at(4, 2, 0));

        assert_eq!(free, Some(vec![at(3, 2, 0), at(4, 2, 0)]));
        assert!(
            standable(&capped, at(2, 1, 0)),
            "the pawn fits where it stands"
        );
        assert_eq!(
            blocked, None,
            "a block over the feet leaves no room to step up"
        );
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = ground(7, 7);
        put(
            &mut grid,
            [(2, 3), (4, 3), (3, 2), (3, 4)]
                .into_iter()
                .flat_map(|(x, z)| [at(x, 1, z), at(x, 2, z)]),
        );

        let route = walk(&grid, at(0, 1, 0), at(3, 1, 3));

        assert!(standable(&grid, at(3, 1, 3)), "the goal itself is open");
        assert_eq!(route, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = ground(40, 3);
        let goal = at(30, 1, 1);

        let starved = route(&grid, at(0, 1, 1), |cell| cell == goal, goal, 10);
        let fed = route(&grid, at(0, 1, 1), |cell| cell == goal, goal, 10_000);

        assert_eq!(starved, None, "30 steps cannot be found in 10 expansions");
        assert_eq!(fed.map(|r| r.len()), Some(30));
    }

    #[test]
    fn a_start_on_a_slope_tries_the_cells_above_and_below() {
        let grid = ground(6, 1);

        let from_air = walk(&grid, at(0, 2, 0), at(3, 1, 0));
        let from_inside = walk(&grid, at(0, 0, 0), at(3, 1, 0));
        let too_high = walk(&grid, at(0, 5, 0), at(3, 1, 0));

        for (name, found) in [("above", from_air), ("below", from_inside)] {
            let cells =
                found.unwrap_or_else(|| panic!("a start one cell {name} the standing cell"));
            assert_eq!(cells.last(), Some(&at(3, 1, 0)), "{name}");
            assert!(cells.iter().all(|&cell| standable(&grid, cell)), "{name}");
        }
        assert_eq!(too_high, None, "only one cell either way is tried");
    }

    /// Ground whose column `(x, z)` is `heights[x * 6 + z]` blocks tall (1 to 3), with a pawn
    /// standing on top of each column.
    fn terrain(heights: &[u8]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..6 {
            for z in 0..6 {
                let height = i32::from(heights[(x * 6 + z) as usize]);
                put(&mut grid, (0..height).map(|y| at(x, y, z)));
            }
        }
        grid
    }

    fn standing_on(heights: &[u8], x: i32, z: i32) -> BlockPos {
        at(x, i32::from(heights[(x * 6 + z) as usize]), z)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(1u8..=2, 36),
            from in (0i32..6, 0i32..6),
            to in (0i32..6, 0i32..6),
        ) {
            let grid = terrain(&heights);
            let start = standing_on(&heights, from.0, from.1);
            let goal = standing_on(&heights, to.0, to.1);

            let found = walk(&grid, start, goal);

            prop_assert!(found.is_some(), "heights of one or two blocks always connect");
            let cells = found.unwrap_or_default();
            prop_assert_eq!(cells.last().copied().unwrap_or(start), goal);
            let mut previous = start;
            for cell in cells {
                prop_assert!(standable(&grid, cell), "{cell:?} is not standable");
                let step = cell - previous;
                prop_assert!(
                    step.x.abs() + step.z.abs() == 1 && step.y.abs() <= 1,
                    "{previous:?} to {cell:?} is not one horizontal step and at most one block up or down"
                );
                previous = cell;
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(1u8..=3, 36),
            from in (0i32..6, 0i32..6),
            to in (0i32..6, 0i32..6),
        ) {
            let start = standing_on(&heights, from.0, from.1);
            let goal = standing_on(&heights, to.0, to.1);
            let forward = terrain(&heights);
            let mut rebuilt = VoxelGrid::new(16);
            let mut every_block: Vec<BlockPos> = (0..6)
                .flat_map(|x| (0..6).map(move |z| (x, z)))
                .flat_map(|(x, z)| (0..i32::from(heights[(x * 6 + z) as usize])).map(move |y| at(x, y, z)))
                .collect();
            every_block.reverse();
            put(&mut rebuilt, every_block);

            let first = walk(&forward, start, goal);
            let again = walk(&forward, start, goal);
            let from_reordered_build = walk(&rebuilt, start, goal);

            prop_assert_eq!(&first, &again);
            prop_assert_eq!(&first, &from_reordered_build);
        }
    }
}
