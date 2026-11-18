//! Routes over the voxel ground: which cells a pawn can stand on and the way between them.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

/// Cells a search may expand before it gives up.
pub const ROUTE_NODE_BUDGET: usize = 4096;

/// Whether a pawn can stand with its feet in `cell`: ground below and three cells of air
/// from the feet up (the capsule is 2.3 m tall).
#[expect(clippy::todo, reason = "stub until implemented")]
pub fn standable(_grid: &VoxelGrid, _cell: BlockPos) -> bool {
    todo!()
}

/// The cell holding `feet`, flooring each axis.
#[expect(clippy::todo, reason = "stub until implemented")]
pub fn feet_cell(_feet: Vec3) -> BlockPos {
    todo!()
}

/// The cells to walk from `start` to the nearest cell satisfying `is_goal`, excluding
/// `start` and ending on the goal cell; empty when `start` is itself a goal. `None` when
/// no goal is reachable within `budget` expansions. `toward` steers the heuristic.
#[expect(clippy::todo, reason = "stub until implemented")]
pub fn route(
    _grid: &VoxelGrid,
    _start: BlockPos,
    _is_goal: impl Fn(BlockPos) -> bool,
    _toward: BlockPos,
    _budget: usize,
) -> Option<Vec<BlockPos>> {
    todo!()
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
    }

    #[test]
    fn standable_rejects_a_block_at_each_of_the_three_air_cells() {
        for y in 1..=3 {
            let mut grid = VoxelGrid::new(16);
            grid.mutator().place(cell(0, 0, 0), GROUND, None);
            grid.mutator().place(cell(0, y, 0), GROUND, None);

            assert!(!standable(&grid, cell(0, 1, 0)), "block at y = {y}");
        }
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
    fn the_search_stops_at_the_nearest_of_several_goals() {
        let grid = flat(10, 0);

        let found = route(
            &grid,
            cell(5, 1, 0),
            |c| c == cell(1, 1, 0) || c == cell(9, 1, 0),
            cell(1, 1, 0),
            ROUTE_NODE_BUDGET,
        );

        assert_eq!(found.map(|r| r.last().copied()), Some(Some(cell(9, 1, 0))));
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

    proptest! {
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
