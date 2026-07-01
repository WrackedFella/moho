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

/// Recomputing sky-exposure twice produces identical results (idempotency).
#[test]
fn sky_recompute_is_idempotent() {
    let mut grid = grid_16();
    grid.mutator().place(IVec3::new(2, 10, 2), 0, None);
    let cp = IVec3::ZERO;

    recompute_sky_exposure(&mut grid, cp);
    let first: Vec<bool> = (0..4096usize)
        .map(|i| grid.chunk_light(cp).unwrap().sky_exposed_at(i))
        .collect();

    grid.chunk_light_mut(cp).sky_dirty = true;
    recompute_sky_exposure(&mut grid, cp);
    let second: Vec<bool> = (0..4096usize)
        .map(|i| grid.chunk_light(cp).unwrap().sky_exposed_at(i))
        .collect();

    assert_eq!(first, second, "sky exposure must be identical on re-run");
}

/// `dirty_chunks_below` marks lower chunks sky_dirty.
#[test]
fn sky_dirty_chunks_below_propagates() {
    let mut grid = grid_16();
    // place_block allocates chunk_light for the block's chunk and triggers note_block_change,
    // which auto-marks lower chunks. We verify via dirty_chunks_below explicitly.
    grid.chunk_light_mut(IVec3::ZERO).sky_dirty = false; // manually clear
    grid.mutator().place(IVec3::new(0, 16, 0), 0, None); // chunk (0,1,0)
    dirty_chunks_below(&mut grid, IVec3::new(0, 1, 0));

    if let Some(cl) = grid.chunk_light(IVec3::ZERO) {
        assert!(cl.sky_dirty, "chunk below should be sky_dirty");
    }
}

/// `ensure_chunk_sky_ready` is a no-op when sky_dirty is false.
#[test]
fn sky_ensure_ready_noop_when_clean() {
    let mut grid = grid_16();
    let cp = IVec3::ZERO;
    grid.chunk_light_mut(cp).sky_dirty = false;
    ensure_chunk_sky_ready(&mut grid, cp);
    assert!(!grid.chunk_light(cp).unwrap().sky_dirty);
}

// ---------------------------------------------------------------------------
// Block-light propagation invariants
// ---------------------------------------------------------------------------

/// Light decays by exactly 1 per step along a line of transparent blocks.
#[test]
fn block_light_decay_one_per_step() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new(16);

    for x in 0..8i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 0, 0]);

    for x in 0..8i32 {
        let r = rgb(&grid, IVec3::new(x, 0, 0))[0];
        assert_eq!(r, (15 - x as u8), "x={x}: expected R={}, got {r}", 15 - x);
    }
}

/// A weaker source must not reduce an existing brighter value.
#[test]
fn block_light_does_not_dim_brighter() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new(16);

    let pos = IVec3::new(0, 0, 0);
    grid.mutator().place(pos, 1, None);

    // Write R=12 directly into chunk light.
    grid.set_block_light_rgb(pos, [12, 0, 0]);

    // Add a weaker source (R=8).
    prop.add_light_rgb(&mut grid, pos, [8, 0, 0]);
    assert_eq!(rgb(&grid, pos)[0], 12, "existing R=12 must not be reduced");
}

/// Cave voxels with no sources start at R=G=B=0.
#[test]
fn block_light_cave_is_pure_black() {
    let grid = grid_16();
    // No propagation — default ChunkLight values should all be 0.
    // Chunk (0,0,0) has no ChunkLight entry yet; block_light_rgb_at returns [0,0,0].
    assert_eq!(rgb(&grid, IVec3::new(5, 5, 5)), [0, 0, 0]);
}

/// R, G, B propagate independently: emitting only R does not affect G or B.
#[test]
fn block_light_channels_are_independent() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new(16);

    for x in 0..4i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 0, 0]);

    let nb = rgb(&grid, IVec3::new(1, 0, 0));
    assert!(nb[0] > 0, "R should propagate");
    assert_eq!(nb[1], 0, "G must stay 0");
    assert_eq!(nb[2], 0, "B must stay 0");
}

/// A warm torch produces R > G > B at the immediate neighbor.
#[test]
fn block_light_colored_propagation() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    make_torch(&mut grid);
    let mut prop = LightPropagator::new(16);

    for x in 0..5i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    // Place torch: emission [15, 8, 2].
    prop.add_light_rgb(&mut grid, IVec3::new(0, 0, 0), [15, 8, 2]);

    let nb = rgb(&grid, IVec3::new(1, 0, 0));
    assert!(nb[0] >= nb[1], "R must >= G at neighbor");
    assert!(nb[1] >= nb[2], "G must >= B at neighbor");
}

// ---------------------------------------------------------------------------
// Removal invariants
// ---------------------------------------------------------------------------

/// After removing the only source, all voxels go to 0.
#[test]
fn removal_single_source_clears_all() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new(16);

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
    let mut prop = LightPropagator::new(16);

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

/// Removing a light at a position with level=0 is a no-op (no panic).
#[test]
fn removal_of_absent_light_is_noop() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new(16);

    grid.mutator().place(IVec3::new(0, 0, 0), 1, None);
    prop.remove_light(&mut grid, IVec3::new(0, 0, 0)); // nothing to remove — must not panic
}

// ---------------------------------------------------------------------------
// Cross-chunk propagation
// ---------------------------------------------------------------------------

/// Light crosses a chunk boundary with correct decay and marks both chunks.
#[test]
fn cross_chunk_propagation_correct() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new(16);

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
    let mut prop = LightPropagator::new(16);

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
    let mut prop = LightPropagator::new(16);

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
    let mut prop = LightPropagator::new(16);

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

/// flood_fill_block_lights propagates light from all emissive blocks.
#[test]
fn flood_fill_propagates_all_emitters() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    make_torch(&mut grid);
    let mut prop = LightPropagator::new(16);

    for x in 0..5i32 {
        grid.mutator().place(IVec3::new(x, 0, 0), 1, None);
    }
    // Override two positions with torch material.
    grid.mutator().place(IVec3::new(0, 0, 0), 2, None);
    grid.mutator().place(IVec3::new(4, 0, 0), 2, None);

    prop.flood_fill_block_lights(&mut grid);

    assert_eq!(rgb(&grid, IVec3::new(0, 0, 0))[0], 15, "torch 1: R=15");
    assert_eq!(rgb(&grid, IVec3::new(4, 0, 0))[0], 15, "torch 2: R=15");
    assert!(
        rgb(&grid, IVec3::new(2, 0, 0))[0] > 0,
        "midpoint should be lit"
    );
}

// ---------------------------------------------------------------------------
// Accessor round-trips
// ---------------------------------------------------------------------------

/// block_light_rgb_at and sky_exposed_at surface the correct ChunkLight values.
#[test]
fn accessor_round_trip() {
    let mut grid = grid_16();
    make_transparent(&mut grid);
    let mut prop = LightPropagator::new(16);

    let pos = IVec3::new(0, 0, 0);
    grid.mutator().place(pos, 1, None);
    prop.add_light_rgb(&mut grid, pos, [10, 5, 2]);

    assert_eq!(
        rgb(&grid, pos),
        [10, 5, 2],
        "accessor must return stored RGB"
    );
    assert_eq!(
        rgb(&grid, IVec3::new(7, 7, 7)),
        [0, 0, 0],
        "unlit position returns zero"
    );

    // Sky: no blocks above → exposed.
    recompute_sky_exposure(&mut grid, IVec3::ZERO);
    assert!(sky(&grid, pos), "unoccluded voxel must be sky-exposed");
}
