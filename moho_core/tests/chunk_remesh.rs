use glam::IVec3;
use moho_core::voxel::{VoxelChunk, VoxelGrid};

/// Guards the mutate → remesh contract the player mining action depends on:
/// a voxel removed from the grid must produce a different chunk mesh on
/// regeneration. Added after a 3.1a investigation where mining appeared to do
/// nothing on screen — the mesh *was* updating, but a single-voxel change to
/// smooth (marching-cubes) terrain is visually subtle. This test pins the
/// contract so a future regression is caught here rather than mistaken for a
/// perception problem again.
#[test]
fn removing_surface_voxel_changes_chunk_mesh() {
    let mut grid = VoxelGrid::new(16);

    // Fill a solid slab in chunk (0,0,0): y in 0..8, full 16x16 footprint.
    for x in 0..16 {
        for z in 0..16 {
            for y in 0..8 {
                grid.mutator().place(IVec3::new(x, y, z), 1, None);
            }
        }
    }

    let before = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0);
    let before_verts = before.vertices().to_vec();

    // Remove one voxel on the top surface.
    let target = IVec3::new(8, 7, 8);
    let removed = grid.mutator().remove(target);
    assert!(removed, "expected the target voxel to be removed");
    assert!(grid.material_at(target).is_none(), "voxel still solid");

    let after = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0);
    let after_verts = after.vertices().to_vec();

    eprintln!("before: {} verts", before_verts.len());
    eprintln!("after:  {} verts", after_verts.len());
    eprintln!("identical mesh: {}", before_verts == after_verts);

    assert_ne!(
        before_verts, after_verts,
        "removing a surface voxel did not change the regenerated mesh"
    );
}

/// The mesh change from removing one voxel must be spatially local to that
/// voxel — this is what makes "the terrain deforms where I mined" true. Added
/// during the same 3.1a investigation, where deformation appeared to show up
/// somewhere other than the aim point.
#[test]
fn mesh_change_is_local_to_the_removed_voxel() {
    let mut grid = VoxelGrid::new(16);

    // Rolling terrain so the surface varies across the chunk.
    for x in 0..16 {
        for z in 0..16 {
            let h = 6 + ((x + z) % 5);
            for y in 0..h {
                grid.mutator().place(IVec3::new(x, y, z), 1, None);
            }
        }
    }

    let before: Vec<[f32; 3]> = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0)
        .vertices()
        .to_vec();

    let target = IVec3::new(3, 6, 3);
    assert!(grid.material_at(target).is_some(), "target must be solid");
    grid.mutator().remove(target);

    let after: Vec<[f32; 3]> = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0)
        .vertices()
        .to_vec();

    let key = |v: &[f32; 3]| {
        (
            (v[0] * 1000.0).round() as i64,
            (v[1] * 1000.0).round() as i64,
            (v[2] * 1000.0).round() as i64,
        )
    };
    let before_set: std::collections::HashSet<_> = before.iter().map(key).collect();
    let after_set: std::collections::HashSet<_> = after.iter().map(key).collect();

    let changed: Vec<[f32; 3]> = after
        .iter()
        .filter(|v| !before_set.contains(&key(v)))
        .chain(before.iter().filter(|v| !after_set.contains(&key(v))))
        .copied()
        .collect();

    assert!(!changed.is_empty(), "no vertices changed at all");

    // Every changed vertex must lie within one voxel of the removed cell.
    for v in &changed {
        for axis in 0..3 {
            let lo = [target.x, target.y, target.z][axis] as f32 - 1.0;
            let hi = [target.x, target.y, target.z][axis] as f32 + 2.0;
            assert!(
                v[axis] >= lo && v[axis] <= hi,
                "changed vertex {v:?} lies outside the removed voxel's neighborhood \
                 on axis {axis} (expected {lo}..={hi})"
            );
        }
    }
}
