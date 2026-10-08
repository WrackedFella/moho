//! Integration tests for the Stage 4C lighting system.
//!
//! Each test targets a specific invariant, regression risk, or algorithmic
//! property. Failing tests identify exactly which guarantee broke.

use glam::IVec3;
use moho_core::voxel::{
    LightPropagator, MaterialLighting, VoxelGrid, dirty_chunks_below, ensure_chunk_sky_ready,
    recompute_sky_exposure,
};

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

/// Removing the only opaque block in a column re-exposes the voxels below it
/// once the sky pass reruns.
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

/// `dirty_chunks_below` dirties chunks beneath `chunk_pos` in the same column
/// only, never `chunk_pos` itself or other columns.
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

/// `ensure_chunk_sky_ready` does not recompute when sky_dirty is false.
#[test]
fn sky_ensure_ready_noop_when_clean() {
    let mut grid = grid_16();
    let cp = IVec3::ZERO;
    let below = IVec3::new(3, 7, 3);
    recompute_sky_exposure(&mut grid, cp);
    grid.mutator().place(IVec3::new(3, 8, 3), 0, None);
    grid.chunk_light_mut(cp).sky_dirty = false;

    ensure_chunk_sky_ready(&mut grid, cp);

    assert!(sky(&grid, below), "a clean chunk keeps its stale exposure");
}

/// An opaque block in a neighbouring (x, z) chunk column does not shadow this
/// column.
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

/// Each channel decays one level per step from its own emission level,
/// clamping at zero.
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

/// Removing at an unlit position changes nothing and affects no chunk.
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

/// An adjacent source of equal level is a survivor, not part of the chain
/// being removed.
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

/// `flood_fill_block_lights` seeds every channel of every emissive block.
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
