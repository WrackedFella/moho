//! Walking routes over the voxel ground: where a pawn can stand and how it gets from one
//! standing cell to another.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Most cells a search expands before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn's feet can be in `cell`: ground below and three cells of air from the feet up.
pub fn standable(grid: &VoxelGrid, cell: BlockPos) -> bool {
    grid.is_solid_at(cell - BlockPos::Y) && clear_above(grid, cell, 3)
}

/// Whether the `count` cells from `cell` upward are air.
fn clear_above(grid: &VoxelGrid, cell: BlockPos, count: i32) -> bool {
    (0..count).all(|up| !grid.is_solid_at(cell + BlockPos::Y * up))
}

/// The cell holding `feet`.
pub fn feet_cell(feet: Vec3) -> BlockPos {
    feet.floor().as_ivec3()
}

const HORIZONTAL: [BlockPos; 4] = [BlockPos::X, BlockPos::NEG_X, BlockPos::Z, BlockPos::NEG_Z];

/// The standable cells one horizontal step from `cell`, at the same height or one up or down.
fn neighbours(grid: &VoxelGrid, cell: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    let can_climb = clear_above(grid, cell, 4);
    HORIZONTAL.into_iter().filter_map(move |step| {
        let side = cell + step;
        [
            Some(side),
            can_climb.then_some(side + BlockPos::Y),
            Some(side - BlockPos::Y),
        ]
        .into_iter()
        .flatten()
        .find(|&candidate| standable(grid, candidate))
    })
}

fn manhattan(a: BlockPos, b: BlockPos) -> i32 {
    let delta = (a - b).abs();
    delta.x + delta.y + delta.z
}

/// The cells to walk through, not including `start`, from `start` to the nearest cell
/// `is_goal` accepts; `None` when none is reachable within `budget` expansions.
///
/// A `start` that is not standable falls back to the cell above or below it. `toward`
/// only orders the search; it does not decide which goal cell is accepted.
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

    // Ordered by (f, g, insertion order) so equal-cost routes resolve the same way every time.
    let mut open = BinaryHeap::new();
    let mut best: HashMap<BlockPos, (i32, Option<BlockPos>)> = HashMap::new();
    let mut inserted = 0_u32;
    best.insert(start, (0, None));
    open.push(Reverse((
        manhattan(start, toward),
        0,
        inserted,
        start.to_array(),
    )));

    let mut expansions = 0;
    while let Some(Reverse((_, g, _, cell))) = open.pop() {
        let cell = BlockPos::from_array(cell);
        if best.get(&cell).is_some_and(|&(known, _)| g > known) {
            continue;
        }
        if is_goal(cell) {
            return Some(walk_back(&best, cell));
        }
        expansions += 1;
        if expansions > budget {
            return None;
        }
        for next in neighbours(grid, cell) {
            let next_g = g + 1;
            if best.get(&next).is_some_and(|&(known, _)| known <= next_g) {
                continue;
            }
            best.insert(next, (next_g, Some(cell)));
            inserted += 1;
            open.push(Reverse((
                next_g + manhattan(next, toward),
                next_g,
                inserted,
                next.to_array(),
            )));
        }
    }
    None
}

/// The cells from the start (excluded) to `end`, following the recorded parents.
fn walk_back(best: &HashMap<BlockPos, (i32, Option<BlockPos>)>, end: BlockPos) -> Vec<BlockPos> {
    let mut cells = Vec::new();
    let mut cell = end;
    while let Some(&(_, Some(parent))) = best.get(&cell) {
        cells.push(cell);
        cell = parent;
    }
    cells.reverse();
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::ops::RangeInclusive;

    fn fill(
        grid: &mut VoxelGrid,
        xs: RangeInclusive<i32>,
        ys: RangeInclusive<i32>,
        zs: RangeInclusive<i32>,
    ) {
        for x in xs {
            for y in ys.clone() {
                for z in zs.clone() {
                    grid.mutator().place(BlockPos::new(x, y, z), 1, None);
                }
            }
        }
    }

    /// A floor one block thick, its top face at y = 1, over `0..width` by `0..depth`.
    fn floor(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        fill(&mut grid, 0..=width - 1, 0..=0, 0..=depth - 1);
        grid
    }

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn route_to(grid: &VoxelGrid, start: BlockPos, goal: BlockPos) -> Option<Vec<BlockPos>> {
        route(grid, start, |cell| cell == goal, goal, ROUTE_NODE_BUDGET)
    }

    #[test]
    fn standable_needs_ground_below_and_the_capsules_height_of_air() {
        let grid = floor(5, 5);
        let mut blocked_at_feet = floor(5, 5);
        fill(&mut blocked_at_feet, 2..=2, 1..=1, 2..=2);
        let mut blocked_at_the_head = floor(5, 5);
        fill(&mut blocked_at_the_head, 2..=2, 3..=3, 2..=2);
        let mut ceiling_above_the_capsule = floor(5, 5);
        fill(&mut ceiling_above_the_capsule, 2..=2, 4..=4, 2..=2);

        assert!(standable(&grid, at(2, 1, 2)), "open ground");
        assert!(!standable(&grid, at(2, 2, 2)), "air below");
        assert!(!standable(&grid, at(2, 0, 2)), "inside the floor");
        assert!(!standable(&grid, at(9, 1, 9)), "no ground there");
        assert!(
            !standable(&blocked_at_feet, at(2, 1, 2)),
            "a block at the feet"
        );
        assert!(
            !standable(&blocked_at_the_head, at(2, 1, 2)),
            "the third cell up is the capsule's top"
        );
        assert!(
            standable(&ceiling_above_the_capsule, at(2, 1, 2)),
            "the fourth cell up is clear of the capsule"
        );
    }

    #[test]
    fn feet_cell_floors_the_position() {
        assert_eq!(feet_cell(Vec3::new(2.9, 1.0, -0.1)), at(2, 1, -1));
        assert_eq!(feet_cell(Vec3::new(-0.5, 4.2, 7.99)), at(-1, 4, 7));
    }

    #[test]
    fn a_route_on_flat_ground_is_a_straight_line() {
        let grid = floor(10, 3);

        let path = route_to(&grid, at(1, 1, 1), at(6, 1, 1));

        assert_eq!(
            path,
            Some(vec![
                at(2, 1, 1),
                at(3, 1, 1),
                at(4, 1, 1),
                at(5, 1, 1),
                at(6, 1, 1)
            ])
        );
    }

    #[test]
    fn a_route_from_a_goal_cell_has_nothing_to_walk() {
        let grid = floor(5, 5);

        let path = route_to(&grid, at(2, 1, 2), at(2, 1, 2));

        assert_eq!(path, Some(Vec::new()));
    }

    #[test]
    fn a_route_climbs_a_one_block_rise_and_descends_it() {
        let mut grid = floor(12, 3);
        fill(&mut grid, 4..=6, 1..=1, 0..=2);

        let path = route_to(&grid, at(1, 1, 1), at(9, 1, 1));

        assert_eq!(
            path,
            Some(vec![
                at(2, 1, 1),
                at(3, 1, 1),
                at(4, 2, 1),
                at(5, 2, 1),
                at(6, 2, 1),
                at(7, 1, 1),
                at(8, 1, 1),
                at(9, 1, 1),
            ])
        );
    }

    #[test]
    fn a_two_block_wall_blocks_the_route() {
        let open = floor(10, 3);
        let mut walled = floor(10, 3);
        fill(&mut walled, 4..=4, 1..=2, 0..=2);

        let through = route_to(&open, at(1, 1, 1), at(8, 1, 1));
        let blocked = route_to(&walled, at(1, 1, 1), at(8, 1, 1));

        assert!(through.is_some(), "the way is open without the wall");
        assert_eq!(blocked, None);
    }

    #[test]
    fn climbing_needs_headroom() {
        let mut clear = floor(8, 3);
        fill(&mut clear, 4..=4, 1..=1, 0..=2);
        let mut low_ceiling = floor(8, 3);
        fill(&mut low_ceiling, 4..=4, 1..=1, 0..=2);
        fill(&mut low_ceiling, 3..=3, 4..=4, 0..=2);

        let with_headroom = route_to(&clear, at(1, 1, 1), at(6, 1, 1));
        let without = route_to(&low_ceiling, at(1, 1, 1), at(6, 1, 1));

        assert!(with_headroom.is_some(), "the step is climbable");
        assert!(
            standable(&low_ceiling, at(3, 1, 1)),
            "the foot of the step is still a place to stand"
        );
        assert_eq!(
            without, None,
            "no fourth air cell above the foot of the step"
        );
    }

    #[test]
    fn an_enclosed_goal_has_no_route() {
        let mut grid = floor(9, 9);
        fill(&mut grid, 3..=3, 1..=3, 3..=5);
        fill(&mut grid, 5..=5, 1..=3, 3..=5);
        fill(&mut grid, 4..=4, 1..=3, 3..=3);
        fill(&mut grid, 4..=4, 1..=3, 5..=5);

        let path = route_to(&grid, at(1, 1, 1), at(4, 1, 4));

        assert_eq!(path, None);
    }

    #[test]
    fn the_node_budget_ends_a_search_with_none() {
        let grid = floor(40, 40);
        let (start, goal) = (at(1, 1, 1), at(36, 1, 36));

        let starved = route(&grid, start, |cell| cell == goal, goal, 5);
        let funded = route_to(&grid, start, goal);

        assert_eq!(starved, None);
        assert_eq!(funded.expect("within the full budget").last(), Some(&goal));
        assert_eq!(ROUTE_NODE_BUDGET, 4096);
    }

    #[test]
    fn a_start_on_a_slope_tries_the_cells_above_and_below() {
        let grid = floor(10, 3);
        let goal = at(6, 1, 1);

        let from_inside_the_ground = route_to(&grid, at(1, 0, 1), goal);
        let from_the_air = route_to(&grid, at(1, 2, 1), goal);

        assert!(
            from_inside_the_ground.is_some(),
            "one below a standable cell"
        );
        assert!(from_the_air.is_some(), "one above a standable cell");
    }

    /// A heightfield over 8 by 8 columns: column `(x, z)` is solid from y = 0 up to its height.
    fn terrain(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (index, &height) in heights.iter().enumerate() {
            let (x, z) = (index as i32 % 8, index as i32 / 8);
            fill(&mut grid, x..=x, 0..=height, z..=z);
        }
        grid
    }

    fn standing_on(heights: &[i32], x: i32, z: i32) -> BlockPos {
        at(x, heights[(z * 8 + x) as usize] + 1, z)
    }

    proptest! {
        #[test]
        fn every_route_cell_is_standable_and_adjacent_to_the_previous(
            heights in proptest::collection::vec(0..3i32, 64),
            from in (0..8i32, 0..8i32),
            to in (0..8i32, 0..8i32),
        ) {
            let grid = terrain(&heights);
            let start = standing_on(&heights, from.0, from.1);
            let goal = standing_on(&heights, to.0, to.1);

            let path = route_to(&grid, start, goal);

            if let Some(path) = path {
                let mut previous = start;
                for &cell in &path {
                    let step = cell - previous;
                    prop_assert!(standable(&grid, cell), "{cell:?} is not standable");
                    prop_assert_eq!(step.x.abs() + step.z.abs(), 1, "{:?} to {:?}", previous, cell);
                    prop_assert!(step.y.abs() <= 1, "{previous:?} to {cell:?} is not one block");
                    previous = cell;
                }
                prop_assert_eq!(previous, goal, "the route ends at the goal");
            }
        }

        #[test]
        fn the_same_grid_gives_the_same_route(
            heights in proptest::collection::vec(0..2i32, 64),
            from in (0..8i32, 0..8i32),
            to in (0..8i32, 0..8i32),
        ) {
            let start = standing_on(&heights, from.0, from.1);
            let goal = standing_on(&heights, to.0, to.1);

            let first = route_to(&terrain(&heights), start, goal);
            let second = route_to(&terrain(&heights), start, goal);

            prop_assert_eq!(first, second);
        }

        #[test]
        fn open_ground_always_has_a_shortest_route(
            from in (0..8i32, 0..8i32),
            to in (0..8i32, 0..8i32),
        ) {
            let heights = [0; 64];
            let grid = terrain(&heights);
            let start = standing_on(&heights, from.0, from.1);
            let goal = standing_on(&heights, to.0, to.1);

            let path = route_to(&grid, start, goal).expect("flat ground connects every cell");

            let manhattan = (goal.x - start.x).abs() + (goal.z - start.z).abs();
            prop_assert_eq!(path.len() as i32, manhattan);
        }
    }
}
