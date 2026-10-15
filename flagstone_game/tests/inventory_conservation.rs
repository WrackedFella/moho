use flagstone_game::pawn::{HOTBAR_SLOT_COUNT, Pawn};
use flagstone_voxel::VoxelGrid;
use glam::{IVec3, Vec3};
use proptest::prelude::*;

const RESOURCE_KINDS: u32 = 4;
const CUBE_MIN: i32 = 2;
const CUBE_SIZE: i32 = 6;
const CELLS: usize = (CUBE_SIZE * CUBE_SIZE * CUBE_SIZE) as usize;

#[derive(Clone, Debug)]
enum Op {
    /// Aims along an axis at the solid cube from outside, so most of these hit.
    Aimed { axis: usize, u: f32, v: f32 },
    /// Arbitrary origin, direction and range; mostly misses or hits odd spots.
    Free {
        origin: [f32; 3],
        dir: [f32; 3],
        max_distance: f32,
    },
    /// Slots run past `HOTBAR_SLOT_COUNT` to cover the ignored range.
    SelectSlot(usize),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let span = (CUBE_MIN as f32 + 0.25)..(CUBE_MIN as f32 + CUBE_SIZE as f32 - 0.25);
    prop_oneof![
        (0usize..6, span.clone(), span).prop_map(|(axis, u, v)| Op::Aimed { axis, u, v }),
        (
            prop::array::uniform3(-3.0f32..11.0),
            prop::array::uniform3(-1.0f32..1.0),
            0.0f32..30.0,
        )
            .prop_map(|(origin, dir, max_distance)| Op::Free {
                origin,
                dir,
                max_distance
            }),
        (0usize..HOTBAR_SLOT_COUNT * 2 + 2).prop_map(Op::SelectSlot),
    ]
}

fn aimed_ray(axis: usize, u: f32, v: f32) -> (Vec3, Vec3) {
    let positive = axis.is_multiple_of(2);
    let (start, sign) = if positive { (-2.0, 1.0) } else { (12.0, -1.0) };
    match axis / 2 {
        0 => (Vec3::new(start, u, v), Vec3::X * sign),
        1 => (Vec3::new(u, start, v), Vec3::Y * sign),
        _ => (Vec3::new(u, v, start), Vec3::Z * sign),
    }
}

fn cube_positions() -> impl Iterator<Item = IVec3> {
    (CUBE_MIN..CUBE_MIN + CUBE_SIZE).flat_map(|x| {
        (CUBE_MIN..CUBE_MIN + CUBE_SIZE)
            .flat_map(move |y| (CUBE_MIN..CUBE_MIN + CUBE_SIZE).map(move |z| IVec3::new(x, y, z)))
    })
}

fn build_grid(resources: &[Option<u32>]) -> VoxelGrid {
    let mut grid = VoxelGrid::new(16);
    for (pos, resource) in cube_positions().zip(resources) {
        grid.mutator().place(pos, 1, *resource);
    }
    grid
}

fn world_count(grid: &VoxelGrid, resource_id: u32) -> usize {
    cube_positions()
        .filter(|&pos| {
            grid.material_at(pos).is_some() && grid.resource_at(pos) == Some(resource_id)
        })
        .count()
}

fn assert_conserved(pawn: &Pawn, grid: &VoxelGrid, starting: &[usize]) {
    for (id, &start) in (0u32..).zip(starting) {
        let held = pawn.inventory.count(id) as usize;
        assert_eq!(
            held + world_count(grid, id),
            start,
            "resource {id}: {held} held"
        );
    }
    assert!(
        pawn.inventory.iter().all(|(id, _)| id < RESOURCE_KINDS),
        "inventory holds a resource that was never placed"
    );
}

proptest! {
    #[test]
    fn mining_conserves_resources_between_world_and_inventory(
        resources in prop::collection::vec(prop::option::of(0..RESOURCE_KINDS), CELLS),
        ops in prop::collection::vec(op_strategy(), 0..12),
    ) {
        let mut grid = build_grid(&resources);
        let starting: Vec<usize> = (0..RESOURCE_KINDS).map(|id| world_count(&grid, id)).collect();
        let mut pawn = Pawn::default();
        assert_conserved(&pawn, &grid, &starting);

        let primed = pawn.mine(&mut grid, Vec3::new(4.5, 4.5, -2.0), Vec3::Z, 40.0);

        prop_assert!(primed.is_some(), "priming mine into the solid cube must hit");
        assert_conserved(&pawn, &grid, &starting);

        for op in ops {
            match op {
                Op::Aimed { axis, u, v } => {
                    let (origin, dir) = aimed_ray(axis, u, v);
                    pawn.mine(&mut grid, origin, dir, 40.0);
                }
                Op::Free { origin, dir, max_distance } => {
                    pawn.mine(&mut grid, Vec3::from_array(origin), Vec3::from_array(dir), max_distance);
                }
                Op::SelectSlot(slot) => pawn.select_slot(slot),
            }

            assert_conserved(&pawn, &grid, &starting);
        }
    }
}

#[test]
fn mining_a_resource_block_moves_exactly_one_unit_into_the_inventory() {
    let mut resources = vec![None; CELLS];
    resources[0] = Some(2);
    let mut grid = build_grid(&resources);
    let mut pawn = Pawn::default();

    let outcome = pawn.mine(&mut grid, Vec3::new(4.5, 4.5, -2.0), Vec3::Z, 40.0);

    assert!(outcome.is_some());
    assert_eq!(world_count(&grid, 2) + pawn.inventory.count(2) as usize, 1);
}
