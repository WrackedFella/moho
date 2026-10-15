use flagstone_voxel::{BlockPos, MarchingCubes, VoxelGrid};
use glam::{IVec3, Vec3};

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

/// Result of [`raycast_surface`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceHit {
    /// The solid block responsible for the surface that was hit
    pub block_pos: BlockPos,
    /// The exact world position of the intersection
    pub position: Vec3,
    /// Distance from ray origin
    pub distance: f32,
}

/// Slack on the barycentric test, so a ray crossing exactly on an edge shared by two
/// triangles is not missed by rounding.
const EDGE_EPSILON: f32 = 1e-4;

/// Upper bound on the cell boundaries a ray of `max_distance` can cross: at most one per
/// axis per unit travelled, plus those of the starting cell. Bounds the walks so a stalled
/// step terminates instead of looping.
fn max_boundary_crossings(max_distance: f32) -> usize {
    3 * (max_distance.max(0.0).ceil() as usize + 1)
}

/// Corner block `i` of the dual-lattice cell whose base block is `base`, in the mesher's
/// corner order.
fn corner_block(base: IVec3, i: usize) -> BlockPos {
    base + IVec3::from_array(MarchingCubes::CORNER_OFFSETS[i].map(|c| c as i32))
}

/// Which of the eight corner blocks of the cell at `base` are solid, in the corner order
/// of the mesher (bit `i` set when corner `i` is solid).
fn cube_index(grid: &VoxelGrid, base: IVec3) -> u8 {
    (0..8)
        .filter(|&i| grid.material_at(corner_block(base, i)).is_some())
        .fold(0, |index, i| index | (1 << i))
}

/// The solid block responsible for the surface at `p` inside the cell at `base` with
/// solid corners `index`: of the cell's solid corner blocks, the one nearest `p`.
fn owning_block(base: IVec3, index: u8, p: Vec3) -> Option<BlockPos> {
    (0..8)
        .filter(|&i| index & (1 << i) != 0)
        .map(|i| corner_block(base, i))
        .min_by(|a, b| {
            let d = |block: &BlockPos| (block.as_vec3() + Vec3::splat(0.5) - p).length_squared();
            d(a).total_cmp(&d(b))
        })
}

/// Distance along the ray to triangle `tri` (Moller-Trumbore, inclusive at the edges), and
/// whether the ray crosses it from the outward side. `None` when the ray misses it or runs
/// parallel to its plane.
fn triangle_crossing(origin: Vec3, dir: Vec3, tri: &[Vec3; 3]) -> Option<(f32, bool)> {
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }

    let to_origin = origin - tri[0];
    let u = to_origin.dot(p) / det;
    let q = to_origin.cross(e1);
    let v = dir.dot(q) / det;
    if u < -EDGE_EPSILON || v < -EDGE_EPSILON || u + v > 1.0 + EDGE_EPSILON {
        return None;
    }

    // The mesher winds triangles so the right-hand normal points away from the solid.
    Some((e2.dot(q) / det, e1.cross(e2).dot(dir) < 0.0))
}

/// Raycast against the *rendered* smooth (marching-cubes) surface.
///
/// Prefer this over [`raycast`] for anything the player aims at. [`raycast`]
/// walks binary voxel cubes, but the renderer draws an isosurface on the dual
/// lattice, roughly two thirds of whose geometry lies inside blocks the grid calls
/// empty, which a cube-stepping ray passes straight through.
///
/// Walks the dual-lattice cells (corners at block centres) the ray passes through and
/// intersects it with the triangles the mesher draws for each, so the hit is the first
/// crossing of the visible surface. Returns `None` when the ray starts inside the surface
/// (its first crossing is a back face), has a zero direction, or reaches nothing within
/// `max_distance`.
///
/// The hit names the solid block, among the eight bounding the crossing cell, nearest the
/// hit point, so the caller can remove it.
pub fn raycast_surface(
    grid: &VoxelGrid,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<SurfaceHit> {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }

    // Cell `c` spans `c + 0.5 .. c + 1.5` in world space; shifting by half a unit puts the
    // cell boundaries on integers, as in `raycast`.
    let mut walk = GridWalk::new(origin - Vec3::splat(0.5), dir);

    let mut t_enter = 0.0f32;
    for _ in 0..=max_boundary_crossings(max_distance) {
        let t_exit = walk.next_crossing();
        let cell = walk.cell;

        let index = cube_index(grid, cell);
        let cell_origin = cell.as_vec3() + Vec3::splat(0.5);
        let nearest = MarchingCubes::cell_triangles(index)
            .filter_map(|tri| triangle_crossing(origin, dir, &tri.map(|v| v + cell_origin)))
            // One cell's exit distance is the next one's entry, so the windows tile the ray.
            .filter(|&(t, _)| (t_enter..=t_exit).contains(&t))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((t, front_facing)) = nearest {
            if !front_facing || t > max_distance {
                return None;
            }
            let position = origin + dir * t;
            let block_pos = owning_block(cell, index, position)?;
            return Some(SurfaceHit {
                block_pos,
                position,
                distance: t,
            });
        }

        walk.advance();
        t_enter = t_exit;
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

/// Amanatides & Woo walk over the unit lattice cells a ray passes through.
struct GridWalk {
    cell: IVec3,
    step: IVec3,
    /// Ray distance at which each axis next crosses a cell boundary.
    t_max: Vec3,
    /// Ray distance needed to cross one cell along each axis.
    t_delta: Vec3,
}

impl GridWalk {
    /// Starts a walk at the cell containing `origin`, on a lattice with boundaries at the
    /// integers. `dir` must be normalized.
    fn new(origin: Vec3, dir: Vec3) -> Self {
        let cell = origin.floor().as_ivec3();
        let step = IVec3::new(
            if dir.x > 0.0 { 1 } else { -1 },
            if dir.y > 0.0 { 1 } else { -1 },
            if dir.z > 0.0 { 1 } else { -1 },
        );
        let (t_max_x, t_delta_x) = axis_traversal(origin.x, dir.x, cell.x, step.x);
        let (t_max_y, t_delta_y) = axis_traversal(origin.y, dir.y, cell.y, step.y);
        let (t_max_z, t_delta_z) = axis_traversal(origin.z, dir.z, cell.z, step.z);
        Self {
            cell,
            step,
            t_max: Vec3::new(t_max_x, t_max_y, t_max_z),
            t_delta: Vec3::new(t_delta_x, t_delta_y, t_delta_z),
        }
    }

    /// Ray distance at which the current cell is left.
    fn next_crossing(&self) -> f32 {
        self.t_max.x.min(self.t_max.y).min(self.t_max.z)
    }

    /// Moves to the next cell along the axis with the nearest boundary (x, then y, win
    /// only when strictly nearer). Returns the ray distance of the boundary crossed and
    /// the normal of the face the ray entered through.
    fn advance(&mut self) -> (f32, IVec3) {
        let axis = if self.t_max.x < self.t_max.y && self.t_max.x < self.t_max.z {
            0
        } else if self.t_max.x >= self.t_max.y && self.t_max.y < self.t_max.z {
            1
        } else {
            2
        };
        let t = self.t_max[axis];
        self.cell[axis] += self.step[axis];
        self.t_max[axis] += self.t_delta[axis];
        let mut normal = IVec3::ZERO;
        normal[axis] = -self.step[axis];
        (t, normal)
    }
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
    let dir = direction.normalize();
    let mut walk = GridWalk::new(origin, dir);

    // Face normal of the last entered voxel
    let mut normal = IVec3::ZERO;
    let mut distance = 0.0;

    for _ in 0..=max_boundary_crossings(max_distance) {
        if distance > max_distance {
            return None;
        }

        // Assuming 0 is air/empty
        if grid.material_at(walk.cell).is_some_and(|id| id != 0) {
            return Some(RaycastResult {
                block_pos: walk.cell,
                normal,
                position: origin + dir * distance,
                distance,
            });
        }

        (distance, normal) = walk.advance();
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
        let mut grid = VoxelGrid::new(16);
        grid.mutator().place(BlockPos::new(0, 0, 11), 1, None);

        let beyond = raycast(&grid, Vec3::ZERO, Vec3::Z, 10.0);

        assert!(beyond.is_none());

        grid.mutator().place(BlockPos::new(0, 0, 10), 1, None);

        let at_limit = raycast(&grid, Vec3::ZERO, Vec3::Z, 10.0);

        assert_eq!(at_limit.map(|h| h.block_pos), Some(BlockPos::new(0, 0, 10)));
    }

    #[test]
    fn raycast_surface_edge_cases() {
        let mut grid = VoxelGrid::new(16);
        grid.mutator().place(BlockPos::new(0, 0, 12), 1, None);
        let origin = Vec3::new(0.5, 0.5, 0.0);

        assert!(raycast_surface(&grid, origin, Vec3::ZERO, 20.0).is_none());
        assert!(raycast_surface(&grid, origin, Vec3::Z, 10.0).is_none());
        let hit = raycast_surface(&grid, origin, Vec3::Z, 13.0).expect("block is within 13");
        assert_eq!(hit.block_pos, BlockPos::new(0, 0, 12));
        assert!((hit.distance - 12.0).abs() < 0.01, "{}", hit.distance);

        let mut lone = VoxelGrid::new(16);
        lone.mutator().place(BlockPos::new(0, 0, 0), 1, None);
        lone.mutator().place(BlockPos::new(0, 0, 3), 1, None);
        let inside = Vec3::splat(0.5);
        assert!(raycast_surface(&lone, inside, Vec3::Z, 20.0).is_none());
    }

    /// A ray aimed at a lone block's centre from a slightly off-lattice origin, so no step
    /// is an exact tie between axes. The far scene's range is long; the near one starts at the
    /// far corner of its cell, so the walk needs many more cells than `ceil(range)` suggests.
    struct Diagonal {
        block: BlockPos,
        origin: Vec3,
    }

    const FAR: Diagonal = Diagonal {
        block: BlockPos::new(17, 17, 17),
        origin: Vec3::new(0.55, 0.5, 0.45),
    };
    const NEAR: Diagonal = Diagonal {
        block: BlockPos::new(2, 2, 2),
        origin: Vec3::new(0.99, 0.97, 0.98),
    };

    impl Diagonal {
        fn grid(&self) -> VoxelGrid {
            let mut grid = VoxelGrid::new(16);
            grid.mutator().place(self.block, 1, None);
            grid
        }

        fn direction(&self) -> Vec3 {
            self.block.as_vec3() + Vec3::splat(0.5) - self.origin
        }

        /// Distance at which the ray enters the block's cube (slab test).
        fn cube_entry(&self) -> f32 {
            let dir = self.direction().normalize();
            let lo = self.block.as_vec3();
            let near = (lo - self.origin) / dir;
            let far = (lo + Vec3::ONE - self.origin) / dir;
            near.min(far).max_element()
        }

        /// Distance at which the ray meets the drawn surface: the near face of the
        /// octahedron of radius 0.5 around the block centre, `|p - c|_1 = 0.5`.
        fn surface_entry(&self) -> f32 {
            let dir = self.direction().normalize();
            let to_centre = self.block.as_vec3() + Vec3::splat(0.5) - self.origin;
            let t = (to_centre.element_sum() - 0.5) / dir.element_sum();
            let hit = self.origin + dir * t;
            assert!(
                hit.cmplt(self.block.as_vec3() + Vec3::splat(0.5)).all(),
                "scene must meet the octahedron's (-,-,-) face"
            );
            t
        }
    }

    #[test]
    fn cube_diagonal_ray_reaches_block_at_range_limit() {
        for scene in [FAR, NEAR] {
            let grid = scene.grid();

            let hit = raycast(
                &grid,
                scene.origin,
                scene.direction(),
                scene.cube_entry() + 0.05,
            );

            assert_eq!(
                hit.map(|h| h.block_pos),
                Some(scene.block),
                "{:?}",
                scene.block
            );
        }
    }

    #[test]
    fn cube_diagonal_ray_stops_just_short_of_block() {
        for scene in [FAR, NEAR] {
            let grid = scene.grid();

            let hit = raycast(
                &grid,
                scene.origin,
                scene.direction(),
                scene.cube_entry() - 0.05,
            );

            assert!(hit.is_none(), "{:?}: {hit:?}", scene.block);
        }
    }

    #[test]
    fn surface_diagonal_ray_reaches_block_at_range_limit() {
        for scene in [FAR, NEAR] {
            let grid = scene.grid();
            let entry = scene.surface_entry();

            let hit = raycast_surface(&grid, scene.origin, scene.direction(), entry + 0.05)
                .unwrap_or_else(|| panic!("{:?}: no hit within {}", scene.block, entry + 0.05));

            assert_eq!(hit.block_pos, scene.block);
            assert!(
                (hit.distance - entry).abs() < 0.01,
                "{} vs {entry}",
                hit.distance
            );
        }
    }

    #[test]
    fn surface_diagonal_ray_stops_just_short_of_surface() {
        for scene in [FAR, NEAR] {
            let grid = scene.grid();

            let hit = raycast_surface(
                &grid,
                scene.origin,
                scene.direction(),
                scene.surface_entry() - 0.05,
            );

            assert!(hit.is_none(), "{:?}: {hit:?}", scene.block);
        }
    }
}
