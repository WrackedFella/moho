use std::cell::Cell;

use flagstone_game::raycast;
use flagstone_voxel::{VoxelChunk, VoxelGrid};
use glam::{DVec3, IVec3, Vec3};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

/// Slack for "the same point on the mesh" and for crossings the f32 raycast and the
/// f64 reference may resolve differently at an edge.
const TOLERANCE: f64 = 1e-3;

/// Below this |cos| between the ray and a triangle's plane, which side the ray crosses
/// from is not numerically meaningful.
const GRAZING_COS: f64 = 1e-3;

fn config() -> Config {
    Config {
        cases: 256,
        failure_persistence: None,
        ..Config::default()
    }
}

#[derive(Debug, Clone)]
struct Case {
    blocks: Vec<IVec3>,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
}

impl Case {
    fn grid(&self) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for &block in &self.blocks {
            grid.mutator().place(block, 1, None);
        }
        grid
    }

    fn unit_direction(&self) -> DVec3 {
        self.direction.as_dvec3().normalize()
    }
}

/// Smooth blocks in `2..14`^3 (the whole surface falls inside chunk 0's mesh), an origin
/// anywhere in `-6..22`^3, and a direction aimed at a solid block's centre with jitter with
/// probability `aim`, otherwise random.
fn cases(max_distance: std::ops::Range<f32>, aim: f64) -> impl Strategy<Value = Case> {
    let block = (2..14i32, 2..14i32, 2..14i32).prop_map(|(x, y, z)| IVec3::new(x, y, z));
    let point = |range: std::ops::Range<f32>| {
        (range.clone(), range.clone(), range).prop_map(|(x, y, z)| Vec3::new(x, y, z))
    };
    (
        prop::collection::vec(block, 1..=200),
        point(-6.0..22.0),
        any::<prop::sample::Index>(),
        point(-0.6..0.6),
        point(-1.0..1.0),
        prop::bool::weighted(aim),
        max_distance,
    )
        .prop_filter_map(
            "direction must be non-zero",
            |(blocks, origin, pick, jitter, random, aimed, max_distance)| {
                let target = blocks[pick.index(blocks.len())].as_vec3() + Vec3::splat(0.5) + jitter;
                let direction = if aimed { target - origin } else { random };
                (direction.length() > 1e-2).then_some(Case {
                    blocks,
                    origin,
                    direction,
                    max_distance,
                })
            },
        )
}

type Triangle = [DVec3; 3];

/// The exact triangles of chunk 0's mesh, in world space.
fn mesh_triangles(grid: &VoxelGrid) -> Vec<Triangle> {
    let chunk = VoxelChunk::from_grid_lod(grid, IVec3::ZERO, 0);
    let verts = chunk.vertices();
    let vertex = |i: u32| {
        let v = verts[i as usize];
        DVec3::new(v[0].into(), v[1].into(), v[2].into())
    };
    let (triples, _) = chunk.indices().as_chunks::<3>();
    triples
        .iter()
        .map(|t| [vertex(t[0]), vertex(t[1]), vertex(t[2])])
        .collect()
}

/// Closest point on a triangle to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_point_on_triangle(p: DVec3, [a, b, c]: Triangle) -> DVec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

fn distance_to_mesh(tris: &[Triangle], p: DVec3) -> f64 {
    tris.iter()
        .map(|&t| (closest_point_on_triangle(p, t) - p).length())
        .fold(f64::INFINITY, f64::min)
}

/// A segment's crossing of a mesh triangle.
#[derive(Debug, Clone, Copy)]
struct Crossing {
    /// Distance along the segment.
    t: f64,
    /// The ray runs against the triangle's outward normal (the CCW winding faces outward).
    front: bool,
    /// |cos| of the angle between the ray and the triangle's plane normal.
    cos: f64,
}

/// Möller–Trumbore segment test, double-sided; edges count as inside.
fn segment_triangle(
    origin: DVec3,
    dir: DVec3,
    length: f64,
    [v0, v1, v2]: Triangle,
) -> Option<Crossing> {
    const EDGE_SLACK: f64 = 1e-9;
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let pvec = dir.cross(e2);
    let det = e1.dot(pvec);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = origin - v0;
    let bary_u = tvec.dot(pvec) * inv;
    if !(-EDGE_SLACK..=1.0 + EDGE_SLACK).contains(&bary_u) {
        return None;
    }
    let qvec = tvec.cross(e1);
    let bary_v = dir.dot(qvec) * inv;
    if bary_v < -EDGE_SLACK || bary_u + bary_v > 1.0 + EDGE_SLACK {
        return None;
    }
    let along = e2.dot(qvec) * inv;
    if !(0.0..=length).contains(&along) {
        return None;
    }
    let normal = e1.cross(e2).normalize();
    Some(Crossing {
        t: along,
        front: normal.dot(dir) < 0.0,
        cos: normal.dot(dir).abs(),
    })
}

/// Every triangle crossing of the segment, nearest first.
fn crossings(tris: &[Triangle], origin: DVec3, dir: DVec3, length: f64) -> Vec<Crossing> {
    let mut all: Vec<Crossing> = tris
        .iter()
        .filter_map(|&t| segment_triangle(origin, dir, length, t))
        .collect();
    all.sort_by(|a, b| a.t.total_cmp(&b.t));
    all
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn surface_hit_lies_on_the_chunk_mesh(case in cases(20.0..60.0, 0.9)) {
        let grid = case.grid();
        let tris = mesh_triangles(&grid);

        let hit = raycast::raycast_surface(&grid, case.origin, case.direction, case.max_distance);

        if let Some(hit) = hit {
            let off_mesh = distance_to_mesh(&tris, hit.position.as_dvec3());
            prop_assert!(off_mesh <= TOLERANCE, "hit {:?} is {off_mesh} from the mesh", hit.position);
            let along_ray = case.origin.as_dvec3() + case.unit_direction() * f64::from(hit.distance);
            prop_assert!(
                (along_ray - hit.position.as_dvec3()).length() <= TOLERANCE,
                "position {:?} is not origin + dir * distance ({along_ray:?})",
                hit.position
            );
        }
    }

    #[test]
    fn surface_hit_is_first_front_face_and_owned_by_a_bounding_solid_block(
        case in cases(20.0..60.0, 0.9)
    ) {
        let grid = case.grid();
        let tris = mesh_triangles(&grid);
        let origin = case.origin.as_dvec3();
        let dir = case.unit_direction();
        let max = f64::from(case.max_distance);

        let hit = raycast::raycast_surface(&grid, case.origin, case.direction, case.max_distance);
        let crossed = crossings(&tris, origin, dir, max + TOLERANCE);

        if let Some(hit) = hit {
            let distance = f64::from(hit.distance);
            let earlier = crossed
                .iter()
                .find(|c| c.front && c.t < distance - TOLERANCE);
            prop_assert!(
                earlier.is_none(),
                "front face crossed at {earlier:?} before the hit at {distance}"
            );
            prop_assert!(grid.material_at(hit.block_pos).is_some(), "{:?} is not solid", hit.block_pos);
            let centre = hit.block_pos.as_dvec3() + DVec3::splat(0.5);
            let offset = (centre - hit.position.as_dvec3()).abs().max_element();
            prop_assert!(
                offset <= 1.0 + TOLERANCE,
                "block {:?} does not bound the hit at {:?}",
                hit.block_pos,
                hit.position
            );
        }

        if let Some(first) = crossed.first() {
            let tied = crossed.iter().take_while(|c| c.t - first.t < TOLERANCE);
            let ambiguous = first.t < TOLERANCE
                || first.t > max - TOLERANCE
                || tied.clone().any(|c| c.front != first.front || c.cos < GRAZING_COS);
            if !ambiguous && first.front {
                let hit = hit.ok_or_else(|| TestCaseError::fail(format!(
                    "missed a front face at {} in range {max}", first.t
                )))?;
                prop_assert!(
                    (f64::from(hit.distance) - first.t).abs() <= TOLERANCE,
                    "hit at {} but the first front face is at {}",
                    hit.distance,
                    first.t
                );
            } else if !ambiguous {
                prop_assert!(
                    hit.is_none(),
                    "origin is inside the surface (first crossing {first:?}) yet hit {hit:?}"
                );
            }
        }
    }

    #[test]
    fn cube_raycast_reports_first_solid_block_with_face_toward_origin(
        case in cases(20.0..60.0, 0.9)
    ) {
        let grid = case.grid();
        let origin = case.origin.as_dvec3();
        let dir = case.unit_direction();
        let max = f64::from(case.max_distance);
        let origin_block = case.origin.floor().as_ivec3();
        prop_assume!(grid.material_at(origin_block).is_none());

        let hit = raycast::raycast(&grid, case.origin, case.direction, case.max_distance);

        // Slab test against every solid block: (entry t, entry axis, entry t per axis).
        let mut entries: Vec<(f64, usize, [f64; 3], IVec3)> = case
            .blocks
            .iter()
            .filter_map(|&block| {
                let lo = block.as_dvec3();
                let mut near = [f64::NEG_INFINITY; 3];
                let mut far = [f64::INFINITY; 3];
                for axis in 0..3 {
                    let t0 = (lo[axis] - origin[axis]) / dir[axis];
                    let t1 = (lo[axis] + 1.0 - origin[axis]) / dir[axis];
                    near[axis] = t0.min(t1);
                    far[axis] = t0.max(t1);
                }
                let enter = near[0].max(near[1]).max(near[2]);
                let exit = far[0].min(far[1]).min(far[2]);
                let axis = (0..3).max_by(|&a, &b| near[a].total_cmp(&near[b])).unwrap();
                (enter <= exit && enter >= 0.0).then_some((enter, axis, near, block))
            })
            .collect();
        entries.sort_by(|a, b| a.0.total_cmp(&b.0));
        entries.dedup_by_key(|e| e.3);
        let first = entries.first().copied();

        let ambiguous = first.is_some_and(|(t, axis, near, _)| {
            let tied_axis = (0..3).any(|a| a != axis && (near[a] - t).abs() < 1e-6);
            let tied_block = entries.get(1).is_some_and(|e| e.0 - t < 1e-6);
            tied_axis || tied_block || (t - max).abs() < 1e-4
        });
        prop_assume!(!ambiguous);

        match first.filter(|e| e.0 <= max) {
            None => prop_assert!(hit.is_none(), "reported {hit:?} but no block lies in range"),
            Some((t, axis, _, block)) => {
                let hit = hit.ok_or_else(|| TestCaseError::fail(format!(
                    "missed block {block:?} entered at {t}"
                )))?;
                prop_assert_eq!(hit.block_pos, block);
                prop_assert!((f64::from(hit.distance) - t).abs() <= 1e-4, "distance {} vs {t}", hit.distance);
                let mut expected = IVec3::ZERO;
                expected[axis] = if dir[axis] > 0.0 { -1 } else { 1 };
                prop_assert_eq!(hit.normal, expected);
                prop_assert!(hit.normal.as_vec3().dot(case.direction) < 0.0);
                let along_ray = origin + dir * f64::from(hit.distance);
                prop_assert!((along_ray - hit.position.as_dvec3()).length() <= TOLERANCE);
            }
        }
    }
}

#[test]
fn ray_through_empty_space_reports_no_hit() {
    let strategy = prop_oneof![cases(1.0..12.0, 0.9), cases(1.0..60.0, 0.2)];
    let empty_cases = Cell::new(0u32);
    let mut runner = TestRunner::new(config());

    runner
        .run(&strategy, |case| {
            let grid = case.grid();
            let tris = mesh_triangles(&grid);
            let length = f64::from(case.max_distance) + TOLERANCE;
            let touching = crossings(&tris, case.origin.as_dvec3(), case.unit_direction(), length);
            prop_assume!(touching.is_empty());
            empty_cases.set(empty_cases.get() + 1);

            let hit =
                raycast::raycast_surface(&grid, case.origin, case.direction, case.max_distance);

            prop_assert!(hit.is_none(), "empty segment reported {hit:?}");
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));

    assert!(
        empty_cases.get() >= 100,
        "only {} generated cases had an empty segment; the strategy is not exercising misses",
        empty_cases.get()
    );
}

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

/// The block a hit reports is the one forming the surface: with it gone, the same ray
/// must miss or land strictly farther along.
#[test]
fn removing_reported_block_moves_hit_farther_on_ramp() {
    let mut grid = ramp_grid();
    let eye = Vec3::new(-3.0, 10.5, 8.0);
    let mut tested = 0usize;

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

            grid.mutator().remove(hit.block_pos);
            let after = raycast::raycast_surface(&grid, eye, dir, 40.0);
            grid.mutator().place(hit.block_pos, 1, None);

            if let Some(next) = after {
                assert!(
                    next.distance > hit.distance + 0.01,
                    "removing {:?} left a hit at {} (was {}) for yaw {yaw}, pitch {pitch}",
                    hit.block_pos,
                    next.distance,
                    hit.distance
                );
            }
        }
    }

    assert!(
        tested > 100,
        "expected a decent number of surface hits, got {tested}"
    );
}
