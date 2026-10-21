//! Integration tests for the Stage 4C lighting system.
//!
//! Each test targets a specific invariant, regression risk, or algorithmic
//! property. Failing tests identify exactly which guarantee broke.

use flagstone_voxel::{
    LightPropagator, MaterialLighting, MaterialType, VoxelGrid, dirty_chunks_below,
    recompute_sky_exposure,
};
use glam::{IVec3, Vec3};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn grid_16() -> VoxelGrid {
    VoxelGrid::new(16)
}

/// Material 1: transparent, no emission.
fn make_transparent(grid: &mut VoxelGrid) {
    grid.material_registry.set_lighting(
        1,
        MaterialLighting {
            emission: [0, 0, 0],
            opacity_cost: 0,
        },
    );
}

/// Material 2: warm torch (R=15, G=8, B=2), transparent body.
fn make_torch(grid: &mut VoxelGrid) {
    grid.material_registry.set_lighting(
        2,
        MaterialLighting {
            emission: [15, 8, 2],
            opacity_cost: 0,
        },
    );
}

fn rgb(grid: &VoxelGrid, pos: IVec3) -> [u8; 3] {
    grid.block_light_rgb_at(pos)
}

fn sky(grid: &VoxelGrid, pos: IVec3) -> bool {
    grid.sky_exposed_at(pos)
}

// ---------------------------------------------------------------------------
// Sky-exposure invariants
// ---------------------------------------------------------------------------

/// Open air chunk (no blocks): every voxel must be sky-exposed.
#[test]
fn sky_open_air_is_fully_exposed() {
    let mut grid = grid_16();
    let cp = IVec3::ZERO;
    grid.chunk_light_mut(cp);
    recompute_sky_exposure(&mut grid, cp);

    let cl = grid.chunk_light(cp).unwrap();
    for idx in 0..4096usize {
        assert!(
            cl.sky_exposed_at(idx),
            "voxel {idx} in empty chunk should be sky-exposed"
        );
    }
}

/// A voxel directly below an opaque block in the same chunk is not sky-exposed.
#[test]
fn sky_occlusion_same_chunk() {
    let mut grid = grid_16();
    grid.mutator().place(IVec3::new(3, 8, 3), 0, None);
    recompute_sky_exposure(&mut grid, IVec3::ZERO);

    assert!(
        !sky(&grid, IVec3::new(3, 7, 3)),
        "below the opaque block must not be exposed"
    );
    assert!(
        sky(&grid, IVec3::new(3, 9, 3)),
        "above the opaque block must be exposed"
    );
    assert!(
        sky(&grid, IVec3::new(4, 7, 3)),
        "adjacent column must be exposed"
    );
}

/// A block in chunk y=1 occludes voxels in chunk y=0 in the same (x,z) column.
#[test]
fn sky_cross_chunk_occlusion() {
    let mut grid = grid_16();
    grid.mutator().place(IVec3::new(5, 20, 5), 0, None);

    recompute_sky_exposure(&mut grid, IVec3::new(0, 1, 0));
    recompute_sky_exposure(&mut grid, IVec3::ZERO);

    assert!(
        !sky(&grid, IVec3::new(5, 15, 5)),
        "directly below cross-chunk block: not exposed"
    );
    assert!(sky(&grid, IVec3::new(6, 15, 5)), "adjacent column: exposed");
}

#[test]
fn sky_recompute_reflects_removed_block() {
    let mut grid = grid_16();
    let block = IVec3::new(2, 10, 2);
    let below = IVec3::new(2, 9, 2);
    grid.mutator().place(block, 0, None);
    recompute_sky_exposure(&mut grid, IVec3::ZERO);
    assert!(!sky(&grid, below), "shadowed while the block stands");

    grid.mutator().remove(block);
    recompute_sky_exposure(&mut grid, IVec3::ZERO);

    assert!(sky(&grid, below), "exposed again after the block is gone");
}

/// Never dirties `chunk_pos` itself or other columns.
#[test]
fn sky_dirty_chunks_below_propagates() {
    let mut grid = grid_16();
    let above = IVec3::new(0, 1, 0);
    let below = IVec3::ZERO;
    let other_columns = [IVec3::new(1, 0, 0), IVec3::new(0, 0, 1)];
    grid.mutator().place(IVec3::new(0, 16, 0), 0, None);
    grid.mutator().place(IVec3::new(0, 0, 0), 0, None);
    grid.mutator().place(IVec3::new(16, 0, 0), 0, None);
    grid.mutator().place(IVec3::new(0, 0, 16), 0, None);
    for cp in [above, below].into_iter().chain(other_columns) {
        grid.chunk_light_mut(cp).sky_dirty = false;
    }

    dirty_chunks_below(&mut grid, above);

    let dirty = |cp| {
        grid.chunk_light(cp)
            .expect("chunk light allocated by placement")
            .sky_dirty
    };
    assert!(dirty(below), "chunk below should be sky_dirty");
    assert!(!dirty(above), "chunk at chunk_pos must stay clean");
    for cp in other_columns {
        assert!(!dirty(cp), "other column {cp:?} must stay clean");
    }
}

#[test]
fn sky_recompute_noop_when_clean() {
    let mut grid = grid_16();
    let cp = IVec3::ZERO;
    let below = IVec3::new(3, 7, 3);
    recompute_sky_exposure(&mut grid, cp);
    grid.mutator().place(IVec3::new(3, 8, 3), 0, None);
    grid.chunk_light_mut(cp).sky_dirty = false;

    recompute_sky_exposure(&mut grid, cp);

    assert!(sky(&grid, below), "a clean chunk keeps its stale exposure");
}

/// In any chunk layer the block's own voxel and everything below it in the
/// column are shadowed, everything above it is exposed.
#[test]
fn sky_occlusion_shadows_the_block_and_below_in_any_chunk_layer() {
    // (chunk, block position, probes as (position, exposed))
    let cases = [
        (
            IVec3::ZERO,
            IVec3::new(3, 8, 3),
            [
                (IVec3::new(3, 9, 3), true),
                (IVec3::new(3, 8, 3), false),
                (IVec3::new(3, 7, 3), false),
                (IVec3::new(4, 8, 3), true),
                (IVec3::new(3, 15, 3), true),
                (IVec3::new(3, 0, 3), false),
            ],
        ),
        (
            IVec3::new(0, 2, 0),
            IVec3::new(3, 34, 3),
            [
                (IVec3::new(3, 35, 3), true),
                (IVec3::new(3, 47, 3), true),
                (IVec3::new(3, 34, 3), false),
                (IVec3::new(3, 33, 3), false),
                (IVec3::new(3, 32, 3), false),
                (IVec3::new(4, 33, 3), true),
            ],
        ),
        (
            IVec3::new(0, -1, 0),
            IVec3::new(3, -10, 3),
            [
                (IVec3::new(3, -9, 3), true),
                (IVec3::new(3, -1, 3), true),
                (IVec3::new(3, -10, 3), false),
                (IVec3::new(3, -11, 3), false),
                (IVec3::new(3, -16, 3), false),
                (IVec3::new(4, -11, 3), true),
            ],
        ),
    ];

    for (chunk, block, probes) in cases {
        let mut grid = grid_16();
        grid.mutator().place(block, 0, None);

        recompute_sky_exposure(&mut grid, chunk);

        for (pos, exposed) in probes {
            assert_eq!(sky(&grid, pos), exposed, "chunk {chunk:?}, probe {pos:?}");
        }
    }
}

#[test]
fn sky_block_in_other_chunk_column_does_not_occlude() {
    // Opaque blocks in chunks (1,1,0) and (0,1,1), one x and one z neighbour.
    for (block, chunk) in [
        (IVec3::new(21, 20, 5), IVec3::new(1, 1, 0)),
        (IVec3::new(5, 20, 21), IVec3::new(0, 1, 1)),
    ] {
        let mut grid = grid_16();
        grid.mutator().place(block, 0, None);

        recompute_sky_exposure(&mut grid, chunk);
        recompute_sky_exposure(&mut grid, IVec3::ZERO);

        assert!(
            sky(&grid, IVec3::new(5, 15, 5)),
            "block in chunk {chunk:?} must not occlude column (0,*,0)"
        );
    }
}

// ---------------------------------------------------------------------------
// Block-light propagation invariants
// ---------------------------------------------------------------------------

#[test]
fn block_light_decay_one_per_step() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();
    let source = IVec3::new(5, 5, 5);
    let faces = [
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Y,
        IVec3::NEG_Y,
        IVec3::Z,
        IVec3::NEG_Z,
    ];

    for x in 5..13i32 {
        grid.mutator().place(IVec3::new(x, 5, 5), 1, None);
    }
    for face in faces {
        grid.mutator().place(source + face, 1, None);
    }
    prop.add_light_rgb(&mut grid, source, [15, 0, 0]);

    for step in 0..8i32 {
        let r = rgb(&grid, source + IVec3::new(step, 0, 0))[0];
        assert_eq!(
            r,
            (15 - step as u8),
            "step={step}: expected R={}, got {r}",
            15 - step
        );
    }
    for face in faces {
        assert_eq!(rgb(&grid, source + face)[0], 14, "face neighbour {face:?}");
    }
}

#[test]
fn weaker_source_does_not_dim_brighter() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    for x in 0..5i32 {
        let pos = IVec3::new(x, 0, 0);
        grid.mutator().place(pos, 1, None);
        grid.set_block_light_rgb(pos, [10, 0, 0]);
    }

    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [8, 0, 0]);

    for x in [0, 4] {
        assert_eq!(
            rgb(&grid, IVec3::new(x, 0, 0))[0],
            10,
            "existing R=10 at x={x} must not be reduced"
        );
    }
}

/// Positions outside any light read zero, whether or not their chunk has a
/// `ChunkLight` allocated.
#[test]
fn unlit_position_reads_zero() {
    let probe = IVec3::new(7, 7, 7);

    for allocated in [false, true] {
        let mut grid = grid_16();
        if allocated {
            make_transparent(&mut grid);
            let mut prop = LightPropagator::new();
            grid.mutator().place(IVec3::ZERO, 1, None);
            prop.add_light_rgb(&mut grid, IVec3::ZERO, [10, 0, 0]);
            assert!(grid.chunk_light(IVec3::ZERO).is_some());
        } else {
            assert!(grid.chunk_light(IVec3::ZERO).is_none());
        }

        assert_eq!(rgb(&grid, probe), [0, 0, 0], "allocated={allocated}");
    }
}

/// R, G, B propagate independently: emitting only R does not affect G or B.
#[test]
fn block_light_channels_are_independent() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    for x in 0..4i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 0, 0]);

    let nb = rgb(&grid, IVec3::new(1, 0, 0));
    assert!(nb[0] > 0, "R should propagate");
    assert_eq!(nb[1], 0, "G must stay 0");
    assert_eq!(nb[2], 0, "B must stay 0");
}

/// Decay clamps at zero.
#[test]
fn block_light_colored_propagation() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    for x in 0..5i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 8, 2]);

    assert_eq!(rgb(&grid, IVec3::new(1, 0, 0)), [14, 7, 1]);
    assert_eq!(rgb(&grid, IVec3::new(2, 0, 0)), [13, 6, 0]);
}

// ---------------------------------------------------------------------------
// Removal invariants
// ---------------------------------------------------------------------------

/// After removing the only source, all voxels go to 0.
#[test]
fn removal_single_source_clears_all() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    for x in 0..6i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    let src = IVec3::new(0, 0, 0);
    prop.add_light_rgb(&mut grid, src, [15, 0, 0]);
    prop.remove_light(&mut grid, src);

    for x in 0..6i32 {
        assert_eq!(
            rgb(&grid, IVec3::new(x, 0, 0))[0],
            0,
            "x={x} should be dark"
        );
    }
}

/// Removing one of two sources preserves the surviving source's illumination.
#[test]
fn removal_preserves_surviving_source() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    for x in 0..11i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 0, 0]);
    prop.add_light_rgb(&mut grid, IVec3::new(10, 0, 0), [15, 0, 0]);
    prop.remove_light(&mut grid, IVec3::new(0, 0, 0));

    // x=0 is 10 steps from light2 → R = 15 - 10 = 5.
    assert_eq!(
        rgb(&grid, IVec3::new(0, 0, 0))[0],
        5,
        "old source pos should have R=5 from light2"
    );
    // light2 itself unchanged.
    assert_eq!(rgb(&grid, IVec3::new(10, 0, 0))[0], 15);
}

#[test]
fn removal_of_absent_light_is_noop() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();
    for x in 0..3i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 0, 0]);

    let affected = prop.remove_light(&mut grid, IVec3::new(20, 0, 0));

    assert!(affected.is_empty(), "nothing to remove: {affected:?}");
    for (x, expected) in [(0, 15u8), (1, 14), (2, 13)] {
        assert_eq!(rgb(&grid, IVec3::new(x, 0, 0))[0], expected, "x={x}");
    }
}

#[test]
fn removal_with_adjacent_equal_sources_keeps_survivor() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();
    for x in 0..3i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 0, 0]);
    prop.add_light_rgb(&mut grid, IVec3::new(1, 0, 0), [15, 0, 0]);

    prop.remove_light(&mut grid, IVec3::new(0, 0, 0));

    for (x, expected) in [(0, 14u8), (1, 15), (2, 14)] {
        assert_eq!(rgb(&grid, IVec3::new(x, 0, 0))[0], expected, "x={x}");
    }
}

#[test]
fn removal_clears_light_along_every_axis() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    let center = IVec3::new(5, 5, 5);
    let arms: Vec<IVec3> = (1..5i32)
        .flat_map(|i| {
            [
                IVec3::new(i, 0, 0),
                IVec3::new(0, i, 0),
                IVec3::new(0, 0, i),
            ]
        })
        .map(|d| center + d)
        .collect();
    grid.mutator().place(center, 1, None);
    for &p in &arms {
        grid.mutator().place(p, 1, None);
    }
    prop.add_light_rgb(&mut grid, center, [15, 0, 0]);
    for &p in &arms {
        assert!(
            rgb(&grid, p)[0] > 0,
            "arm block at {p:?} must be lit before removal"
        );
    }
    prop.remove_light(&mut grid, center);

    assert_eq!(rgb(&grid, center)[0], 0, "source should be dark");
    for p in arms {
        assert_eq!(rgb(&grid, p)[0], 0, "arm block at {p:?} should be dark");
    }
}

/// A grid that only ever held `sources`, as the reference for a removal.
fn grid_with_sources(sources: &[(IVec3, [u8; 3])]) -> VoxelGrid {
    let mut grid = grid_16();
    let mut prop = LightPropagator::new();
    for &(pos, emission) in sources {
        prop.add_light_rgb(&mut grid, pos, emission);
    }
    grid
}

/// Every voxel in `[min, max]` carries the same light in both grids.
fn assert_same_light(actual: &VoxelGrid, expected: &VoxelGrid, min: IVec3, max: IVec3) {
    assert_same_light_in(actual, expected, min, max, "");
}

/// As [`assert_same_light`]; a failure also names `context`.
fn assert_same_light_in(
    actual: &VoxelGrid,
    expected: &VoxelGrid,
    min: IVec3,
    max: IVec3,
    context: &str,
) {
    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                let pos = IVec3::new(x, y, z);
                assert_eq!(
                    rgb(actual, pos),
                    rgb(expected, pos),
                    "light at {pos:?} {context}"
                );
            }
        }
    }
}

/// Light every source, remove `sources[removed]`, and require the whole
/// neighbourhood to equal a grid that only ever held the survivors.
fn assert_removal_leaves_only_survivors(sources: &[(IVec3, [u8; 3])], removed: usize) {
    let mut grid = grid_with_sources(sources);
    let mut prop = LightPropagator::new();
    let survivors: Vec<_> = sources
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != removed)
        .map(|(_, &s)| s)
        .collect();
    let expected = grid_with_sources(&survivors);
    let min = sources.iter().map(|s| s.0).reduce(IVec3::min).unwrap() - IVec3::splat(16);
    let max = sources.iter().map(|s| s.0).reduce(IVec3::max).unwrap() + IVec3::splat(16);

    prop.remove_light(&mut grid, sources[removed].0);

    assert_same_light(&grid, &expected, min, max);
}

/// Each case lights every source, then removes each in turn. The first case sits in
/// chunks with non-zero coordinates on every axis; the second keeps both sources
/// in one chunk; the third is a ring of four.
#[test]
fn removing_one_of_two_lights_leaves_the_other_area_lit() {
    let cases: [&[(IVec3, [u8; 3])]; 3] = [
        &[
            (IVec3::new(20, 18, 33), [15, 10, 5]),
            (IVec3::new(14, 14, 30), [12, 15, 3]),
        ],
        &[
            (IVec3::new(17, 17, 17), [15, 15, 15]),
            (IVec3::new(21, 19, 18), [13, 14, 15]),
        ],
        &[
            (IVec3::new(19, 19, 19), [15, 15, 12]),
            (IVec3::new(25, 19, 19), [14, 15, 13]),
            (IVec3::new(19, 25, 19), [15, 14, 12]),
            (IVec3::new(19, 19, 25), [13, 15, 14]),
        ],
    ];

    for sources in cases {
        for removed in 0..sources.len() {
            assert_removal_leaves_only_survivors(sources, removed);
        }
    }
}

/// The removed light's boundary meets both survivors at the same chunk-local
/// index in different chunks (x = 16 and x = 32).
#[test]
fn removing_the_middle_of_three_lights_one_chunk_apart_keeps_both_survivors() {
    let left = (IVec3::new(10, 8, 8), [15, 15, 15]);
    let middle = (IVec3::new(24, 8, 8), [15, 15, 15]);
    let right = (IVec3::new(38, 8, 8), [15, 15, 15]);

    assert_removal_leaves_only_survivors(&[left, middle, right], 1);
}

#[test]
fn light_falls_off_one_per_block_through_a_gap() {
    let mut grid = grid_16();
    let mut prop = LightPropagator::new();
    let source = IVec3::new(30, 18, 34);
    let gap = IVec3::new(34, 18, 34);
    for y in 10..=26i32 {
        for z in 26..=42i32 {
            let wall = IVec3::new(34, y, z);
            if wall != gap {
                grid.mutator().place(wall, 0, None);
            }
        }
    }

    prop.add_light_rgb(&mut grid, source, [15, 0, 0]);

    for x in 30..=40i32 {
        let expected = (15 - (x - 30)) as u8;
        assert_eq!(rgb(&grid, IVec3::new(x, 18, 34))[0], expected, "x={x}");
    }
    for (pos, expected) in [
        (IVec3::new(35, 19, 34), 9u8),
        (IVec3::new(35, 20, 34), 8),
        (IVec3::new(36, 18, 38), 5),
    ] {
        assert_eq!(rgb(&grid, pos)[0], expected, "behind the wall at {pos:?}");
    }
    for wall in [
        IVec3::new(34, 18, 35),
        IVec3::new(34, 19, 34),
        IVec3::new(34, 22, 30),
    ] {
        assert_eq!(rgb(&grid, wall)[0], 0, "wall cell {wall:?} stays dark");
    }
}

#[test]
fn adding_zero_emission_lights_nothing_and_affects_no_chunk() {
    let mut grid = grid_16();
    let mut prop = LightPropagator::new();
    let pos = IVec3::new(20, 18, 33);

    let affected = prop.add_light_rgb(&mut grid, pos, [0, 0, 0]);

    assert!(affected.is_empty(), "{affected:?}");
    assert_eq!(rgb(&grid, pos), [0, 0, 0]);
    assert!(grid.chunk_light(IVec3::new(1, 1, 2)).is_none());
}

// ---------------------------------------------------------------------------
// Cross-chunk propagation
// ---------------------------------------------------------------------------

/// Light crosses a chunk boundary with correct decay and marks both chunks.
#[test]
fn cross_chunk_propagation_correct() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    for x in 14..=18i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    let affected = prop.add_light_rgb(&mut grid, IVec3::new(15, 0, 0), [15, 0, 0]);

    assert!(
        affected.contains(&IVec3::new(0, 0, 0)),
        "chunk 0 must be in affected set"
    );
    assert!(
        affected.contains(&IVec3::new(1, 0, 0)),
        "chunk 1 must be in affected set"
    );

    for (x, expected) in [(15i32, 15u8), (16, 14), (17, 13), (18, 12)] {
        assert_eq!(rgb(&grid, IVec3::new(x, 0, 0))[0], expected, "x={x}");
    }
}

/// Removing a cross-chunk light clears both chunks.
#[test]
fn cross_chunk_removal_clears_both() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    for x in 12..=20i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    let src = IVec3::new(15, 0, 0);
    prop.add_light_rgb(&mut grid, src, [15, 0, 0]);
    let affected = prop.remove_light(&mut grid, src);

    assert!(affected.contains(&IVec3::new(0, 0, 0)));
    assert!(affected.contains(&IVec3::new(1, 0, 0)));

    for x in 12..=20i32 {
        assert_eq!(
            rgb(&grid, IVec3::new(x, 0, 0))[0],
            0,
            "x={x} should be dark"
        );
    }
}

// ---------------------------------------------------------------------------
// Opaque-block blocking
// ---------------------------------------------------------------------------

/// An opaque block does not receive light — propagation skips it.
/// Note: light CAN travel around a single opaque block through adjacent air;
/// this test verifies only that the opaque block itself stays dark.
#[test]
fn opaque_block_does_not_receive_light() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    grid.mutator().place(IVec3::new(0, 0, 0), 1, None); // transparent source host
    grid.mutator().place(IVec3::new(1, 0, 0), 0, None); // opaque block (mat 0, opacity=15)

    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 0, 0]);

    // The opaque block at x=1 must NOT receive light.
    assert_eq!(
        rgb(&grid, IVec3::new(1, 0, 0))[0],
        0,
        "opaque block must not receive light"
    );
}

/// A voxel fully enclosed by opaque blocks cannot receive light.
#[test]
fn sealed_cave_stays_dark() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    // Build a transparent room at (5,5,5) completely surrounded by opaque shells.
    let center = IVec3::new(5, 5, 5);
    grid.mutator().place(center, 1, None); // interior

    // Seal all 6 faces with opaque blocks (mat 0).
    for &off in &[
        IVec3::new(1, 0, 0),
        IVec3::new(-1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(0, -1, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(0, 0, -1),
    ] {
        grid.mutator().place(center + off, 0, None);
    }

    // Light source just outside the shell.
    let src = IVec3::new(7, 5, 5);
    grid.mutator().place(src, 1, None);
    prop.add_light_rgb(&mut grid, src, [15, 0, 0]);

    // The interior (center) is sealed behind opaque faces — must stay dark.
    assert_eq!(rgb(&grid, center)[0], 0, "sealed interior must stay dark");
}

// ---------------------------------------------------------------------------
// Flood-fill
// ---------------------------------------------------------------------------

#[test]
fn flood_fill_propagates_all_emitters() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    make_torch(&mut grid);
    let mut prop = LightPropagator::new();

    for x in 0..5i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    // Override two positions with torch material.
    grid.mutator().place(IVec3::new(0, 0, 0), 2, None);
    grid.mutator().place(IVec3::new(4, 0, 0), 2, None);

    prop.flood_fill_block_lights(&mut grid);

    assert_eq!(rgb(&grid, IVec3::new(0, 0, 0)), [15, 8, 2], "torch 1");
    assert_eq!(rgb(&grid, IVec3::new(4, 0, 0)), [15, 8, 2], "torch 2");
    assert_eq!(rgb(&grid, IVec3::new(2, 0, 0)), [13, 6, 0], "midpoint");
}

// ---------------------------------------------------------------------------
// Accessor round-trips
// ---------------------------------------------------------------------------

#[test]
fn accessor_round_trip() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new();

    let pos = IVec3::new(0, 0, 0);
    grid.mutator().place(pos, 1, None);
    prop.add_light_rgb(&mut grid, pos, [10, 5, 2]);

    assert_eq!(
        rgb(&grid, pos),
        [10, 5, 2],
        "accessor must return stored RGB"
    );
}

#[test]
fn transparent_block_does_not_occlude_sky() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    grid.mutator().place(IVec3::new(3, 8, 3), 1, None);

    recompute_sky_exposure(&mut grid, IVec3::ZERO);

    assert!(
        sky(&grid, IVec3::new(3, 7, 3)),
        "below a transparent block must stay exposed"
    );
}

// ---------------------------------------------------------------------------
// Removal with emitter blocks
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct TestBlock {
    pos: IVec3,
    emission: [u8; 3],
    opacity_cost: u8,
}

fn blue_emitter(pos: IVec3, level: u8) -> TestBlock {
    TestBlock {
        pos,
        emission: [0, 0, level],
        opacity_cost: 0,
    }
}

/// Unlit grid holding `blocks`, each with a freshly registered material.
fn grid_with_blocks(blocks: &[TestBlock]) -> VoxelGrid {
    let mut grid = grid_16();
    for block in blocks {
        let material = grid.material_registry.register(MaterialType::Lambertian {
            albedo: Vec3::splat(0.5),
        });
        grid.material_registry.set_lighting(
            material,
            MaterialLighting {
                emission: block.emission,
                opacity_cost: block.opacity_cost,
            },
        );
        grid.mutator().place(block.pos, material, None);
    }
    grid
}

/// Reference: a second grid holding `blocks`, lit by a full flood fill.
fn fresh_fill(blocks: &[TestBlock]) -> VoxelGrid {
    let mut grid = grid_with_blocks(blocks);
    LightPropagator::new().flood_fill_block_lights(&mut grid);
    grid
}

/// Light `blocks`, mine `blocks[removed]` out of the grid, then run the removal
/// job for its position, as the game does. Returns the grid after removal and
/// the reference fill of the remaining blocks.
fn remove_block_light(blocks: &[TestBlock], removed: usize) -> (VoxelGrid, VoxelGrid) {
    let mut grid = fresh_fill(blocks);
    let remaining: Vec<TestBlock> = blocks
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != removed)
        .map(|(_, &b)| b)
        .collect();

    grid.mutator().remove(blocks[removed].pos);
    LightPropagator::new().remove_light(&mut grid, blocks[removed].pos);

    (grid, fresh_fill(&remaining))
}

/// A dimmer emitter keeps its whole area when a brighter one nearby goes.
/// Rows are (survivor level, removed level, distance apart along z).
#[test]
fn removing_a_brighter_light_keeps_the_dimmer_survivors_area() {
    let survivor_pos = IVec3::new(19, 19, 19);
    for (survivor, removed, distance) in [(8u8, 15u8, 6i32), (9, 15, 6), (15, 15, 1)] {
        let removed_pos = survivor_pos + IVec3::new(0, 0, distance);
        let blocks = [
            blue_emitter(survivor_pos, survivor),
            blue_emitter(removed_pos, removed),
        ];

        let (grid, expected) = remove_block_light(&blocks, 1);

        let row = format!("survivor {survivor}, removed {removed}, distance {distance}");
        assert_same_light_in(
            &grid,
            &expected,
            survivor_pos.min(removed_pos) - IVec3::splat(16),
            survivor_pos.max(removed_pos) + IVec3::splat(16),
            &row,
        );
        assert_eq!(rgb(&grid, survivor_pos)[2], survivor, "{row}");
    }
}

/// An emitter inside the removed light's reach is not darkened by the removal.
#[test]
fn removing_a_light_keeps_a_nearby_emitter_at_full_strength() {
    let emitter = IVec3::new(19, 19, 19);
    let blocks = [
        blue_emitter(emitter, 10),
        blue_emitter(emitter + IVec3::new(0, 0, 3), 15),
    ];

    let (grid, _) = remove_block_light(&blocks, 1);

    assert_eq!(rgb(&grid, emitter), [0, 0, 10]);
}

/// An opaque survivor is re-seeded too, though no light can reach it from outside.
#[test]
fn removal_re_seeds_an_opaque_emitter() {
    let survivor_pos = IVec3::new(19, 19, 19);
    let blocks = [
        TestBlock {
            opacity_cost: 15,
            ..blue_emitter(survivor_pos, 8)
        },
        blue_emitter(survivor_pos + IVec3::new(0, 0, 6), 15),
    ];

    let (grid, expected) = remove_block_light(&blocks, 1);

    assert_eq!(rgb(&grid, survivor_pos), [0, 0, 8]);
    assert_same_light(
        &grid,
        &expected,
        survivor_pos - IVec3::splat(16),
        survivor_pos + IVec3::new(16, 16, 22),
    );
}

/// A relight seed that the dark wave clears afterwards must not spread light.
#[test]
fn removal_does_not_spread_light_from_a_relight_seed_that_was_cleared() {
    let removed_pos = IVec3::new(15, 12, 12);
    let opaque_pos = IVec3::new(19, 12, 13);
    let blocks = [
        blue_emitter(removed_pos, 8),
        TestBlock {
            opacity_cost: 15,
            ..blue_emitter(opaque_pos, 1)
        },
    ];

    let (grid, expected) = remove_block_light(&blocks, 0);

    assert_same_light(
        &grid,
        &expected,
        removed_pos.min(opaque_pos) - IVec3::splat(16),
        removed_pos.max(opaque_pos) + IVec3::splat(16),
    );
}

/// Each channel is restored from that channel's own emission.
#[test]
fn removal_restores_each_channel_from_its_own_emission() {
    let survivor_pos = IVec3::new(19, 19, 19);
    let removed_pos = survivor_pos + IVec3::new(0, 0, 6);
    let blocks = [
        TestBlock {
            pos: survivor_pos,
            emission: [8, 3, 0],
            opacity_cost: 0,
        },
        TestBlock {
            pos: removed_pos,
            emission: [15, 0, 12],
            opacity_cost: 0,
        },
    ];

    let (grid, expected) = remove_block_light(&blocks, 1);

    assert_eq!(rgb(&grid, survivor_pos), [8, 3, 0]);
    assert_same_light(
        &grid,
        &expected,
        survivor_pos - IVec3::splat(16),
        survivor_pos + IVec3::new(16, 16, 22),
    );
}

const CASES: u32 = 12;

fn box_pos() -> impl Strategy<Value = (i32, i32, i32)> {
    // [10, 22) straddles the chunk border at 16 on every axis.
    (10..22i32, 10..22i32, 10..22i32)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// Removing any one transparent emitter leaves exactly the light a fresh fill
    /// of the remaining blocks gives, everywhere within reach. Survivors are
    /// opaque or transparent; removing an opaque block is a separate rule.
    #[test]
    fn prop_removal_matches_a_fresh_fill(
        emitter_cells in prop::collection::btree_map(
            box_pos(),
            ([0u8..=15, 0u8..=15, 0u8..=15], prop::bool::ANY),
            2..=5,
        ),
        wall_cells in prop::collection::btree_set(box_pos(), 0..=6),
        removed_pick in 0usize..5,
    ) {
        let removed = removed_pick % emitter_cells.len();
        let emitters: Vec<TestBlock> = emitter_cells
            .iter()
            .enumerate()
            .map(|(i, (&(x, y, z), &(emission, opaque)))| TestBlock {
                pos: IVec3::new(x, y, z),
                emission,
                opacity_cost: if opaque && i != removed { 15 } else { 0 },
            })
            .collect();
        let walls = wall_cells
            .iter()
            .filter(|cell| !emitter_cells.contains_key(*cell))
            .map(|&(x, y, z)| TestBlock {
                pos: IVec3::new(x, y, z),
                emission: [0; 3],
                opacity_cost: 15,
            });
        let blocks: Vec<TestBlock> = emitters.iter().copied().chain(walls).collect();

        let (grid, expected) = remove_block_light(&blocks, removed);

        assert_same_light(
            &grid,
            &expected,
            IVec3::splat(10 - 16),
            IVec3::splat(21 + 16),
        );
    }
}

// ---------------------------------------------------------------------------
// Reconciling a position against the grid
// ---------------------------------------------------------------------------

/// Level 15 blue emitter at (14,16,16) behind a 33x33 opaque wall in the x = 16
/// plane (wide enough that no light leaks round its edge), two blocks away; the
/// wall block in line with the emitter is mined.
#[test]
fn removing_a_wall_block_lets_light_through_as_a_fresh_fill() {
    let emitter = IVec3::new(14, 16, 16);
    let mined = IVec3::new(16, 16, 16);
    let opaque = |pos| TestBlock {
        pos,
        emission: [0; 3],
        opacity_cost: 15,
    };
    let mut blocks = vec![blue_emitter(emitter, 15)];
    for y in 0..=32 {
        for z in 0..=32 {
            blocks.push(opaque(IVec3::new(16, y, z)));
        }
    }
    let mut grid = fresh_fill(&blocks);
    assert_eq!(
        rgb(&grid, IVec3::new(17, 16, 16)),
        [0; 3],
        "dark behind the wall"
    );
    blocks.retain(|b| b.pos != mined);

    grid.mutator().remove(mined);
    LightPropagator::new().update_at(&mut grid, mined);

    assert_eq!(rgb(&grid, mined), [0, 0, 13], "the opening is lit");
    assert_eq!(
        rgb(&grid, IVec3::new(17, 16, 16)),
        [0, 0, 12],
        "light passes through"
    );
    assert_same_light(
        &grid,
        &fresh_fill(&blocks),
        IVec3::splat(16 - 20),
        IVec3::splat(16 + 20),
    );
}

/// The wall block in line with a level 15 emitter becomes a dimmer emitter of its
/// own: its neighbour's light refills it first, and its emission must still win.
#[test]
fn replacing_a_wall_block_with_an_emitter_lights_it_at_its_emission() {
    let emitter = IVec3::new(14, 16, 16);
    let replaced = IVec3::new(16, 16, 16);
    let mut blocks = vec![blue_emitter(emitter, 15)];
    for y in 0..=32 {
        for z in 0..=32 {
            blocks.push(TestBlock {
                pos: IVec3::new(16, y, z),
                emission: [0; 3],
                opacity_cost: 15,
            });
        }
    }
    let mut grid = fresh_fill(&blocks);
    for block in &mut blocks {
        if block.pos == replaced {
            *block = blue_emitter(replaced, 14);
        }
    }
    let lamp = grid.material_registry.register(MaterialType::Lambertian {
        albedo: Vec3::splat(0.5),
    });
    grid.material_registry.set_lighting(
        lamp,
        MaterialLighting {
            emission: [0, 0, 14],
            opacity_cost: 0,
        },
    );

    grid.mutator().place(replaced, lamp, None);
    LightPropagator::new().update_at(&mut grid, replaced);

    assert_eq!(
        rgb(&grid, replaced),
        [0, 0, 14],
        "the emitter reads its emission"
    );
    assert_same_light(
        &grid,
        &fresh_fill(&blocks),
        IVec3::splat(16 - 20),
        IVec3::splat(16 + 20),
    );
}

#[test]
fn reconciling_far_from_light_changes_nothing_and_reports_no_chunk() {
    let far = IVec3::new(60, 60, 60);
    let blocks = [
        blue_emitter(IVec3::new(4, 4, 4), 15),
        TestBlock {
            pos: far,
            emission: [0; 3],
            opacity_cost: 15,
        },
    ];
    let mut grid = fresh_fill(&blocks);
    let expected = fresh_fill(&blocks[..1]);
    grid.mutator().remove(far);

    let chunks = LightPropagator::new().update_at(&mut grid, far);

    assert!(chunks.is_empty(), "no chunk written: {chunks:?}");
    assert_same_light(&grid, &expected, IVec3::splat(-16), IVec3::splat(80));
}

#[test]
fn reconciling_a_removed_emitter_darkens_its_whole_area() {
    let pos = IVec3::new(19, 19, 19);
    let blocks = [blue_emitter(pos, 15)];
    let mut grid = fresh_fill(&blocks);
    assert_eq!(rgb(&grid, pos), [0, 0, 15], "lit before the removal");

    grid.mutator().remove(pos);
    let chunks = LightPropagator::new().update_at(&mut grid, pos);

    assert!(!chunks.is_empty(), "the darkened chunks are reported");
    assert_same_light(
        &grid,
        &fresh_fill(&[]),
        pos - IVec3::splat(16),
        pos + IVec3::splat(16),
    );
}

/// Light cost of a test block: transparent, partly absorbing, or fully opaque.
const OPACITY_COSTS: [u8; 3] = [0, 7, 15];

/// What a test edit puts at a position: `None` mines it; `Some((emission,
/// opacity_cost))` places a block with that emission and opacity.
type EditSpec = Option<([u8; 3], u8)>;

type BlockState = std::collections::BTreeMap<(i32, i32, i32), ([u8; 3], u8)>;

fn apply_edit(grid: &mut VoxelGrid, pos: IVec3, spec: EditSpec) {
    let Some((emission, opacity_cost)) = spec else {
        grid.mutator().remove(pos);
        return;
    };
    let material = grid.material_registry.register(MaterialType::Lambertian {
        albedo: Vec3::splat(0.5),
    });
    grid.material_registry.set_lighting(
        material,
        MaterialLighting {
            emission,
            opacity_cost,
        },
    );
    grid.mutator().place(pos, material, None);
}

fn current_blocks(state: &BlockState) -> Vec<TestBlock> {
    state
        .iter()
        .map(|(&(x, y, z), &(emission, opacity_cost))| TestBlock {
            pos: IVec3::new(x, y, z),
            emission,
            opacity_cost,
        })
        .collect()
}

/// A small box straddling the chunk border at 16, so edits revisit cells, remove
/// real blocks and open walls.
fn edit_pos() -> impl Strategy<Value = (i32, i32, i32)> {
    (14..18i32, 14..18i32, 14..18i32)
}

/// Most blocks emit nothing, so walls and openings matter more than sources.
fn edit_emission() -> impl Strategy<Value = [u8; 3]> {
    prop_oneof![
        3 => Just([0u8; 3]),
        1 => prop::array::uniform3(0u8..=15),
    ]
}

fn edit_block() -> impl Strategy<Value = ([u8; 3], u8)> {
    (
        edit_emission(),
        prop::sample::select(OPACITY_COSTS.to_vec()),
    )
}

fn edit_spec() -> impl Strategy<Value = EditSpec> {
    prop::option::of(edit_block())
}

/// A world that is already lit, so there is light to block and to let through.
fn initial_world() -> impl Strategy<Value = BlockState> {
    prop::collection::btree_map(edit_pos(), edit_block(), 0..=10)
}

/// The light-relevant region: the edit box plus the 16-block reach of light.
const REGION_MIN: IVec3 = IVec3::splat(14 - 16);
const REGION_MAX: IVec3 = IVec3::splat(17 + 16);

/// A placed opaque block that sat in real light goes dark, light behind it
/// matches a fresh fill, and nothing is refilled into it.
#[test]
fn reconciling_a_placed_opaque_block_shadows_and_does_not_refill_it() {
    let emitter = IVec3::new(14, 16, 16);
    let placed = IVec3::new(16, 16, 16);
    let behind = IVec3::new(17, 16, 16);
    let mut blocks = vec![blue_emitter(emitter, 15)];
    let mut grid = fresh_fill(&blocks);
    assert_eq!(rgb(&grid, placed), [0, 0, 13], "lit before the block");
    let lit_behind = rgb(&grid, behind);
    blocks.push(TestBlock {
        pos: placed,
        emission: [0; 3],
        opacity_cost: 15,
    });

    apply_edit(&mut grid, placed, Some(([0; 3], 15)));
    LightPropagator::new().update_at(&mut grid, placed);

    assert_eq!(rgb(&grid, placed), [0; 3], "no light is refilled into it");
    assert_ne!(rgb(&grid, behind), lit_behind, "the block casts a shadow");
    assert_same_light(
        &grid,
        &fresh_fill(&blocks),
        IVec3::splat(16 - 20),
        IVec3::splat(16 + 20),
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// Whatever the edit sequence, reconciling each edited position leaves the
    /// region equal to a fresh fill of the blocks as they now stand.
    #[test]
    fn prop_edits_then_reconcile_match_a_fresh_fill(
        initial in initial_world(),
        edits in prop::collection::vec((edit_pos(), edit_spec()), 1..=10),
    ) {
        let mut state = initial;
        let mut grid = fresh_fill(&current_blocks(&state));
        let mut prop = LightPropagator::new();

        for (i, &((x, y, z), spec)) in edits.iter().enumerate() {
            let pos = IVec3::new(x, y, z);
            apply_edit(&mut grid, pos, spec);
            match spec {
                Some(block) => state.insert((x, y, z), block),
                None => state.remove(&(x, y, z)),
            };
            prop.update_at(&mut grid, pos);

            assert_same_light_in(
                &grid,
                &fresh_fill(&current_blocks(&state)),
                REGION_MIN,
                REGION_MAX,
                &format!("after edit {i}: {spec:?} at {pos:?}"),
            );
        }
    }

    /// Apply a whole batch of edits (removals and replacements of existing blocks
    /// among them) first, then reconcile the edited positions in an arbitrary
    /// order, some more than once: the result is still a fresh fill.
    #[test]
    fn prop_batch_reconciled_in_any_order_matches_a_fresh_fill(
        (initial, edits, order) in (
            initial_world(),
            prop::collection::btree_map(edit_pos(), edit_spec(), 1..=10),
        )
            .prop_flat_map(|(initial, edits)| {
                let count = edits.len();
                let order = prop::collection::vec(0..count, 0..=4)
                    .prop_map(move |repeats| (0..count).chain(repeats).collect::<Vec<_>>())
                    .prop_shuffle();
                (Just(initial), Just(edits), order)
            }),
    ) {
        let mut state = initial;
        let mut grid = fresh_fill(&current_blocks(&state));
        let edited: Vec<((i32, i32, i32), EditSpec)> = edits.into_iter().collect();
        for &((x, y, z), spec) in &edited {
            apply_edit(&mut grid, IVec3::new(x, y, z), spec);
            match spec {
                Some(block) => state.insert((x, y, z), block),
                None => state.remove(&(x, y, z)),
            };
        }

        let mut prop = LightPropagator::new();
        for i in order {
            let (x, y, z) = edited[i].0;
            prop.update_at(&mut grid, IVec3::new(x, y, z));
        }

        assert_same_light(
            &grid,
            &fresh_fill(&current_blocks(&state)),
            REGION_MIN,
            REGION_MAX,
        );
    }
}
