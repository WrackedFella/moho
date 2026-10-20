//! Flat cap faces that close a chunk's solid ground at a horizontal cut.

use super::super::grid::VoxelGrid;
use super::VoxelMesh;
use glam::IVec3;

/// How far above the cut the solid test samples, so a surface lying exactly at the cut
/// gets no coplanar cap and the slice never passes through marching-cubes vertices.
pub const CAP_PROBE: f32 = 1e-3;

/// The flat face at `y = cut` covering the part of chunk `chunk_pos`'s cells (meshed at
/// `lod`) that is solid at `cut + CAP_PROBE`. Every vertex faces `+Y`, with full
/// ambient light, no block light and geometry type 0. A chunk the cut does not cross,
/// or with no solid there, gives an empty mesh.
pub fn cut_cap_mesh(_grid: &VoxelGrid, _chunk_pos: IVec3, _lod: u8, _cut: f32) -> VoxelMesh {
    VoxelMesh::empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::HybridMeshGenerator;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    const SMOOTH: u32 = 0;
    const BLOCKY: u32 = 100;
    const CHUNK: i32 = 16;
    /// How far a cap edge may sit from the wall slice it should follow.
    const WALL_TOLERANCE: f64 = 1e-4;
    const AREA_TOLERANCE: f64 = 1e-3;
    /// Marching cubes cuts the corner of every convex vertical edge of a solid, so the
    /// cross-section of a block-aligned box loses an eighth of a block per corner.
    const CORNER_CHAMFER: f64 = 0.125;
    /// Smooth `flat_ground` cross-section inside its chunk: it ends half a block short of
    /// the chunk's far x and z edges, and only that one corner is a convex edge.
    const GROUND_CAP_AREA: f64 = 15.5 * 15.5 - CORNER_CHAMFER;

    type Point = [f64; 3];
    type Tri = [Point; 3];

    /// A slice segment of the chunk mesh in the (x, z) plane, with the horizontal part
    /// of its triangle's outward normal (the air side).
    struct Wall {
        a: [f64; 2],
        b: [f64; 2],
        outward: [f64; 2],
    }

    fn triangles(mesh: &VoxelMesh) -> Vec<Tri> {
        let at = |i: u32| mesh.vertices[i as usize].map(f64::from);
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(at))
            .collect()
    }

    fn sub(a: Point, b: Point) -> Point {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn cross(a: Point, b: Point) -> Point {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn fill(grid: &mut VoxelGrid, min: [i32; 3], max: [i32; 3], material: u32) {
        grid.mutator()
            .fill_region(IVec3::from(min), IVec3::from(max), material);
    }

    /// Ground over the whole chunk column, blocks `y < top` solid.
    fn flat_ground(material: u32, top: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(CHUNK);
        fill(&mut grid, [0, 0, 0], [15, top - 1, 15], material);
        grid
    }

    fn cap(grid: &VoxelGrid, cut: f32) -> VoxelMesh {
        cut_cap_mesh(grid, IVec3::ZERO, 0, cut)
    }

    /// Area of the cap's triangles seen from above.
    fn cap_area(mesh: &VoxelMesh) -> f64 {
        triangles(mesh)
            .iter()
            .map(|t| cross(sub(t[1], t[0]), sub(t[2], t[0]))[1].abs() / 2.0)
            .sum()
    }

    /// (min, max) of the cap's vertices on x and z.
    fn footprint(mesh: &VoxelMesh) -> ([f32; 2], [f32; 2]) {
        let fold = |axis: usize| {
            mesh.vertices
                .iter()
                .fold([f32::MAX, f32::MIN], |[lo, hi], v| {
                    [lo.min(v[axis]), hi.max(v[axis])]
                })
        };
        (fold(0), fold(2))
    }

    fn assert_near(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() < AREA_TOLERANCE,
            "{what}: {actual} is not {expected}"
        );
    }

    fn assert_empty(mesh: &VoxelMesh, why: &str) {
        assert!(
            mesh.vertices.is_empty() && mesh.indices.is_empty(),
            "{why}: {} triangles",
            mesh.indices.len() / 3
        );
    }

    fn assert_well_formed_cap(mesh: &VoxelMesh, cut: f32) {
        let n = mesh.vertices.len();
        assert_eq!(mesh.normals.len(), n, "normals");
        assert_eq!(mesh.ambient_occlusion.len(), n, "ambient_occlusion");
        assert_eq!(mesh.geometry_type.len(), n, "geometry_type");
        assert_eq!(mesh.light_level.len(), n, "light_level");
        assert_eq!(mesh.block_light_rgb.len(), n, "block_light_rgb");
        assert_eq!(mesh.sky_exposed.len(), n, "sky_exposed");
        assert_eq!(mesh.indices.len() % 3, 0, "whole triangles");
        assert!(mesh.indices.iter().all(|&i| (i as usize) < n), "indices");
        for (i, v) in mesh.vertices.iter().enumerate() {
            assert!(
                (v[1] - cut).abs() < 1e-6,
                "vertex {i} is at y={}, not at the cut {cut}",
                v[1]
            );
            assert_eq!(mesh.normals[i], [0.0, 1.0, 0.0], "normal {i}");
            assert_eq!(mesh.ambient_occlusion[i], 1.0, "ao {i}");
            assert_eq!(mesh.sky_exposed[i], 1.0, "sky exposure {i}");
            assert_eq!(mesh.block_light_rgb[i], [0.0; 3], "block light {i}");
            assert_eq!(mesh.geometry_type[i], 0, "geometry type {i}");
        }
        for (i, t) in triangles(mesh).iter().enumerate() {
            let facing = cross(sub(t[1], t[0]), sub(t[2], t[0]))[1];
            assert!(facing >= 0.0, "triangle {i} faces down (cross y {facing})");
        }
    }

    /// The solid cross-section at height `h` of `mesh`, as oriented wall segments.
    fn slice(mesh: &VoxelMesh, h: f64) -> Vec<Wall> {
        triangles(mesh)
            .into_iter()
            .filter_map(|t| {
                let above = t.map(|v| v[1] >= h);
                if above.iter().all(|&a| a) || above.iter().all(|&a| !a) {
                    return None;
                }
                let crossings: Vec<[f64; 2]> = [(0, 1), (1, 2), (2, 0)]
                    .into_iter()
                    .filter(|&(i, j)| above[i] != above[j])
                    .map(|(i, j)| {
                        let s = (h - t[i][1]) / (t[j][1] - t[i][1]);
                        [
                            t[i][0] + s * (t[j][0] - t[i][0]),
                            t[i][2] + s * (t[j][2] - t[i][2]),
                        ]
                    })
                    .collect();
                let normal = cross(sub(t[1], t[0]), sub(t[2], t[0]));
                Some(Wall {
                    a: crossings[0],
                    b: crossings[1],
                    outward: [normal[0], normal[2]],
                })
            })
            .collect()
    }

    /// Area enclosed by closed wall loops: shoelace over segments oriented with the
    /// solid on the left. Independent of how a cap triangulates it.
    fn enclosed_area(walls: &[Wall]) -> f64 {
        let twice: f64 = walls
            .iter()
            .map(|w| {
                let d = [w.b[0] - w.a[0], w.b[1] - w.a[1]];
                let left = [-d[1], d[0]];
                let (p, q) = if left[0] * w.outward[0] + left[1] * w.outward[1] < 0.0 {
                    (w.a, w.b)
                } else {
                    (w.b, w.a)
                };
                p[0] * q[1] - q[0] * p[1]
            })
            .sum();
        twice.abs() / 2.0
    }

    fn covers(tris: &[Tri], x: f64, z: f64) -> usize {
        const MARGIN: f64 = 1e-9;
        let edge = |a: Point, b: Point| (b[0] - a[0]) * (z - a[2]) - (b[2] - a[2]) * (x - a[0]);
        tris.iter()
            .filter(|t| {
                let e = [edge(t[0], t[1]), edge(t[1], t[2]), edge(t[2], t[0])];
                e.iter().all(|&v| v > MARGIN) || e.iter().all(|&v| v < -MARGIN)
            })
            .count()
    }

    fn distance_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
        let ab = [b[0] - a[0], b[1] - a[1]];
        let len2 = ab[0] * ab[0] + ab[1] * ab[1];
        let s = if len2 == 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0)
        };
        (p[0] - (a[0] + s * ab[0])).hypot(p[1] - (a[1] + s * ab[1]))
    }

    fn intern(points: &mut Vec<[f64; 2]>, p: [f64; 2]) -> usize {
        let near = points
            .iter()
            .position(|q| (q[0] - p[0]).hypot(q[1] - p[1]) < 1e-5);
        near.unwrap_or_else(|| {
            points.push(p);
            points.len() - 1
        })
    }

    /// Edges used by exactly one triangle, with coincident vertices merged and every
    /// edge split at vertices lying on it, so collinear splits cannot fake a boundary.
    fn boundary_edges(mesh: &VoxelMesh) -> Vec<([f64; 2], [f64; 2])> {
        let mut points = Vec::new();
        let ids: Vec<usize> = mesh
            .vertices
            .iter()
            .map(|v| intern(&mut points, [f64::from(v[0]), f64::from(v[2])]))
            .collect();
        let mut uses: BTreeMap<(usize, usize), u32> = BTreeMap::new();
        for (tri, corners) in mesh.indices.as_chunks::<3>().0.iter().zip(triangles(mesh)) {
            let [ia, ib, ic] = tri.map(|i| ids[i as usize]);
            let area = cross(sub(corners[1], corners[0]), sub(corners[2], corners[0]))[1].abs();
            if area < 1e-12 {
                continue;
            }
            for (from, to) in [(ia, ib), (ib, ic), (ic, ia)] {
                let (start, end) = (points[from], points[to]);
                let delta = [end[0] - start[0], end[1] - start[1]];
                let len = delta[0].hypot(delta[1]);
                let mut along: Vec<(f64, usize)> = points
                    .iter()
                    .enumerate()
                    .filter_map(|(k, &pt)| {
                        let rel = [pt[0] - start[0], pt[1] - start[1]];
                        let s = (rel[0] * delta[0] + rel[1] * delta[1]) / len;
                        let off = (rel[0] * delta[1] - rel[1] * delta[0]).abs() / len;
                        (k != from && k != to && off < 1e-6 && s > 1e-6 && s < len - 1e-6)
                            .then_some((s, k))
                    })
                    .collect();
                along.sort_by(|lhs, rhs| lhs.0.total_cmp(&rhs.0));
                let chain: Vec<usize> = std::iter::once(from)
                    .chain(along.into_iter().map(|(_, k)| k))
                    .chain(std::iter::once(to))
                    .collect();
                for pair in chain.windows(2) {
                    *uses
                        .entry((pair[0].min(pair[1]), pair[0].max(pair[1])))
                        .or_default() += 1;
                }
            }
        }
        uses.into_iter()
            .filter(|&(_, n)| n == 1)
            .map(|((from, to), _)| (points[from], points[to]))
            .collect()
    }

    /// Whether `p` lies on the square outline of the cell range `lo..hi` on both axes.
    fn on_outline(p: [f64; 2], lo: f64, hi: f64) -> bool {
        [lo, hi].iter().any(|&edge| {
            (p[0] - edge).abs() < WALL_TOLERANCE || (p[1] - edge).abs() < WALL_TOLERANCE
        })
    }

    /// A convex region of blocks: a box cut by a plane. A cell of such a field never has
    /// two solid corners on a diagonal of a face, so marching cubes leaves no cracks and
    /// the slice of the mesh is a closed curve.
    #[derive(Debug, Clone)]
    struct Field {
        lo: [i32; 3],
        hi: [i32; 3],
        normal: [i32; 3],
        offset: i32,
    }

    impl Field {
        fn solid(&self, p: [i32; 3]) -> bool {
            (0..3).all(|k| self.lo[k] <= p[k] && p[k] <= self.hi[k])
                && (0..3).map(|k| self.normal[k] * p[k]).sum::<i32>() < self.offset
        }

        fn grid(&self, material: u32) -> VoxelGrid {
            let mut grid = VoxelGrid::new(CHUNK);
            for x in self.lo[0]..=self.hi[0] {
                for y in self.lo[1]..=self.hi[1] {
                    for z in self.lo[2]..=self.hi[2] {
                        if self.solid([x, y, z]) {
                            grid.place_block(IVec3::new(x, y, z), material, None);
                        }
                    }
                }
            }
            grid
        }
    }

    /// Fields inside the blocks `min..=max` on every axis.
    fn fields(min: i32, max: i32) -> impl Strategy<Value = Field> {
        let axis = move || {
            (min..=max - 2, 2..=10i32).prop_map(move |(lo, size)| (lo, (lo + size).min(max)))
        };
        (
            axis(),
            axis(),
            axis(),
            (-3..=3i32, -3..=3i32, -3..=3i32),
            -12..=12i32,
        )
            .prop_map(|(x, y, z, normal, shift)| {
                let normal = [normal.0, normal.1, normal.2];
                let centre = [(x.0 + x.1) / 2, (y.0 + y.1) / 2, (z.0 + z.1) / 2];
                Field {
                    lo: [x.0, y.0, z.0],
                    hi: [x.1, y.1, z.1],
                    normal,
                    offset: (0..3).map(|k| normal[k] * centre[k]).sum::<i32>() + shift,
                }
            })
    }

    #[test]
    fn cap_covers_solid_ground_at_the_cut() {
        let grid = flat_ground(SMOOTH, 5);

        for cut in [1.0, 2.5, 3.0] {
            let mesh = cap(&grid, cut);

            assert!(
                !mesh.indices.is_empty(),
                "cut {cut} through solid ground has a cap"
            );
            assert_well_formed_cap(&mesh, cut);
            assert_near(
                cap_area(&mesh),
                GROUND_CAP_AREA,
                &format!("cap area at {cut}"),
            );
            let (xs, zs) = footprint(&mesh);
            assert_eq!(xs, [0.5, 16.0], "x extent at {cut}");
            assert_eq!(zs, [0.5, 16.0], "z extent at {cut}");
        }
    }

    #[test]
    fn cap_leaves_a_cave_open() {
        let mut grid = VoxelGrid::new(CHUNK);
        fill(&mut grid, [2, 2, 2], [13, 13, 13], SMOOTH);
        for x in 5..=10 {
            for y in 5..=10 {
                for z in 5..=10 {
                    grid.clear_block(IVec3::new(x, y, z));
                }
            }
        }

        let outer = 12.0 * 12.0 - 4.0 * CORNER_CHAMFER;
        let hollow = 6.0 * 6.0 - 4.0 * CORNER_CHAMFER;
        let through_cave = cap(&grid, 7.5);
        let under_floor = cap(&grid, 4.0);
        let over_ceiling = cap(&grid, 12.0);

        assert_well_formed_cap(&through_cave, 7.5);
        assert_near(
            cap_area(&through_cave),
            outer - hollow,
            "shell area at the cave",
        );
        for t in triangles(&through_cave) {
            let (x, z) = (
                (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                (t[0][2] + t[1][2] + t[2][2]) / 3.0,
            );
            assert!(
                !(5.0 < x && x < 11.0 && 5.0 < z && z < 11.0),
                "cap triangle centred at ({x}, {z}) lies over the hollow"
            );
            assert!(
                (2.0..=14.0).contains(&x) && (2.0..=14.0).contains(&z),
                "inside the shell"
            );
        }
        assert_near(cap_area(&under_floor), outer, "solid below the cave floor");
        assert_near(
            cap_area(&over_ceiling),
            outer,
            "solid above the cave ceiling",
        );
    }

    #[test]
    fn no_solid_at_the_cut_is_an_empty_cap() {
        let smooth = flat_ground(SMOOTH, 5);
        let blocky = flat_ground(BLOCKY, 5);

        for (name, grid) in [("smooth", &smooth), ("blocky", &blocky)] {
            assert!(
                !cap(grid, 2.5).indices.is_empty(),
                "{name} ground is capped inside the ground"
            );
            assert_empty(&cap(grid, 9.0), &format!("{name}: cut above all ground"));
            assert_empty(&cap(grid, -3.0), &format!("{name}: cut below the chunk"));
            assert_empty(
                &cut_cap_mesh(grid, IVec3::ZERO, 1, 9.0),
                &format!("{name}: LOD 1 cut above all ground"),
            );
        }
        assert_empty(&cap(&VoxelGrid::new(CHUNK), 2.5), "empty grid");
    }

    #[test]
    fn flat_ground_at_the_cut_is_not_capped() {
        let smooth = flat_ground(SMOOTH, 5);
        let blocky = flat_ground(BLOCKY, 5);

        assert_empty(&cap(&smooth, 5.0), "smooth ground surface at the cut");
        assert_empty(&cap(&blocky, 5.0), "blocky ground surface at the cut");
        assert_near(
            cap_area(&cap(&blocky, 4.5)),
            256.0,
            "blocky ground just under its surface is capped",
        );
        assert_near(
            cap_area(&cap(&smooth, 3.0)),
            GROUND_CAP_AREA,
            "smooth ground below its surface is capped",
        );
    }

    #[test]
    fn blocky_block_cap_is_its_square() {
        let mut grid = VoxelGrid::new(CHUNK);
        grid.place_block(IVec3::new(3, 4, 5), BLOCKY, None);

        for cut in [4.0, 4.5] {
            let mesh = cap(&grid, cut);

            assert_well_formed_cap(&mesh, cut);
            assert_near(cap_area(&mesh), 1.0, &format!("area at {cut}"));
            assert_eq!(
                footprint(&mesh),
                ([3.0, 4.0], [5.0, 6.0]),
                "square at {cut}"
            );
        }
        assert_empty(&cap(&grid, 5.0), "cut at the block's top face");
        assert_empty(&cap(&grid, 3.99), "cut under the block");
        grid.place_block(IVec3::new(4, 4, 5), BLOCKY, None);
        assert_near(cap_area(&cap(&grid, 4.5)), 2.0, "two neighbouring blocks");
    }

    #[test]
    fn mixed_chunk_caps_blocky_blocks_as_squares() {
        let mut grid = flat_ground(SMOOTH, 5);
        grid.place_block(IVec3::new(8, 5, 8), BLOCKY, None);

        let through_block = cap(&grid, 5.5);
        let through_ground = cap(&grid, 3.0);

        assert_well_formed_cap(&through_block, 5.5);
        assert_near(
            cap_area(&through_block),
            1.0,
            "only the block spans the cut",
        );
        assert_eq!(footprint(&through_block), ([8.0, 9.0], [8.0, 9.0]));
        assert_near(
            cap_area(&through_ground),
            GROUND_CAP_AREA,
            "the blocky block does not bend the smooth ground",
        );
    }

    #[test]
    fn lod1_cap_matches_coarse_cells() {
        let mut grid = VoxelGrid::new(CHUNK);
        for (pos, material) in [
            ([1, 1, 1], SMOOTH),
            ([6, 2, 3], BLOCKY),
            ([1, 3, 1], SMOOTH),
            ([15, 2, 15], SMOOTH),
            ([7, 3, 9], SMOOTH),
            ([12, 5, 2], SMOOTH),
            ([16, 3, 5], SMOOTH),
        ] {
            grid.place_block(IVec3::from(pos), material, None);
        }

        for cut in [1.5f32, 2.0, 3.0, 4.0] {
            let mesh = cut_cap_mesh(&grid, IVec3::ZERO, 1, cut);
            let layer = ((cut + CAP_PROBE) / 2.0).floor() as i32;
            let solid = |cx: i32, cz: i32| {
                (0..2).any(|dx| {
                    (0..2).any(|dy| {
                        (0..2).any(|dz| {
                            grid.is_solid_at(IVec3::new(2 * cx + dx, 2 * layer + dy, 2 * cz + dz))
                        })
                    })
                })
            };
            let expected_cells = (0..8)
                .flat_map(|cx| (0..8).map(move |cz| (cx, cz)))
                .filter(|&(cx, cz)| solid(cx, cz))
                .count();
            assert!(expected_cells > 0, "the fixture has solid cells at {cut}");

            assert_well_formed_cap(&mesh, cut);
            assert_near(
                cap_area(&mesh),
                4.0 * expected_cells as f64,
                &format!("area at {cut}"),
            );
            let tris = triangles(&mesh);
            for cx in 0..8 {
                for cz in 0..8 {
                    for (qx, qz) in [(0.5, 0.5), (1.5, 0.5), (0.5, 1.5), (1.5, 1.5)] {
                        let at = covers(&tris, f64::from(2 * cx) + qx, f64::from(2 * cz) + qz);
                        assert_eq!(
                            at,
                            usize::from(solid(cx, cz)),
                            "cut {cut}: coarse cell ({cx}, {cz}) covered {at} times"
                        );
                    }
                }
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn cap_edge_lies_on_the_wall(
            field in fields(0, 15),
            sixty_fourths in 32..1056i32,
        ) {
            let grid = field.grid(SMOOTH);
            let cut = sixty_fourths as f32 / 64.0;
            let h = f64::from(cut) + f64::from(CAP_PROBE);
            let chunk_mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, IVec3::ZERO, CHUNK);
            let walls = slice(&chunk_mesh, h);

            let mesh = cap(&grid, cut);

            if walls.iter().any(|w| (w.b[0] - w.a[0]).hypot(w.b[1] - w.a[1]) > 0.25) {
                prop_assert!(!mesh.indices.is_empty(), "the cut crosses a wall but nothing is capped");
            }
            for (a, b) in boundary_edges(&mesh) {
                for s in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let p = [a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1])];
                    let on_wall = walls.iter().any(|w| distance_to_segment(p, w.a, w.b) < WALL_TOLERANCE);
                    prop_assert!(
                        on_wall || on_outline(p, 0.5, 16.5),
                        "cap edge point {:?} at cut {} is off the wall",
                        p,
                        cut
                    );
                }
            }
        }
    }

    fn tiling_cuts() -> impl Strategy<Value = f32> {
        prop_oneof![
            (1..=2i32, prop::sample::select(vec![-0.5f32, 0.0, 0.5]))
                .prop_map(|(k, offset)| 16.0 * k as f32 + offset),
            (32..2016i32).prop_map(|n| n as f32 / 64.0),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]

        #[test]
        fn caps_tile_across_chunk_borders(field in fields(1, 30), cut in tiling_cuts()) {
            let grid = field.grid(SMOOTH);
            let h = f64::from(cut) + f64::from(CAP_PROBE);
            let mut walls = Vec::new();
            for (x, y, z) in (0..2).flat_map(|x| (0..2).flat_map(move |y| (0..2).map(move |z| (x, y, z)))) {
                let mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, IVec3::new(x, y, z), CHUNK);
                walls.extend(slice(&mesh, h));
            }
            let expected = enclosed_area(&walls);

            let mut tris = Vec::new();
            let mut total = 0.0;
            for (x, y, z) in (0..2).flat_map(|x| (-1..3).flat_map(move |y| (0..2).map(move |z| (x, y, z)))) {
                let mesh = cut_cap_mesh(&grid, IVec3::new(x, y, z), 0, cut);
                total += cap_area(&mesh);
                tris.extend(triangles(&mesh));
            }

            prop_assert!(
                (total - expected).abs() < AREA_TOLERANCE,
                "summed cap area {} differs from the solid cross-section {} at cut {}",
                total, expected, cut
            );
            for i in 0..24 {
                for j in 0..24 {
                    let (x, z) = (0.31 + f64::from(i) * 1.337, 0.77 + f64::from(j) * 1.291);
                    let n = covers(&tris, x, z);
                    prop_assert!(n <= 1, "point ({}, {}) is covered by {} caps at cut {}", x, z, n, cut);
                }
            }
        }
    }
}
