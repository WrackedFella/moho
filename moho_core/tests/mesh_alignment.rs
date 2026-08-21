use glam::IVec3;
use moho_core::voxel::{VoxelChunk, VoxelGrid};

/// Smooth (marching-cubes) terrain must occupy the same world-space cube that
/// the voxel grid, raycasting, and the blocky/coarse mesh paths all assume:
/// block `V` fills `[V, V+1]`.
///
/// Regression guard for a bug where the smooth path rendered half a unit
/// off-centre on every axis, because it never undid the density field's
/// one-cell padding or the sample-is-a-whole-block convention. Symptom was
/// mining appearing to hit nothing: the ray struck the block *behind* the
/// visible slope, carving hidden cavities until the surface finally broke open.
#[test]
fn smooth_mesh_is_centered_on_the_voxel_it_represents() {
    let mut grid = VoxelGrid::new(16);
    let v = IVec3::new(5, 5, 5);
    grid.mutator().place(v, 1, None); // material < 100 => smooth path

    let chunk = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0);
    let verts = chunk.vertices();
    assert!(!verts.is_empty(), "expected geometry for the solid voxel");

    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in verts {
        for a in 0..3 {
            min[a] = min[a].min(p[a]);
            max[a] = max[a].max(p[a]);
        }
    }

    for a in 0..3 {
        let expected_center = v[a] as f32 + 0.5;
        let center = (min[a] + max[a]) * 0.5;
        assert!(
            (center - expected_center).abs() < 1e-4,
            "axis {a}: smooth mesh centred at {center}, expected {expected_center} \
             (bbox {min:?}..{max:?}) — smooth terrain is misaligned from the voxel grid"
        );
    }
}
