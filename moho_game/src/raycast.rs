use glam::{IVec3, Vec3};
use moho_core::voxel::{BlockPos, VoxelGrid};

/// Result of a raycast operation
#[derive(Debug, Clone, Copy)]
pub struct RaycastResult {
    /// The position of the block that was hit
    pub block_pos: BlockPos,
    /// The face of the block that was hit (normal)
    pub normal: IVec3,
    /// The exact world position of the intersection
    pub position: Vec3,
    /// Distance from ray origin
    pub distance: f32,
}

/// Isosurface threshold. Must match the marching-cubes generator's `iso_level`
/// (`moho_core::voxel::mesh::marching_cubes`), or aiming and rendering disagree.
const ISO_LEVEL: f32 = 0.5;

/// Ray-march increment, in world units. Fine enough that a 1-unit voxel is
/// sampled many times, so thin features aren't stepped over.
const SURFACE_STEP: f32 = 0.05;

/// Bisection passes used to sharpen the crossing point once a step brackets it.
const SURFACE_REFINE_STEPS: u32 = 8;

/// Occupancy of `block`, matching how the mesh generator builds its density
/// field: any present block is solid, air is empty.
fn occupancy(grid: &VoxelGrid, block: BlockPos) -> f32 {
    if grid.material_at(block).is_some() {
        1.0
    } else {
        0.0
    }
}

/// The eight blocks whose centres bound world point `p`, as a base index plus
/// fractional offset (0..1 on each axis) toward the far corner. Shared by
/// every function that samples the density lattice, so the corner-indexing
/// math (and the `-0.5` block-centre-to-corner shift) exists in one place.
fn corner_lattice(p: Vec3) -> (IVec3, Vec3) {
    let q = p - Vec3::splat(ISO_LEVEL);
    let base = q.floor();
    let f = q - base;
    (IVec3::new(base.x as i32, base.y as i32, base.z as i32), f)
}

/// Trilinearly-interpolated density at a world point, reproducing the field
/// marching cubes meshes from. Density samples represent whole blocks and sit
/// at block centres (`block + 0.5`), so the lattice is offset half a unit from
/// block corners — shifting by `-0.5` puts a block's centre on an integer.
fn density_at(grid: &VoxelGrid, p: Vec3) -> f32 {
    let (bi, f) = corner_lattice(p);

    let mut density = 0.0;
    for dx in 0..2 {
        for dy in 0..2 {
            for dz in 0..2 {
                let w = (if dx == 0 { 1.0 - f.x } else { f.x })
                    * (if dy == 0 { 1.0 - f.y } else { f.y })
                    * (if dz == 0 { 1.0 - f.z } else { f.z });
                density += occupancy(grid, bi + IVec3::new(dx, dy, dz)) * w;
            }
        }
    }
    density
}

/// The solid block responsible for the surface at `p`: of the eight blocks
/// whose centres bound `p`, the nearest one that is actually solid.
fn owning_block(grid: &VoxelGrid, p: Vec3) -> Option<BlockPos> {
    let (bi, _) = corner_lattice(p);

    let mut best: Option<(f32, BlockPos)> = None;
    for dx in 0..2 {
        for dy in 0..2 {
            for dz in 0..2 {
                let block = bi + IVec3::new(dx, dy, dz);
                if grid.material_at(block).is_none() {
                    continue;
                }
                let centre = block.as_vec3() + Vec3::splat(ISO_LEVEL);
                let d = (centre - p).length_squared();
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, block));
                }
            }
        }
    }
    best.map(|(_, block)| block)
}

/// Dominant-axis face normal derived from the density gradient, so callers that
/// want a face (e.g. placing a block against the surface) still get one.
fn surface_normal(grid: &VoxelGrid, p: Vec3) -> IVec3 {
    const H: f32 = 0.5;
    let gx = density_at(grid, p + Vec3::X * H) - density_at(grid, p - Vec3::X * H);
    let gy = density_at(grid, p + Vec3::Y * H) - density_at(grid, p - Vec3::Y * H);
    let gz = density_at(grid, p + Vec3::Z * H) - density_at(grid, p - Vec3::Z * H);

    // Gradient points into the solid; negate so the normal points outward.
    let g = Vec3::new(-gx, -gy, -gz);
    let (ax, ay, az) = (g.x.abs(), g.y.abs(), g.z.abs());
    if ax >= ay && ax >= az {
        IVec3::new(g.x.signum() as i32, 0, 0)
    } else if ay >= az {
        IVec3::new(0, g.y.signum() as i32, 0)
    } else {
        IVec3::new(0, 0, g.z.signum() as i32)
    }
}

/// Raycast against the *rendered* smooth (marching-cubes) surface.
///
/// Prefer this over [`raycast`] for anything the player aims at. [`raycast`]
/// walks binary voxel cubes, but the renderer draws an isosurface on the dual
/// lattice — roughly two thirds of visible surface geometry lies inside blocks
/// the grid calls empty, which a cube-stepping ray passes straight through.
/// That mismatch made mining silently miss most slanted terrain.
///
/// Returns the solid block owning the surface that was hit, so the caller can
/// remove it.
pub fn raycast_surface(
    grid: &VoxelGrid,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<RaycastResult> {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }

    let mut t = 0.0;
    let mut prev_t = 0.0;

    // Already inside the surface at the origin — nothing sensible to aim at.
    if density_at(grid, origin) >= ISO_LEVEL {
        return None;
    }

    while t <= max_distance {
        t += SURFACE_STEP;
        let density = density_at(grid, origin + dir * t);

        if density >= ISO_LEVEL {
            // Bisect the bracketing interval to sharpen the crossing.
            let (mut lo, mut hi) = (prev_t, t);
            for _ in 0..SURFACE_REFINE_STEPS {
                let mid = (lo + hi) * 0.5;
                if density_at(grid, origin + dir * mid) >= ISO_LEVEL {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }

            let distance = hi;
            let position = origin + dir * distance;
            let block_pos = owning_block(grid, position)?;

            return Some(RaycastResult {
                block_pos,
                normal: surface_normal(grid, position),
                position,
                distance,
            });
        }

        prev_t = t;
    }

    None
}

/// Distance to the next voxel boundary and the distance to cross one voxel,
/// for a single axis. Returns `(f32::INFINITY, f32::INFINITY)` when
/// `dir_component` is zero (an axis-aligned ray, e.g. looking straight up or
/// down), so that axis is never selected as the next step — dividing by an
/// exact `0.0` otherwise yields `-inf` or `+inf` depending on the numerator's
/// sign, which corrupts the traversal's `t_max` comparisons.
fn axis_traversal(
    origin_component: f32,
    dir_component: f32,
    voxel_coord: i32,
    step: i32,
) -> (f32, f32) {
    if dir_component == 0.0 {
        return (f32::INFINITY, f32::INFINITY);
    }
    let boundary = if step > 0 {
        voxel_coord as f32 + 1.0 - origin_component
    } else {
        voxel_coord as f32 - origin_component
    };
    (boundary / dir_component, (1.0 / dir_component).abs())
}

/// Perform a raycast against the voxel grid
///
/// Uses a fast voxel traversal algorithm (Amanatides & Woo)
pub fn raycast(
    grid: &VoxelGrid,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<RaycastResult> {
    // Normalize direction
    let dir = direction.normalize();

    // Current voxel position
    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    // Step direction
    let step_x = if dir.x > 0.0 { 1 } else { -1 };
    let step_y = if dir.y > 0.0 { 1 } else { -1 };
    let step_z = if dir.z > 0.0 { 1 } else { -1 };

    // Distance to next voxel boundary and distance to traverse one voxel,
    // per axis.
    let (mut t_max_x, t_delta_x) = axis_traversal(origin.x, dir.x, x, step_x);
    let (mut t_max_y, t_delta_y) = axis_traversal(origin.y, dir.y, y, step_y);
    let (mut t_max_z, t_delta_z) = axis_traversal(origin.z, dir.z, z, step_z);

    // Face normal of the last entered voxel
    let mut normal = IVec3::ZERO;

    // Traversal loop
    let mut distance = 0.0;

    while distance <= max_distance {
        // Check if current voxel is solid
        let pos = IVec3::new(x, y, z);
        if grid.material_at(pos).is_some_and(|id| id != 0) {
            // Assuming 0 is air/empty
            return Some(RaycastResult {
                block_pos: pos,
                normal,
                position: origin + dir * distance,
                distance,
            });
        }

        // Move to next voxel
        if t_max_x < t_max_y {
            if t_max_x < t_max_z {
                x += step_x;
                distance = t_max_x;
                t_max_x += t_delta_x;
                normal = IVec3::new(-step_x, 0, 0);
            } else {
                z += step_z;
                distance = t_max_z;
                t_max_z += t_delta_z;
                normal = IVec3::new(0, 0, -step_z);
            }
        } else if t_max_y < t_max_z {
            y += step_y;
            distance = t_max_y;
            t_max_y += t_delta_y;
            normal = IVec3::new(0, -step_y, 0);
        } else {
            z += step_z;
            distance = t_max_z;
            t_max_z += t_delta_z;
            normal = IVec3::new(0, 0, -step_z);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raycast_hits_block_along_axis_aligned_ray() {
        // Regression test: dir.x == 0.0 and dir.y == 0.0 here (a ray pointed
        // straight along +Z, e.g. a player looking due north/south with zero
        // pitch) previously divided by zero in the DDA setup and never found
        // the block.
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, 0, 0);
        grid.mutator().place(pos, 1, None);

        let hit = raycast(&grid, Vec3::new(0.5, 0.5, -5.0), Vec3::Z, 10.0);

        assert_eq!(hit.map(|h| h.block_pos), Some(pos));
    }

    #[test]
    fn raycast_hits_block_looking_straight_down() {
        // dir.x == 0.0 and dir.z == 0.0 — the straight-down/up case.
        let mut grid = VoxelGrid::new(16);
        let pos = BlockPos::new(0, -5, 0);
        grid.mutator().place(pos, 1, None);

        let hit = raycast(&grid, Vec3::new(0.5, 0.5, 0.5), Vec3::NEG_Y, 10.0);

        assert_eq!(hit.map(|h| h.block_pos), Some(pos));
    }

    #[test]
    fn raycast_returns_none_when_nothing_in_range() {
        let grid = VoxelGrid::new(16);

        let hit = raycast(&grid, Vec3::ZERO, Vec3::Z, 10.0);

        assert!(hit.is_none());
    }
}
