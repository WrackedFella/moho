//! Flat cap faces that close a chunk's solid ground at a horizontal cut.
//!
//! A cap covers exactly the cells the chunk's own mesher drew, cut the way that mesher
//! cut them, so its edge meets the chunk's walls and caps of neighbouring chunks tile.

use super::super::face::FaceDirection;
use super::super::grid::VoxelGrid;
use super::VoxelMesh;
use super::hybrid::{
    COARSE_STRIDE, ChunkContent, DENSITY_SAMPLE_TO_WORLD, HybridMeshGenerator, MeshPlan,
    coarse_cell_solid, coarse_face_material, is_meshed_blocky_in_mixed, mesh_plan,
    sample_face_material,
};
use super::marching_cubes::{MarchingCubes, cube_index, nearest_solid_corner};
use glam::{IVec3, Vec2, Vec3};
use std::ops::RangeInclusive;

/// How far above the cut the solid test samples, so a surface lying exactly at the cut
/// gets no coplanar cap and the slice never passes through marching-cubes vertices.
pub const CAP_PROBE: f32 = 1e-3;

/// Squared distance under which two slice points are the same point.
const SAME_POINT: f32 = 1e-12;

/// How far above the middle height, where the vertical-edge vertices lie, a cell is
/// sliced when asked for exactly that height. A column whose solid starts at the middle
/// has a section only about this wide there, so it must stay well above f32 noise at
/// cell scale; the cap edge then sits at most this far off a slanted wall, the same order
/// as `CAP_PROBE`.
const MIDDLE_SLICE_LIFT: f32 = 4e-3;

/// The chunk layers (`chunk_pos.y`) whose caps can be non-empty at `cut`: only cells
/// within a block of the cut can own it, and those lie in at most two layers.
/// `cut_cap_mesh` is empty for a chunk outside this range.
pub fn cap_chunk_layers(cut: f32, chunk_size: i32) -> RangeInclusive<i32> {
    let size = chunk_size as f32;
    ((cut - 1.0) / size).floor() as i32..=((cut + 1.0) / size).floor() as i32
}

/// The flat face at `y = cut` covering the part of chunk `chunk_pos`'s cells (meshed at
/// `lod`) that is solid at `cut + CAP_PROBE`. Every vertex faces `+Y`, with full
/// ambient light, no block light and geometry type 0. A chunk the cut does not cross,
/// with no solid there, or a non-finite cut gives an empty mesh.
pub fn cut_cap_mesh(grid: &VoxelGrid, chunk_pos: IVec3, lod: u8, cut: f32) -> VoxelMesh {
    if !cut.is_finite() {
        return VoxelMesh::empty();
    }
    let chunk_size = grid.chunk_size();
    let base = chunk_pos * chunk_size;
    let probe = cut + CAP_PROBE;
    let mut cap = CapBuilder::new(cut);
    match mesh_plan(grid, chunk_pos, chunk_size, lod) {
        MeshPlan::Fine(ChunkContent::Empty) => {}
        MeshPlan::Fine(ChunkContent::AllSmooth) => {
            cap_smooth_cells(grid, chunk_pos, chunk_size, false, probe, &mut cap);
        }
        MeshPlan::Fine(ChunkContent::AllBlocky) => {
            cap_blocks(grid, base, chunk_size, probe, |_| true, &mut cap);
        }
        MeshPlan::Fine(ChunkContent::Mixed) => {
            cap_smooth_cells(grid, chunk_pos, chunk_size, true, probe, &mut cap);
            let blocky = |pos| is_meshed_blocky_in_mixed(grid, pos);
            cap_blocks(grid, base, chunk_size, probe, blocky, &mut cap);
        }
        MeshPlan::Coarse => cap_coarse_cells(grid, base, chunk_size, probe, &mut cap),
    }
    cap.mesh
}

/// Caps the marching-cubes cells of the one cell row whose height span `(bottom, top]`
/// holds `probe`, from the same density field the chunk's smooth mesh was built from.
fn cap_smooth_cells(
    grid: &VoxelGrid,
    chunk_pos: IVec3,
    chunk_size: i32,
    only_smooth: bool,
    probe: f32,
    cap: &mut CapBuilder,
) {
    let base = chunk_pos * chunk_size;
    // Cell `i` spans field samples `i..=i + 1`, i.e. world `base + i - 0.5 ..= base + i + 0.5`.
    let field_y = probe - (base.y as f32 + DENSITY_SAMPLE_TO_WORLD);
    // A probe on a row boundary slices the row below at its top, not the row above at
    // its bottom: the horizontal-edge vertices lie there, and a slice that counts them
    // as above (as the chunk mesh's own slice does) finds no wall in the row above.
    let row = field_y.ceil() - 1.0;
    if !(1.0..=chunk_size as f32).contains(&row) {
        return;
    }
    let height = field_y - row;
    let y = row as usize;
    let field = HybridMeshGenerator::create_selective_density_field(
        grid,
        chunk_pos,
        chunk_size,
        only_smooth,
    );
    for x in 1..=chunk_size as usize {
        for z in 1..=chunk_size as usize {
            let cube_index = cube_index(&field, x, y, z);
            let origin = Vec2::new(
                (base.x + x as i32) as f32 + DENSITY_SAMPLE_TO_WORLD,
                (base.z + z as i32) as f32 + DENSITY_SAMPLE_TO_WORLD,
            );
            // Clipped in cell coordinates, where the slice points are exact enough to
            // tell a straight run of wall from a corner.
            for tri in cell_section(cube_index, height)
                .into_iter()
                .flat_map(ear_clip)
            {
                let c = (tri[0] + tri[1] + tri[2]) / 3.0;
                let k = nearest_solid_corner(cube_index, Vec3::new(c.x, height, c.y));
                let material =
                    sample_face_material(grid, base, MarchingCubes::corner_sample(x, y, z, k));
                cap.triangle(tri.map(|p| p + origin), material);
            }
        }
    }
}

/// Caps every block in the chunk's range that spans `probe` and passes `meshed_blocky`.
fn cap_blocks(
    grid: &VoxelGrid,
    base: IVec3,
    chunk_size: i32,
    probe: f32,
    meshed_blocky: impl Fn(IVec3) -> bool,
    cap: &mut CapBuilder,
) {
    let y = probe.floor() as i32;
    if !(base.y..base.y + chunk_size).contains(&y) {
        return;
    }
    for x in base.x..base.x + chunk_size {
        for z in base.z..base.z + chunk_size {
            let pos = IVec3::new(x, y, z);
            if grid.is_solid_at(pos) && meshed_blocky(pos) {
                let material = grid.face_material_at(pos).expect("guarded by is_solid_at");
                cap.square(Vec2::new(x as f32, z as f32), 1, material);
            }
        }
    }
}

/// Caps every coarse (LOD 1) cell of the chunk that spans `probe` and is solid.
fn cap_coarse_cells(
    grid: &VoxelGrid,
    base: IVec3,
    chunk_size: i32,
    probe: f32,
    cap: &mut CapBuilder,
) {
    let cells = chunk_size / COARSE_STRIDE;
    let cy = ((probe - base.y as f32) / COARSE_STRIDE as f32).floor() as i32;
    if !(0..cells).contains(&cy) {
        return;
    }
    for cx in 0..cells {
        for cz in 0..cells {
            if coarse_cell_solid(grid, base, IVec3::new(cx, cy, cz)) {
                let corner = Vec2::new(
                    (base.x + cx * COARSE_STRIDE) as f32,
                    (base.z + cz * COARSE_STRIDE) as f32,
                );
                let face = if probe - base.y as f32 >= (cy * COARSE_STRIDE + 1) as f32 {
                    FaceDirection::PosY
                } else {
                    FaceDirection::NegY
                };
                let material = coarse_face_material(grid, base, IVec3::new(cx, cy, cz), face);
                cap.square(corner, COARSE_STRIDE, material);
            }
        }
    }
}

/// Square corners counter-clockwise in `(x, z)`, in the column order of
/// `MarchingCubes::CORNER_OFFSETS` (corner `k` below, `k + 4` above).
const SQUARE: [Vec2; 4] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(1.0, 0.0),
    Vec2::new(1.0, 1.0),
    Vec2::new(0.0, 1.0),
];

/// The solid part, at `height` in `0 < height <= 1`, of the unit cell's horizontal square in
/// `(x, z)`, as counter-clockwise rings. The boundary is the cell's own triangles sliced
/// at `height`, with the solid on their inner side, plus the stretches of the square's
/// edges that are solid.
///
/// Every slice segment that meets the square's edge ends there (each marching-cubes
/// patch is bounded by the cell's faces), and walking an edge from a corner whose
/// solidity is known toggles at each segment end. A ring that does not close (a crack
/// on an ambiguous face, which the chunk mesh shows too) is left out.
fn cell_section(cube_index: u8, height: f32) -> Vec<Vec<Vec2>> {
    // The vertical-edge vertices sit at the middle height. Slicing through them would
    // count them as above while the rim takes the top corners, so slice a little above
    // them, where both agree.
    let slice_height = if height == 0.5 {
        height + MIDDLE_SLICE_LIFT
    } else {
        height
    };
    let mut segments = slice_walls(cube_index, slice_height);
    // A vertical cell edge changes solidity at its midpoint only.
    let column_solid = |column: usize| {
        let corner = if height < 0.5 { column } else { column + 4 };
        cube_index & (1 << corner) != 0
    };
    let mut rim = Vec::new();
    for (k, &from) in SQUARE.iter().enumerate() {
        let to = SQUARE[(k + 1) % 4];
        let along = to - from;
        // Segment ends on this edge share its fixed coordinate exactly: they are
        // interpolated between two edge midpoints of the face the edge lies in.
        let on_edge = |p: &Vec2| {
            if along.x == 0.0 {
                p.x == from.x
            } else {
                p.y == from.y
            }
        };
        let mut crossings: Vec<Vec2> = segments.iter().flatten().copied().filter(on_edge).collect();
        crossings.sort_by(|a, b| a.dot(along).total_cmp(&b.dot(along)));
        let mut solid = column_solid(k);
        let mut start = from;
        for p in crossings {
            if solid {
                rim.push([start, p]);
            }
            solid = !solid;
            start = p;
        }
        if solid {
            rim.push([start, to]);
        }
    }
    segments.extend(rim);
    chain_rings(segments)
}

/// The cell's triangles sliced at `height`, each segment oriented with the solid on
/// its left in `(x, z)`.
fn slice_walls(cube_index: u8, height: f32) -> Vec<[Vec2; 2]> {
    MarchingCubes::cell_triangles(cube_index)
        .filter_map(|tri| {
            let above = tri.map(|v| v.y >= height);
            let mut ends = [(0, 1), (1, 2), (2, 0)]
                .into_iter()
                .filter(|&(i, j)| above[i] != above[j])
                .map(|(i, j)| {
                    // Interpolating from the lower vertex makes the triangles sharing an
                    // edge compute the identical point, so their segments chain exactly.
                    let (low, high) = if above[i] {
                        (tri[j], tri[i])
                    } else {
                        (tri[i], tri[j])
                    };
                    // Not `Vec3::lerp`: this form keeps a coordinate both ends share exact.
                    let p = low + (high - low) * ((height - low.y) / (high.y - low.y));
                    Vec2::new(p.x, p.z)
                });
            let (a, b) = (ends.next()?, ends.next()?);
            let outward = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
            let along = b - a;
            if along.length_squared() <= SAME_POINT {
                None
            } else if along.perp().dot(Vec2::new(outward.x, outward.z)) < 0.0 {
                Some([a, b])
            } else {
                Some([b, a])
            }
        })
        .collect()
}

/// Joins directed segments end to start into closed rings; segments that never close
/// a ring are dropped.
fn chain_rings(mut segments: Vec<[Vec2; 2]>) -> Vec<Vec<Vec2>> {
    let mut rings = Vec::new();
    while let Some([start, mut end]) = segments.pop() {
        let mut ring = vec![start];
        loop {
            if end.distance_squared(start) <= SAME_POINT {
                rings.push(ring);
                break;
            }
            let Some(next) = segments
                .iter()
                .position(|s| s[0].distance_squared(end) <= SAME_POINT)
            else {
                break;
            };
            ring.push(end);
            end = segments.swap_remove(next)[1];
        }
    }
    rings
}

/// Twice the signed area of `(a, b, c)` in `(x, z)`; positive when counter-clockwise.
fn turn(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b - a).perp_dot(c - a)
}

/// Splits a counter-clockwise simple ring into triangles by ear clipping.
fn ear_clip(mut ring: Vec<Vec2>) -> Vec<[Vec2; 3]> {
    /// How far off the line through its neighbours a vertex may sit and still count
    /// as on it.
    const FLAT: f32 = 1e-6;
    let mut triangles = Vec::new();
    while ring.len() >= 3 {
        let n = ring.len();
        let corner = |i: usize| (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        // A straight or doubled-back vertex adds no area; drop it first.
        if let Some(flat) = (0..n).find(|&i| {
            let (a, b, c) = corner(i);
            turn(a, b, c).abs() <= FLAT * a.distance(c)
        }) {
            ring.remove(flat);
            continue;
        }
        let is_ear = |i: usize| {
            let (a, b, c) = corner(i);
            turn(a, b, c) > 0.0
                && ring.iter().all(|&p| {
                    p == a
                        || p == b
                        || p == c
                        || turn(a, b, p) < 0.0
                        || turn(b, c, p) < 0.0
                        || turn(c, a, p) < 0.0
                })
        };
        // Only a malformed ring (clockwise, or self-touching from a crack) has no ear.
        let Some(ear) = (0..n).find(|&i| is_ear(i)) else {
            break;
        };
        let (a, b, c) = corner(ear);
        triangles.push([a, b, c]);
        ring.remove(ear);
    }
    triangles
}

struct CapBuilder {
    cut: f32,
    mesh: VoxelMesh,
}

impl CapBuilder {
    fn new(cut: f32) -> Self {
        Self {
            cut,
            mesh: VoxelMesh::empty(),
        }
    }

    /// Adds the axis-aligned square of whole-block `size` with its low corner at
    /// `corner` in `(x, z)`, as one-block-deep strips so no triangle edge crosses a
    /// block centre.
    fn square(&mut self, corner: Vec2, size: i32, material: u32) {
        for row in 0..size {
            let near = corner + Vec2::new(0.0, row as f32);
            let far = near + Vec2::new(size as f32, 1.0);
            self.triangle([near, Vec2::new(far.x, near.y), far], material);
            self.triangle([near, far, Vec2::new(near.x, far.y)], material);
        }
    }

    /// Adds a counter-clockwise triangle in `(x, z)`. Counter-clockwise with `z` up the
    /// page is clockwise seen from `+Y`, so the indices are reversed to face up.
    fn triangle(&mut self, [a, b, c]: [Vec2; 3], material: u32) {
        let first = self.mesh.vertices.len() as u32;
        for p in [a, b, c] {
            self.mesh
                .vertices
                .push(Vec3::new(p.x, self.cut, p.y).to_array());
            self.mesh.normals.push([0.0, 1.0, 0.0]);
            self.mesh.ambient_occlusion.push(1.0);
            self.mesh.geometry_type.push(0);
            self.mesh.light_level.push(1.0);
            self.mesh.block_light_rgb.push([0.0; 3]);
            self.mesh.sky_exposed.push(1.0);
            self.mesh.material.push(material);
        }
        self.mesh
            .indices
            .extend_from_slice(&[first, first + 2, first + 1]);
    }
}
#[cfg(test)]
mod tests {
    use super::super::marching_cubes::nearest_solid_corner;
    use super::*;
    use crate::mesh::HybridMeshGenerator;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    const SMOOTH: u32 = 0;
    const BLOCKY: u32 = 100;
    /// A smooth ground that is not material 0, so a cap that defaults to 0 shows.
    const STONE: u32 = 2;
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

    /// Whether (x, z) is inside the closed curve of `walls`, by ray parity towards +x.
    fn inside(walls: &[Wall], x: f64, z: f64) -> bool {
        let crossings = walls
            .iter()
            .filter(|w| {
                (w.a[1] > z) != (w.b[1] > z) && {
                    let t = (z - w.a[1]) / (w.b[1] - w.a[1]);
                    w.a[0] + t * (w.b[0] - w.a[0]) > x
                }
            })
            .count();
        crossings % 2 == 1
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

    fn inside_blocks(min: i32, max: i32) -> BoxedStrategy<(i32, i32)> {
        (min..=max - 2, 2..=10i32)
            .prop_map(move |(lo, size)| (lo, (lo + size).min(max)))
            .boxed()
    }

    /// Per-axis (lo, hi) block ranges that cross the chunk border at 16.
    fn straddling_blocks() -> BoxedStrategy<(i32, i32)> {
        (10..=15i32, 17..=22i32).boxed()
    }

    /// Fields whose box spans `axis` on every axis.
    fn fields_on(axis: BoxedStrategy<(i32, i32)>) -> impl Strategy<Value = Field> {
        (
            axis.clone(),
            axis.clone(),
            axis,
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

    /// Heights in sixty-fourths across the field's own y-span, where its solid is.
    fn heights_through(field: &Field) -> impl Strategy<Value = f32> + use<> {
        (field.lo[1] * 64..=field.hi[1] * 64 + 64).prop_map(|n| n as f32 / 64.0)
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
                &cut_cap_mesh(grid, IVec3::Y, 0, 2.5),
                &format!("{name}: the chunk layer above does not own the cut"),
            );
            assert_empty(
                &cut_cap_mesh(grid, IVec3::NEG_Y, 0, 2.5),
                &format!("{name}: the chunk layer below does not own the cut"),
            );
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

    #[test]
    fn a_non_finite_cut_caps_nothing() {
        let mut grid = VoxelGrid::new(CHUNK);
        fill(&mut grid, [0, 0, 0], [7, 0, 15], SMOOTH);
        fill(&mut grid, [8, 0, 0], [15, 0, 15], BLOCKY);
        let finite = cap(&grid, 0.5);
        assert!(
            !finite.indices.is_empty(),
            "the fixture is capped at a finite cut"
        );

        for cut in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for lod in [0, 1] {
                let mesh = cut_cap_mesh(&grid, IVec3::ZERO, lod, cut);

                assert_empty(&mesh, &format!("cut {cut} at lod {lod}"));
            }
        }
    }

    /// The largest cut whose probe, summed in f32, is exactly `target`.
    fn cut_landing_on(target: f32) -> f32 {
        let start = target - CAP_PROBE;
        (0..16u32)
            .flat_map(|k| {
                [
                    f32::from_bits(start.to_bits() + k),
                    f32::from_bits(start.to_bits() - k),
                ]
            })
            .find(|&c| c + CAP_PROBE == target)
            .unwrap_or_else(|| panic!("no f32 cut has a probe of exactly {target}"))
    }

    #[test]
    fn a_probe_on_a_vertex_height_still_caps_the_section() {
        let mut flat = VoxelGrid::new(CHUNK);
        fill(&mut flat, [1, 1, 1], [14, 9, 14], SMOOTH);
        let mut terraces = VoxelGrid::new(CHUNK);
        fill(&mut terraces, [1, 1, 1], [14, 3, 14], SMOOTH);
        fill(&mut terraces, [3, 4, 3], [12, 6, 12], SMOOTH);
        fill(&mut terraces, [5, 7, 5], [10, 9, 10], SMOOTH);

        let mut failures = Vec::new();
        for (name, grid) in [("flat ground", &flat), ("terraces", &terraces)] {
            let chunk_mesh = HybridMeshGenerator::generate_chunk_mesh(grid, IVec3::ZERO, CHUNK);
            for target in [5.0f32, 5.5, 6.0, 6.5] {
                let cut = cut_landing_on(target);
                assert_eq!(
                    cut + CAP_PROBE,
                    target,
                    "the probe lands on the vertex height"
                );
                let walls = slice(&chunk_mesh, f64::from(target));
                let expected = enclosed_area(&walls);

                let mesh = cap(grid, cut);

                assert!(expected > 1.0, "{name}: the cut at {target} crosses solid");
                let area = cap_area(&mesh);
                if (area - expected).abs() >= AREA_TOLERANCE {
                    failures.push(format!(
                        "{name}: area {area} is not {expected} at probe {target}"
                    ));
                }
                'edges: for (from, to) in boundary_edges(&mesh) {
                    for s in [0.0, 0.5, 1.0] {
                        let p = [
                            from[0] + s * (to[0] - from[0]),
                            from[1] + s * (to[1] - from[1]),
                        ];
                        if !walls
                            .iter()
                            .any(|w| distance_to_segment(p, w.a, w.b) < WALL_TOLERANCE)
                        {
                            failures.push(format!(
                                "{name}: cap edge point {p:?} at probe {target} is off the wall"
                            ));
                            break 'edges;
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn caps_in_negative_chunks() {
        for (material, name, expected) in [
            (SMOOTH, "smooth", 18.0 * 18.0 - 4.0 * CORNER_CHAMFER),
            (BLOCKY, "blocky", 18.0 * 18.0),
        ] {
            let mut grid = VoxelGrid::new(CHUNK);
            fill(&mut grid, [-20, -20, -20], [-3, -3, -3], material);

            for cut in [-10.5f32, -16.0] {
                let mut total = 0.0;
                for (x, y, z) in
                    (-2..0).flat_map(|x| (-3..1).flat_map(move |y| (-2..0).map(move |z| (x, y, z))))
                {
                    let mesh = cut_cap_mesh(&grid, IVec3::new(x, y, z), 0, cut);
                    if !mesh.indices.is_empty() {
                        assert_well_formed_cap(&mesh, cut);
                    }
                    total += cap_area(&mesh);
                }

                assert_near(
                    total,
                    expected,
                    &format!("{name}: summed cap area at {cut}"),
                );
            }
        }
    }

    #[test]
    fn cap_chunk_layers_spans_the_layers_within_a_block_of_the_cut() {
        let rows = [
            (8.0, 16, 0..=0),
            (16.25, 16, 0..=1),
            (15.5, 16, 0..=1),
            (40.0, 16, 2..=2),
            (-0.5, 16, -1..=0),
            (-20.0, 16, -2..=-2),
            (100.0, 16, 6..=6),
            (8.0, 8, 0..=1),
            (20.0, 8, 2..=2),
        ];

        for (cut, size, expected) in rows {
            assert_eq!(
                cap_chunk_layers(cut, size),
                expected,
                "cut {cut}, chunk size {size}"
            );
        }
    }

    #[test]
    fn a_coarse_chunk_off_the_origin_is_capped_in_its_own_cells() {
        for (layer, block_y, cut) in [(1, 18, 18.0f32), (-1, -14, -14.0)] {
            let mut grid = VoxelGrid::new(CHUNK);
            grid.place_block(IVec3::new(1, block_y, 1), SMOOTH, None);

            let mesh = cut_cap_mesh(&grid, IVec3::new(0, layer, 0), 1, cut);

            assert_near(
                cap_area(&mesh),
                4.0,
                &format!("coarse cell cap in layer {layer}"),
            );
            assert_eq!(footprint(&mesh), ([0.0, 2.0], [0.0, 2.0]));
        }
    }

    #[test]
    fn a_probe_on_the_surface_of_flat_ground_caps_nothing() {
        let mut grid = VoxelGrid::new(CHUNK);
        fill(&mut grid, [1, 1, 1], [14, 4, 14], SMOOTH);
        let cut = cut_landing_on(5.0);
        assert_eq!(cut + CAP_PROBE, 5.0);

        let mesh = cap(&grid, cut);

        assert_empty(&mesh, "the probe lies on the ground's top surface");
    }

    fn ring_area(ring: &[Vec2]) -> f32 {
        (0..ring.len())
            .map(|i| ring[i].perp_dot(ring[(i + 1) % ring.len()]))
            .sum::<f32>()
            / 2.0
    }

    fn in_ring(ring: &[Vec2], p: Vec2) -> bool {
        (0..ring.len())
            .filter(|&i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                (a.y > p.y) != (b.y > p.y) && a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) > p.x
            })
            .count()
            % 2
            == 1
    }

    /// Triangles that are all counter-clockwise, cover the ring's area, and cover every
    /// sample point inside it once and every point outside it never.
    fn assert_triangulates(ring: &[Vec2], tris: &[[Vec2; 3]]) {
        for t in tris {
            assert!(
                turn(t[0], t[1], t[2]) > 0.0,
                "{t:?} is not counter-clockwise"
            );
        }
        let area: f32 = tris.iter().map(|t| turn(t[0], t[1], t[2]) / 2.0).sum();
        assert!(
            (area - ring_area(ring)).abs() < 1e-4,
            "triangles cover {area}, ring is {}",
            ring_area(ring)
        );
        for i in 0..40 {
            for j in 0..40 {
                let p = Vec2::new(0.0713 + i as f32 * 0.1013, 0.0391 + j as f32 * 0.1007);
                let covering = tris
                    .iter()
                    .filter(|t| {
                        turn(t[0], t[1], p) > 1e-6
                            && turn(t[1], t[2], p) > 1e-6
                            && turn(t[2], t[0], p) > 1e-6
                    })
                    .count();
                assert_eq!(covering, usize::from(in_ring(ring, p)), "point {p:?}");
            }
        }
    }

    /// Failures of the section's ring orientation, triangulated area and per-column
    /// coverage over every cube index at `heights`.
    fn section_failures(heights: &[f32], inset: f32) -> Vec<String> {
        let mut failures = Vec::new();
        for index in 1..=254u8 {
            for &height in heights {
                let rings = cell_section(index, height);
                let mut tris = Vec::new();
                for ring in &rings {
                    let area = ring_area(ring);
                    if area <= 0.0 {
                        failures.push(format!("index {index} height {height}: ring area {area} is not counter-clockwise"));
                    }
                    let clipped = ear_clip(ring.clone());
                    let clipped_area: f32 =
                        clipped.iter().map(|t| turn(t[0], t[1], t[2]) / 2.0).sum();
                    if (clipped_area - area).abs() > 1e-5 {
                        failures.push(format!("index {index} height {height}: triangles cover {clipped_area}, ring is {area}"));
                    }
                    tris.extend(clipped);
                }
                for (column, &corner) in SQUARE.iter().enumerate() {
                    let bit = if height < 0.5 { column } else { column + 4 };
                    let solid = index & (1 << bit) != 0;
                    let inward = (Vec2::splat(0.5) - corner).signum() * inset;
                    let p = corner + inward;
                    let covered = tris.iter().any(|t| {
                        turn(t[0], t[1], p) >= 0.0
                            && turn(t[1], t[2], p) >= 0.0
                            && turn(t[2], t[0], p) >= 0.0
                    });
                    if covered != solid {
                        failures.push(format!("index {index} height {height}: column {column} solid {solid} but covered {covered}"));
                    }
                }
            }
        }
        failures
    }

    #[test]
    fn cell_section_covers_each_solid_column_for_every_cube_index() {
        let heights: Vec<f32> = (1..=64)
            .filter(|&k| k != 32)
            .map(|k| k as f32 / 64.0)
            .collect();

        let failures = section_failures(&heights, 1e-3);

        assert!(
            failures.is_empty(),
            "{} failures, first: {:#?}",
            failures.len(),
            &failures[..failures.len().min(10)]
        );
    }

    #[test]
    fn cell_section_at_the_middle_height_covers_each_solid_column() {
        // A column whose solid starts at the middle height has a section only about as
        // wide as the slice lift, so its corner is probed inside that.
        let failures = section_failures(&[0.5], MIDDLE_SLICE_LIFT / 4.0);

        assert!(
            failures.is_empty(),
            "{} failures, first: {:#?}",
            failures.len(),
            &failures[..failures.len().min(10)]
        );
    }

    #[test]
    fn ear_clip_skips_an_ear_that_holds_a_reflex_vertex() {
        let ring = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(4.0, 4.0),
            Vec2::new(2.0, 1.0),
            Vec2::new(0.0, 4.0),
        ];

        let tris = ear_clip(ring.clone());

        assert_eq!(tris.len(), 3);
        assert_triangulates(&ring, &tris);
    }

    #[test]
    fn ear_clip_skips_an_ear_with_a_vertex_on_its_edge() {
        // (2, 2) lies on the diagonal of the ear at (4, 0) and the ring bends inward there.
        let ring = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(4.0, 4.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 4.0),
        ];

        let tris = ear_clip(ring.clone());

        assert_triangulates(&ring, &tris);
    }

    #[test]
    fn ear_clip_picks_the_first_ear_no_other_vertex_touches() {
        let cases: [(&[Vec2], &[[Vec2; 3]]); 4] = [
            (
                &[
                    Vec2::new(2.0, 1.0),
                    Vec2::new(2.0, 0.0),
                    Vec2::new(4.0, 0.0),
                    Vec2::new(3.0, 4.0),
                    Vec2::new(0.0, 3.0),
                ],
                &[
                    [
                        Vec2::new(2.0, 1.0),
                        Vec2::new(2.0, 0.0),
                        Vec2::new(4.0, 0.0),
                    ],
                    [
                        Vec2::new(0.0, 3.0),
                        Vec2::new(2.0, 1.0),
                        Vec2::new(4.0, 0.0),
                    ],
                    [
                        Vec2::new(0.0, 3.0),
                        Vec2::new(4.0, 0.0),
                        Vec2::new(3.0, 4.0),
                    ],
                ],
            ),
            (
                &[
                    Vec2::new(0.0, 1.0),
                    Vec2::new(2.0, 1.0),
                    Vec2::new(2.0, 0.0),
                    Vec2::new(4.0, 1.0),
                    Vec2::new(3.0, 4.0),
                    Vec2::new(1.0, 3.0),
                ],
                &[
                    [
                        Vec2::new(1.0, 3.0),
                        Vec2::new(0.0, 1.0),
                        Vec2::new(2.0, 1.0),
                    ],
                    [
                        Vec2::new(2.0, 1.0),
                        Vec2::new(2.0, 0.0),
                        Vec2::new(4.0, 1.0),
                    ],
                    [
                        Vec2::new(1.0, 3.0),
                        Vec2::new(2.0, 1.0),
                        Vec2::new(4.0, 1.0),
                    ],
                    [
                        Vec2::new(1.0, 3.0),
                        Vec2::new(4.0, 1.0),
                        Vec2::new(3.0, 4.0),
                    ],
                ],
            ),
            (
                &[
                    Vec2::new(3.0, 0.0),
                    Vec2::new(4.0, 0.0),
                    Vec2::new(4.0, 1.0),
                    Vec2::new(3.0, 4.0),
                    Vec2::new(2.0, 4.0),
                    Vec2::new(1.0, 4.0),
                    Vec2::new(2.0, 3.0),
                ],
                &[
                    [
                        Vec2::new(2.0, 3.0),
                        Vec2::new(3.0, 0.0),
                        Vec2::new(4.0, 0.0),
                    ],
                    [
                        Vec2::new(2.0, 3.0),
                        Vec2::new(4.0, 0.0),
                        Vec2::new(4.0, 1.0),
                    ],
                    [
                        Vec2::new(1.0, 4.0),
                        Vec2::new(4.0, 1.0),
                        Vec2::new(3.0, 4.0),
                    ],
                ],
            ),
            (
                &[
                    Vec2::new(0.0, 1.0),
                    Vec2::new(3.0, 0.0),
                    Vec2::new(4.0, 1.0),
                    Vec2::new(3.0, 2.0),
                    Vec2::new(2.0, 4.0),
                    Vec2::new(2.0, 3.0),
                    Vec2::new(1.0, 2.0),
                ],
                &[
                    [
                        Vec2::new(2.0, 3.0),
                        Vec2::new(0.0, 1.0),
                        Vec2::new(3.0, 0.0),
                    ],
                    [
                        Vec2::new(3.0, 0.0),
                        Vec2::new(4.0, 1.0),
                        Vec2::new(3.0, 2.0),
                    ],
                    [
                        Vec2::new(2.0, 3.0),
                        Vec2::new(3.0, 0.0),
                        Vec2::new(3.0, 2.0),
                    ],
                    [
                        Vec2::new(2.0, 3.0),
                        Vec2::new(3.0, 2.0),
                        Vec2::new(2.0, 4.0),
                    ],
                ],
            ),
        ];

        for (ring, expected) in cases {
            assert_eq!(ear_clip(ring.to_vec()), expected, "ring {ring:?}");
        }
    }

    #[test]
    fn ear_clip_drops_a_vertex_by_its_offset_from_the_line_not_the_area() {
        let square = |side: f32, off: f32| {
            vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(side / 2.0, -off),
                Vec2::new(side, 0.0),
                Vec2::new(side, side),
                Vec2::new(0.0, side),
            ]
        };

        let far_and_nearly_straight = ear_clip(square(2.0, 5e-7));
        let near_and_bent = ear_clip(square(0.01, 1e-5));

        assert_eq!(
            far_and_nearly_straight.len(),
            2,
            "5e-7 off a 2 long line is straight"
        );
        assert_eq!(
            near_and_bent.len(),
            3,
            "1e-5 off a 0.01 long line is a corner"
        );
    }

    #[test]
    fn blocky_block_on_a_chunk_layer_border_is_capped_once() {
        let mut grid = VoxelGrid::new(CHUNK);
        grid.place_block(IVec3::new(3, 16, 5), BLOCKY, None);
        grid.place_block(IVec3::new(3, 15, 5), BLOCKY, None);

        for (cut, owner) in [(16.25f32, IVec3::Y), (15.25, IVec3::ZERO)] {
            let layers = [IVec3::ZERO, IVec3::Y].map(|pos| cut_cap_mesh(&grid, pos, 0, cut));

            let areas = layers.each_ref().map(cap_area);
            let expected = [IVec3::ZERO, IVec3::Y].map(|pos| if pos == owner { 1.0 } else { 0.0 });
            assert_eq!(areas, expected, "cap area per chunk layer at {cut}");
        }
    }

    /// Each cap triangle's centroid and its three vertex materials.
    fn cap_triangle_materials(mesh: &VoxelMesh) -> Vec<(Vec3, [u32; 3])> {
        assert_eq!(
            mesh.material.len(),
            mesh.vertices.len(),
            "one material per vertex"
        );
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|tri| {
                let at = |k: usize| Vec3::from(mesh.vertices[tri[k] as usize]);
                (
                    (at(0) + at(1) + at(2)) / 3.0,
                    tri.map(|i| mesh.material[i as usize]),
                )
            })
            .collect()
    }

    fn face_material_at(grid: &VoxelGrid, pos: IVec3) -> u32 {
        let material = grid.material_at(pos).expect("a solid block");
        grid.material_registry
            .face_material(material, grid.resource_at(pos))
    }

    #[test]
    fn cap_carries_the_material_of_the_ground_it_cuts() {
        let mut grid = VoxelGrid::new(CHUNK);
        for x in 2..=11 {
            for y in 2..=5 {
                for z in 2..=10 {
                    let soil = x >= 7;
                    grid.place_block(IVec3::new(x, y, z), if soil { 1 } else { 2 }, None);
                }
            }
        }
        grid.place_block(IVec3::new(4, 4, 6), 2, Some(1));
        let cut = 4.5;

        let mesh = cap(&grid, cut);

        let mut seen = std::collections::BTreeSet::new();
        for (centroid, materials) in cap_triangle_materials(&mesh) {
            // The block whose column centre is nearest the triangle; it is the one the
            // cap takes unless it is air, which only happens along the cap's rim.
            let block = IVec3::new(centroid.x.floor() as i32, 4, centroid.z.floor() as i32);
            if grid.is_solid_at(block) {
                let expected = face_material_at(&grid, block);
                assert_eq!(materials, [expected; 3], "triangle at {centroid:?}");
                seen.insert(expected);
            }
        }
        assert_eq!(
            seen,
            std::collections::BTreeSet::from([1, 2, 3]),
            "soil, stone and the ore block all show on the cap"
        );
    }

    #[test]
    fn blocky_cap_squares_take_their_blocks_material() {
        let mut grid = VoxelGrid::new(CHUNK);
        grid.place_block(IVec3::new(3, 4, 5), 100, None);
        grid.place_block(IVec3::new(4, 4, 5), 101, Some(1));

        let mesh = cap(&grid, 4.5);

        let triangles = cap_triangle_materials(&mesh);
        assert_eq!(triangles.len(), 4, "two triangles per block");
        for (centroid, materials) in triangles {
            let expected = if centroid.x < 4.0 { 100 } else { 3 };
            assert_eq!(materials, [expected; 3], "triangle at {centroid:?}");
        }
    }

    #[test]
    fn coarse_cap_squares_take_the_face_material_of_the_cut_layer() {
        let cases = [
            (
                vec![([0, 0, 0], 1), ([0, 1, 0], 2)],
                1.5,
                2,
                "probe in the upper layer",
            ),
            (
                vec![([0, 0, 0], 1), ([0, 1, 0], 2)],
                0.5,
                1,
                "probe in the lower layer",
            ),
            (
                vec![([1, 1, 1], 2)],
                0.5,
                2,
                "empty lower layer falls back to the upper",
            ),
        ];

        for (blocks, cut, expected, why) in cases {
            let mut grid = VoxelGrid::new(CHUNK);
            for (pos, material) in blocks {
                grid.place_block(IVec3::from(pos), material, None);
            }

            let mesh = cut_cap_mesh(&grid, IVec3::ZERO, 1, cut);

            let triangles = cap_triangle_materials(&mesh);
            assert_eq!(triangles.len(), 4, "{why}: a 2x2 square");
            for (centroid, materials) in triangles {
                assert_eq!(materials, [expected; 3], "{why}: triangle at {centroid:?}");
            }
        }
    }

    #[test]
    fn coarse_cap_above_the_origin_picks_the_layer_the_probe_is_in() {
        let chunk = IVec3::new(0, 1, 0);
        let mut grid = VoxelGrid::new(CHUNK);
        // Coarse cell row 1 of the chunk: lower layer y 18 is soil, upper layer y 19 stone.
        grid.place_block(IVec3::new(0, 18, 0), 1, None);
        grid.place_block(IVec3::new(0, 19, 0), 2, None);

        for (cut, expected) in [(18.5f32, 1), (19.5, 2)] {
            let mesh = cut_cap_mesh(&grid, chunk, 1, cut);

            let triangles = cap_triangle_materials(&mesh);
            assert_eq!(triangles.len(), 4, "cut {cut}: a 2x2 square");
            for (centroid, materials) in triangles {
                assert_eq!(
                    materials, [expected; 3],
                    "cut {cut}: triangle at {centroid:?}"
                );
            }
        }
    }

    /// Material id of corner `k` in `cell_grid` is this plus `k`.
    const CORNER_MATERIAL_BASE: u32 = 10;
    const CELL_ORIGIN: IVec3 = IVec3::new(6, 6, 6);

    /// One marching-cubes cell whose solid corners (the bits of `cube_index`) are blocks
    /// of distinct materials, so a triangle's material names the corner it took.
    fn cell_grid(cube_index: u8) -> VoxelGrid {
        let mut grid = VoxelGrid::new(CHUNK);
        for (k, offset) in MarchingCubes::CORNER_OFFSETS.iter().enumerate() {
            if cube_index & (1 << k) != 0 {
                let pos = CELL_ORIGIN + IVec3::from(offset.map(|c| c as i32));
                grid.place_block(pos, CORNER_MATERIAL_BASE + k as u32, None);
            }
        }
        grid
    }

    /// Triangles whose centroid lies in the cell's square footprint, as
    /// `(centroid in cell coordinates, materials)`; `y` is `height` for a cap.
    fn triangles_in_cell(mesh: &VoxelMesh, y: Option<f32>) -> Vec<(Vec3, [u32; 3])> {
        let origin = Vec3::new(CELL_ORIGIN.x as f32 + 0.5, 0.0, CELL_ORIGIN.z as f32 + 0.5);
        cap_triangle_materials(mesh)
            .into_iter()
            .map(|(c, materials)| {
                let local = c - Vec3::new(origin.x, CELL_ORIGIN.y as f32 + 0.5, origin.z);
                (Vec3::new(local.x, y.unwrap_or(local.y), local.z), materials)
            })
            .filter(|(c, _)| {
                (0.0..=1.0).contains(&c.x)
                    && (0.0..=1.0).contains(&c.z)
                    && (0.0..=1.0).contains(&c.y)
            })
            .collect()
    }

    /// The cap triangles (at `sixty_fourths` of the cell's height) and wall triangles of
    /// a one-cell grid that don't take their nearest solid corner's material, and how
    /// many cap triangles were checked.
    fn nearest_block_mismatches(cube_index: u8, sixty_fourths: u32) -> (Vec<String>, usize) {
        let height = sixty_fourths as f32 / 64.0;
        let grid = cell_grid(cube_index);
        let cut = CELL_ORIGIN.y as f32 + 0.5 + height - CAP_PROBE;

        let in_cap = triangles_in_cell(&cap(&grid, cut), Some(height));
        let wall_mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, IVec3::ZERO, CHUNK);
        let in_wall = triangles_in_cell(&wall_mesh, None);
        let cap_count = in_cap.len();
        let mismatches = in_cap
            .into_iter()
            .map(|t| ("cap", t))
            .chain(in_wall.into_iter().map(|t| ("wall", t)))
            .filter_map(|(kind, (centroid, materials))| {
                let corner = nearest_solid_corner(cube_index, centroid);
                let expected = [CORNER_MATERIAL_BASE + corner as u32; 3];
                (materials != expected).then(|| {
                    format!("{kind} triangle at {centroid:?}: {materials:?}, expected {expected:?}")
                })
            })
            .collect();
        (mismatches, cap_count)
    }

    #[test]
    fn cap_triangles_on_a_corner_distance_tie_take_the_lowest_corner() {
        // Each cut puts a cap triangle's centroid equally near two solid corners.
        for (cube_index, sixty_fourths) in [(190, 23), (235, 41), (239, 41), (254, 23)] {
            let (mismatches, cap_count) = nearest_block_mismatches(cube_index, sixty_fourths);

            assert_ne!(cap_count, 0, "cube index {cube_index} has a cap");
            assert!(
                mismatches.is_empty(),
                "cube index {cube_index}, {sixty_fourths}/64: {mismatches:#?}"
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn cap_and_wall_triangles_use_the_same_nearest_block_rule(
            cube_index in 1..=255u8,
            sixty_fourths in (1..=63u32).prop_filter("the middle height is lifted", |n| *n != 32),
        ) {
            let (mismatches, cap_count) = nearest_block_mismatches(cube_index, sixty_fourths);

            prop_assume!(cap_count != 0);
            prop_assert!(
                mismatches.is_empty(),
                "cube index {}, {}/64: {:#?}", cube_index, sixty_fourths, mismatches
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        #[test]
        fn cap_edge_lies_on_the_wall(
            (field, cut) in fields_on(inside_blocks(1, 14))
                .prop_flat_map(|f| { let cut = heights_through(&f); (Just(f), cut) }),
        ) {
            let grid = field.grid(SMOOTH);
            let h = f64::from(cut) + f64::from(CAP_PROBE);
            let chunk_mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, IVec3::ZERO, CHUNK);
            let walls = slice(&chunk_mesh, h);
            let expected = enclosed_area(&walls);

            let mesh = cap(&grid, cut);

            if expected > 0.01 {
                prop_assert!(!mesh.indices.is_empty(), "the cut crosses solid but nothing is capped");
            }
            prop_assert!(
                (cap_area(&mesh) - expected).abs() < AREA_TOLERANCE,
                "cap area {} is not the solid area {} at cut {}",
                cap_area(&mesh), expected, cut
            );
            for t in triangles(&mesh) {
                let area = cross(sub(t[1], t[0]), sub(t[2], t[0]))[1].abs() / 2.0;
                let (x, z) = ((t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0);
                prop_assert!(area < 1e-9 || inside(&walls, x, z), "cap triangle at ({}, {}) is on the air side at cut {}", x, z, cut);
            }
            for (from, to) in boundary_edges(&mesh) {
                for s in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let p = [from[0] + s * (to[0] - from[0]), from[1] + s * (to[1] - from[1])];
                    let on_wall = walls.iter().any(|w| distance_to_segment(p, w.a, w.b) < WALL_TOLERANCE);
                    prop_assert!(on_wall, "cap edge point {:?} at cut {} is off the wall", p, cut);
                }
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]

        #[test]
        fn only_chunks_in_the_cap_layers_are_capped(
            (field, cut) in fields_on(inside_blocks(1, 30))
                .prop_flat_map(|f| { let cut = heights_through(&f); (Just(f), cut) }),
            blocky in any::<bool>(),
            lod in 0u8..2,
        ) {
            let grid = field.grid(if blocky { BLOCKY } else { SMOOTH });
            let layers = cap_chunk_layers(cut, CHUNK);

            prop_assert!(layers.end() - layers.start() <= 1, "{:?} spans over two layers", layers);
            for (x, y, z) in (0..2).flat_map(|x| (-1..3).flat_map(move |y| (0..2).map(move |z| (x, y, z)))) {
                let mesh = cut_cap_mesh(&grid, IVec3::new(x, y, z), lod, cut);
                prop_assert!(
                    mesh.indices.is_empty() || layers.contains(&y),
                    "chunk layer {} is capped at {} outside {:?}", y, cut, layers
                );
            }
        }
    }

    /// A field that straddles the chunk borders at 16, a material, and a cut: on a
    /// border, half a block either side of it, or anywhere through the field.
    fn tiling_cases() -> impl Strategy<Value = (Field, bool, f32)> {
        (fields_on(straddling_blocks()), any::<bool>()).prop_flat_map(|(field, blocky)| {
            let cut = prop_oneof![
                prop::sample::select(vec![16.0f32, 15.5, 16.5]),
                heights_through(&field),
            ];
            (Just(field), Just(blocky), cut)
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]

        #[test]
        fn caps_tile_across_chunk_borders((field, blocky, cut) in tiling_cases()) {
            let grid = field.grid(if blocky { BLOCKY } else { STONE });
            let h = f64::from(cut) + f64::from(CAP_PROBE);
            // The mesher draws nothing for an Empty chunk even when its padding holds
            // solid, a crack at -x/-y/-z faces that caps do not repair.
            if !blocky {
                let owner_layer = ((h - 0.5) / 16.0).floor() as i32;
                for (x, z) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                    let pos = IVec3::new(x, owner_layer, z);
                    let (content, _, _) = HybridMeshGenerator::analyze_chunk(&grid, pos, CHUNK);
                    let padding_solid = (0..18).any(|i| (0..18).any(|j| (0..18).any(|k| {
                        grid.is_solid_at(pos * CHUNK + IVec3::new(i, j, k) - IVec3::ONE)
                    })));
                    prop_assume!(content != ChunkContent::Empty || !padding_solid);
                }
            }
            let expected = if blocky {
                let layer = h.floor() as i32;
                (0..32).flat_map(|x| (0..32).map(move |z| (x, z)))
                    .filter(|&(x, z)| field.solid([x, layer, z]))
                    .count() as f64
            } else {
                let mut walls = Vec::new();
                for (x, y, z) in (0..2).flat_map(|x| (0..2).flat_map(move |y| (0..2).map(move |z| (x, y, z)))) {
                    let mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, IVec3::new(x, y, z), CHUNK);
                    walls.extend(slice(&mesh, h));
                }
                enclosed_area(&walls)
            };

            let mut tris = Vec::new();
            let mut total = 0.0;
            let ground_material = if blocky { BLOCKY } else { STONE };
            for (x, y, z) in (0..2).flat_map(|x| (-1..3).flat_map(move |y| (0..2).map(move |z| (x, y, z)))) {
                let mesh = cut_cap_mesh(&grid, IVec3::new(x, y, z), 0, cut);
                total += cap_area(&mesh);
                tris.extend(triangles(&mesh));
                prop_assert_eq!(mesh.material.len(), mesh.vertices.len(), "one material per vertex");
                for (k, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
                    let materials = tri.map(|i| mesh.material[i as usize]);
                    prop_assert_eq!(
                        materials, [ground_material; 3],
                        "cap triangle {} of chunk ({}, {}, {}) is one material, the ground's",
                        k, x, y, z
                    );
                }
            }

            prop_assert!(
                (total - expected).abs() < AREA_TOLERANCE,
                "summed cap area {} differs from the solid cross-section {} at cut {} (blocky: {})",
                total, expected, cut, blocky
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
