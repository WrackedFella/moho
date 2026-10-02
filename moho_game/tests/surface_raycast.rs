use glam::{IVec3, Vec3};
use moho_core::voxel::{VoxelChunk, VoxelGrid};
use moho_game::raycast;

fn ramp_grid() -> VoxelGrid {
    let mut grid = VoxelGrid::new(16);
    for x in 0..16 {
        for z in 0..16 {
            let h = (x + 1).min(14);
            for y in 0..h {
                grid.mutator().place(IVec3::new(x, y, z), 1, None);
            }
        }
    }
    grid
}

/// Dense point sampling of the rendered surface, used to ask "does this point
/// lie on what the player can see?"
fn surface_points(grid: &VoxelGrid) -> Vec<Vec3> {
    let chunk = VoxelChunk::from_grid_lod(grid, IVec3::ZERO, 0);
    let verts = chunk.vertices();
    let indices = chunk.indices();

    let mut pts = Vec::new();
    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let p = |i: u32| {
            let v = verts[i as usize];
            Vec3::new(v[0], v[1], v[2])
        };
        let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
        pts.push(a);
        pts.push(b);
        pts.push(c);
        pts.push((a + b + c) / 3.0);
        pts.push((a + b) * 0.5);
        pts.push((b + c) * 0.5);
        pts.push((a + c) * 0.5);
    }
    pts
}

fn distance_to_surface(pts: &[Vec3], p: Vec3) -> f32 {
    pts.iter()
        .map(|q| (*q - p).length())
        .fold(f32::INFINITY, f32::min)
}

/// A raycast's reported hit must lie on the surface the player can actually
/// see. Regression guard for the bug where mining silently did nothing on
/// slanted terrain: the cube-stepping raycast reports hits on binary voxel
/// cube faces, but the renderer draws a marching-cubes isosurface on the dual
/// lattice, so reported hits sat well behind the visible surface and the block
/// removed was not the one forming it.
#[test]
fn surface_raycast_reports_hits_on_the_visible_surface() {
    let grid = ramp_grid();
    let pts = surface_points(&grid);

    // A player standing off the ramp, looking across it — oblique, grazing
    // angles are exactly where cube-stepping goes wrong.
    let eye = Vec3::new(-3.0, 10.5, 8.0);

    let mut tested = 0usize;
    let mut surface_on_surface = 0usize;
    let mut dda_on_surface = 0usize;
    let mut worst_surface = 0.0f32;
    let mut worst_dda = 0.0f32;

    for yaw_step in -12..=12 {
        for pitch_step in -12..=6 {
            // Base yaw looks along +X, the direction the ramp rises.
            let yaw = std::f32::consts::FRAC_PI_2 + yaw_step as f32 * 0.05;
            let pitch = pitch_step as f32 * 0.05;
            let dir = Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                yaw.cos() * pitch.cos(),
            )
            .normalize();

            let Some(hit) = raycast::raycast_surface(&grid, eye, dir, 40.0) else {
                continue;
            };
            tested += 1;

            let d = distance_to_surface(&pts, hit.position);
            worst_surface = worst_surface.max(d);
            if d <= 0.35 {
                surface_on_surface += 1;
            }

            if let Some(dda) = raycast::raycast(&grid, eye, dir, 40.0) {
                let dd = distance_to_surface(&pts, dda.position);
                worst_dda = worst_dda.max(dd);
                if dd <= 0.35 {
                    dda_on_surface += 1;
                }
            }
        }
    }

    assert!(tested > 100, "expected a decent number of surface hits");

    let rate = 100.0 * surface_on_surface as f32 / tested as f32;
    let dda_rate = 100.0 * dda_on_surface as f32 / tested as f32;
    eprintln!("rays hitting the surface: {tested}");
    eprintln!(
        "surface raycast on visible surface: {surface_on_surface}/{tested} ({rate:.1}%), worst err {worst_surface:.2}"
    );
    eprintln!(
        "cube DDA       on visible surface: {dda_on_surface}/{tested} ({dda_rate:.1}%), worst err {worst_dda:.2}"
    );

    assert!(
        rate >= 95.0,
        "only {rate:.1}% of reported hits landed on the visible surface"
    );
}
