//! Where a pawn can stand and how it walks between standing cells.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Cells a search may expand before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn's feet can be in `cell`: ground below (`cell - Y` solid) and three cells of
/// air from the feet up, the capsule's height.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y)
        && (0..3).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell holding `feet`, flooring each axis.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

/// A shortest walk over standable cells from `start` to the first cell `is_goal` accepts, or
/// `None` when there is none or the search expands more than `budget` cells.
///
/// Moves are the four horizontal neighbours, one block up or down; `toward` steers the
/// Manhattan heuristic.
///
/// The route lists the cells to walk through and leaves out the cell the walk starts in; a
/// start that is already a goal gives an empty route. A start that is not standable tries the
/// cell above it, then the one below, before giving up.
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
    let heuristic = |cell: BlockPos| ((cell.x - toward.x).abs() + (cell.z - toward.z).abs()) as u32;

    let mut open = BinaryHeap::new();
    let mut best: HashMap<BlockPos, u32> = HashMap::from([(start, 0)]);
    let mut parent: HashMap<BlockPos, BlockPos> = HashMap::new();
    let mut inserted = 0_u64;
    open.push(Reverse((
        heuristic(start),
        Reverse(0_u32),
        inserted,
        start.to_array(),
    )));
    let mut expanded = 0;
    while let Some(Reverse((_, Reverse(g), _, cell))) = open.pop() {
        let cell = BlockPos::from_array(cell);
        if best.get(&cell).is_some_and(|&known| known < g) {
            continue;
        }
        if is_goal(cell) {
            let mut walk = vec![cell];
            while let Some(&previous) = parent.get(walk.last().expect("walk is not empty")) {
                walk.push(previous);
            }
            walk.pop();
            walk.reverse();
            return Some(walk);
        }
        if expanded == budget {
            return None;
        }
        expanded += 1;
        for next in neighbours(grid, cell) {
            let cost = g + 1;
            if best.get(&next).is_none_or(|&known| cost < known) {
                best.insert(next, cost);
                parent.insert(next, cell);
                inserted += 1;
                open.push(Reverse((
                    cost + heuristic(next),
                    Reverse(cost),
                    inserted,
                    next.to_array(),
                )));
            }
        }
    }
    None
}

/// The standable cells one step from `cell`: level, up one block (which needs a fourth cell of
/// air above the feet) or down one block (which needs the head to clear the edge).
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    const SIDES: [BlockPos; 4] = [
        BlockPos::new(1, 0, 0),
        BlockPos::new(-1, 0, 0),
        BlockPos::new(0, 0, 1),
        BlockPos::new(0, 0, -1),
    ];
    let can_climb = !grid.is_solid_at(cell + BlockPos::Y * 3);
    SIDES.into_iter().flat_map(move |side| {
        let level = cell + side;
        let up = level + BlockPos::Y;
        let down = level - BlockPos::Y;
        let steps = [
            standable(grid, level).then_some(level),
            (can_climb && standable(grid, up)).then_some(up),
            (standable(grid, down) && !grid.is_solid_at(level + BlockPos::Y * 2)).then_some(down),
        ];
        steps.into_iter().flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const WIDE: i32 = 12;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// Solid ground at `y = 0` over `0..size` in x and z.
    fn ground(size: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(size - 1, 0, size - 1), 1);
        grid
    }

    fn fill(grid: &mut VoxelGrid, min: BlockPos, max: BlockPos) {
        grid.mutator().fill_region(min, max, 1);
    }

    /// The cells to walk through: `route` without a leading `start`, which a route may or may
    /// not repeat.
    fn walked(route: Vec<BlockPos>, start: BlockPos) -> Vec<BlockPos> {
        match route.first() {
            Some(&first) if first == start => route[1..].to_vec(),
            _ => route,
        }
    }

    fn route_to(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn feet_cell_floors_each_axis() {
        let cases = [
            (Vec3::new(1.5, 1.0, 2.5), cell(1, 1, 2)),
            (Vec3::new(0.0, 0.0, 0.0), cell(0, 0, 0)),
            (Vec3::new(-0.1, 0.99, -1.0), cell(-1, 0, -1)),
            (Vec3::new(3.999, 4.0, -0.001), cell(3, 4, -1)),
        ];

        for (feet, expected) in cases {
            assert_eq!(feet_cell(feet), expected, "feet {feet:?}");
        }
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let open = ground(8);
        let mut inside_a_block = ground(8);
        fill(&mut inside_a_block, cell(3, 1, 3), cell(3, 1, 3));

        assert!(standable(&open, cell(3, 1, 3)), "ground below, air above");
        assert!(!standable(&open, cell(3, 0, 3)), "the cell is the ground");
        assert!(!standable(&open, cell(3, 2, 3)), "air below");
        assert!(!standable(&open, cell(3, 5, 3)), "high in the air");
        assert!(!standable(&VoxelGrid::new(16), cell(3, 1, 3)), "no ground");
        assert!(
            !standable(&inside_a_block, cell(3, 1, 3)),
            "solid feet cell"
        );
        for (ceiling_y, expected) in [(4, true), (3, false), (2, false), (1, false)] {
            let mut grid = ground(8);
            fill(&mut grid, cell(3, ceiling_y, 3), cell(3, ceiling_y, 3));

            assert_eq!(
                standable(&grid, cell(3, 1, 3)),
                expected,
                "a block at height {ceiling_y} over feet at 1"
            );
        }
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = ground(WIDE);
        let start = cell(1, 1, 5);
        let goal = cell(8, 1, 5);

        let route = route_to(&grid, start, goal).expect("open ground has a route");

        let expected: Vec<BlockPos> = (2..=8).map(|x| cell(x, 1, 5)).collect();
        assert_eq!(walked(route, start), expected);
    }

    #[test]
    fn a_start_on_the_goal_needs_no_walking() {
        let grid = ground(WIDE);
        let start = cell(4, 1, 4);

        let route = route_to(&grid, start, start).expect("already there");

        assert!(walked(route, start).is_empty());
    }

    /// A one-block plateau across every row at `x` in `from..=to`.
    fn plateau(from: i32, to: i32) -> VoxelGrid {
        let mut grid = ground(WIDE);
        fill(&mut grid, cell(from, 1, 0), cell(to, 1, WIDE - 1));
        grid
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let grid = plateau(4, 5);
        let start = cell(1, 1, 5);
        let goal = cell(8, 1, 5);

        let route = route_to(&grid, start, goal).expect("a rise of one block is walkable");

        let heights: Vec<(i32, i32)> = walked(route, start).iter().map(|c| (c.x, c.y)).collect();
        assert_eq!(
            heights,
            [(2, 1), (3, 1), (4, 2), (5, 2), (6, 1), (7, 1), (8, 1)]
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let mut wall = ground(WIDE);
        fill(&mut wall, cell(4, 1, 0), cell(4, 2, WIDE - 1));
        let step = plateau(4, 4);

        let blocked = route_to(&wall, cell(1, 1, 5), cell(8, 1, 5));
        let climbable = route_to(&step, cell(1, 1, 5), cell(8, 1, 5));

        assert_eq!(blocked, None);
        assert!(climbable.is_some(), "control: one block high is a step");
    }

    #[test]
    fn climbing_needs_headroom() {
        let open = plateau(4, 5);
        let mut low_ceiling = plateau(4, 5);
        fill(&mut low_ceiling, cell(3, 4, 0), cell(3, 4, WIDE - 1));
        let (start, goal) = (cell(1, 1, 5), cell(8, 1, 5));

        assert!(
            standable(&low_ceiling, cell(3, 1, 5)),
            "the cell under the ceiling still holds the capsule"
        );
        assert!(route_to(&open, start, goal).is_some(), "control: open sky");
        assert_eq!(route_to(&low_ceiling, start, goal), None);
    }

    #[test]
    fn a_start_one_block_off_the_ground_still_routes() {
        let grid = ground(WIDE);
        let goal = cell(6, 1, 5);

        for (offset, routes) in [(1, true), (2, false), (-1, true), (-2, false)] {
            let route = route_to(&grid, cell(2, 1 + offset, 5), goal);

            assert_eq!(
                route.is_some(),
                routes,
                "a start {offset} blocks off the ground"
            );
        }
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = ground(WIDE);
        let goal = cell(6, 1, 6);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            fill(&mut grid, cell(6 + dx, 1, 6 + dz), cell(6 + dx, 2, 6 + dz));
        }

        let route = route_to(&grid, cell(1, 1, 1), goal);

        assert_eq!(route, None);
        let mut gapped = ground(WIDE);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1)] {
            fill(
                &mut gapped,
                cell(6 + dx, 1, 6 + dz),
                cell(6 + dx, 2, 6 + dz),
            );
        }
        assert!(
            route_to(&gapped, cell(1, 1, 1), goal).is_some(),
            "control: one open side is a way in"
        );
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = ground(40);
        let (start, goal) = (cell(1, 1, 1), cell(35, 1, 30));

        let starved = route(&grid, start, |c| c == goal, goal, 10);
        let funded = route(&grid, start, |c| c == goal, goal, ROUTE_NODE_BUDGET);

        assert_eq!(starved, None, "ten expansions cannot cross the field");
        assert!(funded.is_some(), "the full budget can");
    }

    /// Ground over an 8 x 8 field, `heights[x * 8 + z]` high: the top block at `y = height`.
    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..8 {
            for z in 0..8 {
                let height = heights[(x * 8 + z) as usize];
                fill(&mut grid, cell(x, 0, z), cell(x, height, z));
            }
        }
        grid
    }

    fn feet_above(heights: &[i32], (x, z): (i32, i32)) -> BlockPos {
        cell(x, heights[(x * 8 + z) as usize] + 1, z)
    }

    fn column() -> impl Strategy<Value = (i32, i32)> {
        (0..8i32, 0..8i32)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(0..=1i32, 64),
            from in column(),
            to in column(),
        ) {
            let grid = terrain(&heights);
            let (start, goal) = (feet_above(&heights, from), feet_above(&heights, to));

            let route = route_to(&grid, start, goal);

            prop_assert!(route.is_some(), "steps of one block never cut the field");
            let mut previous = start;
            for next in walked(route.unwrap(), start) {
                prop_assert!(standable(&grid, next), "{next} is not standable");
                let (dx, dy, dz) = (next.x - previous.x, next.y - previous.y, next.z - previous.z);
                prop_assert_eq!(dx.abs() + dz.abs(), 1, "{} to {} is not a neighbour", previous, next);
                prop_assert!(dy.abs() <= 1, "{previous} to {next} climbs more than a block");
                previous = next;
            }
            prop_assert_eq!(previous, goal, "the route ends at the goal");
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(0..=1i32, 64),
            from in column(),
            to in column(),
        ) {
            let grid = terrain(&heights);
            let (start, goal) = (feet_above(&heights, from), feet_above(&heights, to));

            let first = route_to(&grid, start, goal);
            let second = route_to(&grid, start, goal);

            prop_assert!(first.is_some(), "there is a route to compare");
            prop_assert_eq!(first, second);
        }

        #[test]
        fn a_route_is_as_short_as_the_manhattan_distance(
            heights in proptest::collection::vec(0..=1i32, 64),
            from in column(),
            to in column(),
        ) {
            let grid = terrain(&heights);
            let (start, goal) = (feet_above(&heights, from), feet_above(&heights, to));

            let route = route_to(&grid, start, goal);

            prop_assert!(route.is_some(), "steps of one block never cut the field");
            let steps = walked(route.unwrap(), start).len() as i32;
            prop_assert_eq!(steps, (from.0 - to.0).abs() + (from.1 - to.1).abs());
        }
    }
}
