use std::cell::Cell;

use flagstone_game::raycast;
use flagstone_voxel::{VoxelChunk, VoxelGrid};
use glam::{DVec3, IVec3, Vec3};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

/// Slack for "the same point on the mesh" and for crossings the f32 raycast and the
/// f64 reference may resolve differently at an edge.
const TOLERANCE: f64 = 1e-3;

/// Below this |cos| between the ray and a triangle's plane, which side the ray crosses
/// from is not numerically meaningful.
const GRAZING_COS: f64 = 1e-3;

/// Floors for the seed's branch counts, so each property's branch is known to have run.
const MIN_HITS: u32 = 40;
const MIN_CUBE: u32 = 100;
const MIN_FRONT: u32 = 40;
const MIN_INSIDE: u32 = 60;
const MIN_ORIGIN_CELL: u32 = 6;
const MIN_NONE: u32 = 40;
const MIN_EMPTY_BEYOND: u32 = 40;

fn config() -> Config {
    Config {
        cases: 256,
        failure_persistence: None,
        ..Config::default()
    }
}

/// Where a generated ray starts.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Start {
    /// Anywhere in `-6..22`^3.
    Anywhere,
    /// Inside a solid block's octahedron (L1 distance below 0.5 from its centre), so inside
    /// the surface whatever the neighbours are.
    Inside,
    /// A little short of the aimed-at point, so the first crossing is often in the origin's
    /// own dual-lattice cell.
    NearTarget,
}

#[derive(Debug, Clone)]
struct Case {
    blocks: Vec<IVec3>,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    start: Start,
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

/// Smooth blocks in `2..14`^3 (the whole surface falls inside chunk 0's mesh), a start per
/// `start`, and a direction aimed at a solid block's centre with jitter with probability
/// `aim`, otherwise random.
fn cases(
    max_distance: std::ops::Range<f32>,
    aim: f64,
    start: Start,
) -> impl Strategy<Value = Case> {
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
        (any::<prop::sample::Index>(), point(-0.5..0.5), 0.05..1.5f32),
    )
        .prop_filter_map(
            "direction must be non-zero",
            move |(
                blocks,
                origin,
                pick,
                jitter,
                random,
                aimed,
                max_distance,
                (start_pick, inner, back),
            )| {
                let target = blocks[pick.index(blocks.len())].as_vec3() + Vec3::splat(0.5) + jitter;
                let (origin, direction) = match start {
                    Start::Anywhere => (origin, if aimed { target - origin } else { random }),
                    Start::Inside => {
                        let l1 = inner.abs().element_sum();
                        let inner = if l1 > 0.45 {
                            inner * (0.45 / l1)
                        } else if l1 < 0.02 {
                            inner + Vec3::new(0.05, 0.0, 0.0)
                        } else {
                            inner
                        };
                        let centre = blocks[start_pick.index(blocks.len())].as_vec3() + 0.5;
                        let origin = centre + inner;
                        (origin, if aimed { target - origin } else { random })
                    }
                    Start::NearTarget => {
                        let to_target = target - origin;
                        if to_target.length() > 1e-2 {
                            (target - to_target.normalize() * back, to_target)
                        } else {
                            (origin, random)
                        }
                    }
                };
                (direction.length() > 1e-2).then_some(Case {
                    blocks,
                    origin,
                    direction,
                    max_distance,
                    start,
                })
            },
        )
}

/// Same seed every run, so the branch counts asserted after a run are stable.
fn deterministic_runner() -> TestRunner {
    TestRunner::new_with_rng(config(), TestRng::deterministic_rng(RngAlgorithm::ChaCha))
}

type Triangle = [DVec3; 3];

/// The exact triangles of chunk 0's mesh, in world space.
fn mesh_triangles(grid: &VoxelGrid) -> Vec<Triangle> {
    chunk_triangles(grid, IVec3::ZERO)
}

/// The exact triangles of the mesh of chunk `coord`, in world space.
fn chunk_triangles(grid: &VoxelGrid, coord: IVec3) -> Vec<Triangle> {
    let chunk = VoxelChunk::from_grid_lod(grid, coord, 0);
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

#[test]
fn surface_hit_lies_on_the_chunk_mesh() {
    let strategy = cases(20.0..60.0, 0.9, Start::Anywhere);
    let hits = Cell::new(0u32);
    let mut runner = deterministic_runner();

    runner
        .run(&strategy, |case| {
            let grid = case.grid();
            let tris = mesh_triangles(&grid);

            let hit =
                raycast::raycast_surface(&grid, case.origin, case.direction, case.max_distance);

            if let Some(hit) = hit {
                bump(&hits);
                let off_mesh = distance_to_mesh(&tris, hit.position.as_dvec3());
                prop_assert!(
                    off_mesh <= TOLERANCE,
                    "hit {:?} is {off_mesh} from the mesh",
                    hit.position
                );
                let along_ray =
                    case.origin.as_dvec3() + case.unit_direction() * f64::from(hit.distance);
                prop_assert!(
                    (along_ray - hit.position.as_dvec3()).length() <= TOLERANCE,
                    "position {:?} is not origin + dir * distance ({along_ray:?})",
                    hit.position
                );
            }
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));

    assert!(hits.get() >= MIN_HITS, "only {} hits", hits.get());
}

#[test]
fn cube_raycast_reports_first_solid_block_with_face_toward_origin() {
    let strategy = cases(20.0..60.0, 0.9, Start::Anywhere);
    let checked = Cell::new(0u32);
    let mut runner = deterministic_runner();

    runner
        .run(&strategy, |case| {
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
                    let hit = hit.ok_or_else(|| {
                        TestCaseError::fail(format!("missed block {block:?} entered at {t}"))
                    })?;
                    prop_assert_eq!(hit.block_pos, block);
                    prop_assert!(
                        (f64::from(hit.distance) - t).abs() <= 1e-4,
                        "distance {} vs {t}",
                        hit.distance
                    );
                    let mut expected = IVec3::ZERO;
                    expected[axis] = if dir[axis] > 0.0 { -1 } else { 1 };
                    prop_assert_eq!(hit.normal, expected);
                    prop_assert!(hit.normal.as_vec3().dot(case.direction) < 0.0);
                    let along_ray = origin + dir * f64::from(hit.distance);
                    prop_assert!((along_ray - hit.position.as_dvec3()).length() <= TOLERANCE);
                }
            }
            bump(&checked);
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));

    assert!(checked.get() >= MIN_CUBE, "only {} checked", checked.get());
}

/// How often each branch of the first-front-face property ran.
#[derive(Default)]
struct Branches {
    front_face_hit_matched: Cell<u32>,
    inside_start_reported_none: Cell<u32>,
    first_crossing_in_origin_cell: Cell<u32>,
    no_crossing: Cell<u32>,
}

fn bump(counter: &Cell<u32>) {
    counter.set(counter.get() + 1);
}

/// The dual-lattice cell (block-centre corners) containing `p`.
fn dual_cell(p: DVec3) -> IVec3 {
    (p - DVec3::splat(0.5)).floor().as_ivec3()
}

fn check_first_front_face(case: &Case, branches: &Branches) -> Result<(), TestCaseError> {
    let grid = case.grid();
    let tris = mesh_triangles(&grid);
    let origin = case.origin.as_dvec3();
    let dir = case.unit_direction();
    let max = f64::from(case.max_distance);

    let hit = raycast::raycast_surface(&grid, case.origin, case.direction, case.max_distance);
    let crossed = crossings(&tris, origin, dir, max + TOLERANCE);

    if case.start == Start::Inside {
        prop_assert!(
            hit.is_none(),
            "origin {:?} is inside the surface yet hit {hit:?}",
            case.origin
        );
        bump(&branches.inside_start_reported_none);
    }

    if let Some(hit) = hit {
        let distance = f64::from(hit.distance);
        let earlier = crossed
            .iter()
            .find(|c| c.front && c.t < distance - TOLERANCE);
        prop_assert!(
            earlier.is_none(),
            "front face crossed at {earlier:?} before the hit at {distance}"
        );
        prop_assert!(
            grid.material_at(hit.block_pos).is_some(),
            "{:?} is not solid",
            hit.block_pos
        );
        let centre = hit.block_pos.as_dvec3() + DVec3::splat(0.5);
        let at = hit.position.as_dvec3();
        let offset = (centre - at).abs().max_element();
        prop_assert!(
            offset <= 1.0 + TOLERANCE,
            "block {:?} does not bound the hit at {:?}",
            hit.block_pos,
            hit.position
        );
        // Of the cell's eight corner blocks, the solid one nearest the hit owns it.
        let reported = (centre - at).length();
        let base = dual_cell(at);
        for corner in 0..8 {
            let block = base + IVec3::new(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
            if grid.material_at(block).is_none() {
                continue;
            }
            let rival = (block.as_dvec3() + DVec3::splat(0.5) - at).length();
            prop_assert!(
                rival >= reported - TOLERANCE,
                "solid block {block:?} ({rival}) is nearer the hit than reported {:?} ({reported})",
                hit.block_pos
            );
        }
    }

    let Some(first) = crossed.first() else {
        bump(&branches.no_crossing);
        return Ok(());
    };
    let tied = crossed.iter().take_while(|c| c.t - first.t < TOLERANCE);
    let ambiguous = first.t < TOLERANCE
        || first.t > max - TOLERANCE
        || tied
            .clone()
            .any(|c| c.front != first.front || c.cos < GRAZING_COS);
    if ambiguous {
        return Ok(());
    }
    if first.front {
        let hit = hit.ok_or_else(|| {
            TestCaseError::fail(format!("missed a front face at {} in range {max}", first.t))
        })?;
        prop_assert!(
            (f64::from(hit.distance) - first.t).abs() <= TOLERANCE,
            "hit at {} but the first front face is at {}",
            hit.distance,
            first.t
        );
        bump(&branches.front_face_hit_matched);
        if dual_cell(origin + dir * first.t) == dual_cell(origin) {
            bump(&branches.first_crossing_in_origin_cell);
        }
    } else {
        prop_assert!(
            hit.is_none(),
            "origin is inside the surface (first crossing {first:?}) yet hit {hit:?}"
        );
    }
    Ok(())
}

#[test]
fn surface_hit_is_first_front_face_and_owned_by_a_bounding_solid_block() {
    let strategy = prop_oneof![
        cases(20.0..60.0, 0.9, Start::Anywhere),
        cases(20.0..60.0, 0.9, Start::Inside),
        cases(20.0..60.0, 0.9, Start::NearTarget),
    ];
    let branches = Branches::default();
    let mut runner = deterministic_runner();

    runner
        .run(&strategy, |case| check_first_front_face(&case, &branches))
        .unwrap_or_else(|e| panic!("{e}"));

    eprintln!(
        "branches: front_face_hit_matched={} inside_start_reported_none={} first_crossing_in_origin_cell={} no_crossing={}",
        branches.front_face_hit_matched.get(),
        branches.inside_start_reported_none.get(),
        branches.first_crossing_in_origin_cell.get(),
        branches.no_crossing.get(),
    );
    assert!(
        branches.front_face_hit_matched.get() >= MIN_FRONT,
        "front-face hits"
    );
    assert!(
        branches.inside_start_reported_none.get() >= MIN_INSIDE,
        "inside starts"
    );
    assert!(
        branches.first_crossing_in_origin_cell.get() >= MIN_ORIGIN_CELL,
        "origin-cell crossings"
    );
    assert!(
        branches.no_crossing.get() >= MIN_NONE,
        "rays with no crossing"
    );
}

#[test]
fn ray_through_empty_space_reports_no_hit() {
    let strategy = prop_oneof![
        cases(1.0..12.0, 0.9, Start::Anywhere),
        cases(1.0..60.0, 0.2, Start::Anywhere),
    ];
    let empty = Cell::new(0u32);
    let empty_but_mesh_just_beyond = Cell::new(0u32);
    let mut runner = deterministic_runner();

    runner
        .run(&strategy, |case| {
            let grid = case.grid();
            let tris = mesh_triangles(&grid);
            let origin = case.origin.as_dvec3();
            let dir = case.unit_direction();
            let length = f64::from(case.max_distance) + TOLERANCE;
            prop_assume!(crossings(&tris, origin, dir, length).is_empty());
            bump(&empty);
            // The surface lies on the ray, only past max_distance: a hit here is a broken range check.
            if !crossings(&tris, origin, dir, 200.0).is_empty() {
                bump(&empty_but_mesh_just_beyond);
            }

            let hit =
                raycast::raycast_surface(&grid, case.origin, case.direction, case.max_distance);

            prop_assert!(hit.is_none(), "empty segment reported {hit:?}");
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));

    eprintln!(
        "empty segments={} of which the mesh lies beyond max_distance={}",
        empty.get(),
        empty_but_mesh_just_beyond.get()
    );
    assert!(
        empty_but_mesh_just_beyond.get() >= MIN_EMPTY_BEYOND,
        "only {} empty segments had the surface just past max_distance",
        empty_but_mesh_just_beyond.get()
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

fn lone_block_grid(block: IVec3) -> VoxelGrid {
    let mut grid = VoxelGrid::new(16);
    grid.mutator().place(block, 1, None);
    grid
}

#[test]
fn surface_ray_hits_near_vertex_of_block_in_negative_chunk() {
    let grid = lone_block_grid(IVec3::splat(-5));

    let hit = raycast::raycast_surface(&grid, Vec3::new(-4.5, -4.5, 3.0), Vec3::NEG_Z, 20.0)
        .expect("ray should hit the block's surface");

    assert_eq!(hit.block_pos, IVec3::splat(-5));
    assert!(
        (hit.distance - 7.0).abs() <= 1e-3,
        "distance {}",
        hit.distance
    );
    assert!(
        (hit.position.z + 4.0).abs() <= 1e-3,
        "position {:?}",
        hit.position
    );
}

#[test]
fn surface_ray_hits_near_vertex_of_block_at_large_coordinates() {
    let grid = lone_block_grid(IVec3::new(1000, 40, -1000));

    let hit = raycast::raycast_surface(&grid, Vec3::new(1000.5, 40.5, -990.0), Vec3::NEG_Z, 20.0)
        .expect("ray should hit the block's surface");

    assert_eq!(hit.block_pos, IVec3::new(1000, 40, -1000));
    assert!(
        (hit.distance - 9.0).abs() <= 1e-3,
        "distance {}",
        hit.distance
    );
    assert!(
        (hit.position.z + 999.0).abs() <= 1e-3,
        "position {:?}",
        hit.position
    );
}

#[test]
fn surface_hit_lies_on_mesh_of_negative_chunk_when_scene_is_shifted_by_a_chunk() {
    let strategy = cases(20.0..60.0, 0.9, Start::Anywhere);
    let shift = IVec3::splat(-16);
    let hits = Cell::new(0u32);
    let mut runner = deterministic_runner();

    runner
        .run(&strategy, |case| {
            let blocks: Vec<IVec3> = case.blocks.iter().map(|&b| b + shift).collect();
            let origin = case.origin + shift.as_vec3();
            let shifted = Case {
                blocks,
                origin,
                ..case
            };
            let grid = shifted.grid();
            let tris = chunk_triangles(&grid, IVec3::splat(-1));

            let hit =
                raycast::raycast_surface(&grid, origin, shifted.direction, shifted.max_distance);

            if let Some(hit) = hit {
                bump(&hits);
                let off_mesh = distance_to_mesh(&tris, hit.position.as_dvec3());
                prop_assert!(
                    off_mesh <= TOLERANCE,
                    "hit {:?} is {off_mesh} from the mesh",
                    hit.position
                );
                prop_assert!(grid.material_at(hit.block_pos).is_some());
            }
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));

    assert!(hits.get() >= 40, "only {} hits", hits.get());
}

fn slab_grid() -> VoxelGrid {
    let mut grid = VoxelGrid::new(16);
    for x in 4..9 {
        for z in 4..9 {
            grid.mutator().place(IVec3::new(x, 0, z), 1, None);
        }
    }
    grid
}

#[test]
fn surface_ray_in_lattice_plane_above_flat_slab_reports_no_hit() {
    let grid = slab_grid();

    let hit = raycast::raycast_surface(&grid, Vec3::new(2.0, 1.5, 6.5), Vec3::X, 20.0);

    assert_eq!(hit, None);
}

#[test]
fn surface_ray_just_below_slab_top_hits_side_wall_where_the_mesh_does() {
    let grid = slab_grid();
    let tris = mesh_triangles(&grid);
    let origin = Vec3::new(2.0, 0.9, 6.5);
    let first_front = crossings(&tris, origin.as_dvec3(), DVec3::X, 20.0)
        .into_iter()
        .find(|c| c.front)
        .expect("mesh side wall should be crossed");

    let hit = raycast::raycast_surface(&grid, origin, Vec3::X, 20.0)
        .expect("ray should hit the slab's side wall");

    assert_eq!(hit.block_pos.x, 4);
    assert!(
        (f64::from(hit.distance) - first_front.t).abs() <= TOLERANCE,
        "distance {} vs mesh {}",
        hit.distance,
        first_front.t
    );
}

#[test]
fn surface_ray_with_zero_or_negative_max_distance_reports_no_hit() {
    let grid = lone_block_grid(IVec3::new(5, 5, 5));
    let origin = Vec3::new(5.5, 5.5, 3.0);

    let zero = raycast::raycast_surface(&grid, origin, Vec3::Z, 0.0);
    let negative = raycast::raycast_surface(&grid, origin, Vec3::Z, -1.0);

    assert_eq!(zero, None);
    assert_eq!(negative, None);
}

#[test]
fn surface_ray_from_dual_cell_corner_hits_block_vertex() {
    let grid = lone_block_grid(IVec3::new(6, 3, 3));

    let hit = raycast::raycast_surface(&grid, Vec3::new(3.5, 3.5, 3.5), Vec3::X, 20.0)
        .expect("ray should hit the block's surface");

    assert_eq!(hit.block_pos, IVec3::new(6, 3, 3));
    assert!(
        (hit.distance - 2.5).abs() <= 1e-3,
        "distance {}",
        hit.distance
    );
}

/// Three solid corners of one dual-lattice cell make triangles that are not coplanar, so a
/// ray can cross the plane of a triangle well outside it before reaching the real surface.
#[test]
fn surface_ray_ignores_triangle_planes_outside_the_triangle() {
    let mut grid = VoxelGrid::new(16);
    for block in [
        IVec3::new(5, 5, 5),
        IVec3::new(6, 5, 5),
        IVec3::new(5, 6, 5),
    ] {
        grid.mutator().place(block, 1, None);
    }
    let tris = mesh_triangles(&grid);
    let origin = Vec3::new(4.763, 5.757, 2.011);
    let direction = Vec3::new(0.283, 0.138, 1.0);
    let first_front = crossings(
        &tris,
        origin.as_dvec3(),
        direction.as_dvec3().normalize(),
        20.0,
    )
    .into_iter()
    .next()
    .filter(|c| c.front)
    .expect("the first mesh crossing is a front face");

    let hit = raycast::raycast_surface(&grid, origin, direction, 20.0)
        .expect("ray should hit the surface");

    assert!(
        (f64::from(hit.distance) - first_front.t).abs() <= TOLERANCE,
        "distance {} vs mesh {}",
        hit.distance,
        first_front.t
    );
    assert!(
        distance_to_mesh(&tris, hit.position.as_dvec3()) <= TOLERANCE,
        "hit {:?} is off the mesh",
        hit.position
    );
}

/// Where the ray crosses the plane of a triangle past its edge, the hit must still be the
/// true surface crossing, not that plane extension.
#[test]
fn surface_ray_ignores_triangle_plane_extensions_past_the_triangle_edge() {
    let mut grid = VoxelGrid::new(16);
    for block in [
        IVec3::new(5, 5, 5),
        IVec3::new(6, 5, 5),
        IVec3::new(6, 6, 5),
    ] {
        grid.mutator().place(block, 1, None);
    }
    let tris = mesh_triangles(&grid);
    let origin = Vec3::new(4.763, 4.757, 2.011);
    let direction = Vec3::new(0.283, 0.278, 1.0);
    let first_front = crossings(
        &tris,
        origin.as_dvec3(),
        direction.as_dvec3().normalize(),
        20.0,
    )
    .into_iter()
    .next()
    .filter(|c| c.front)
    .expect("the first mesh crossing is a front face");

    let hit = raycast::raycast_surface(&grid, origin, direction, 20.0)
        .expect("ray should hit the surface");

    assert!(
        (f64::from(hit.distance) - first_front.t).abs() <= TOLERANCE,
        "distance {} vs mesh {}",
        hit.distance,
        first_front.t
    );
    assert!(
        distance_to_mesh(&tris, hit.position.as_dvec3()) <= TOLERANCE,
        "hit {:?} is off the mesh",
        hit.position
    );
}

fn assert_hit_is_first_front_crossing(grid: &VoxelGrid, origin: Vec3, direction: Vec3) {
    let tris = mesh_triangles(grid);
    let first_front = crossings(
        &tris,
        origin.as_dvec3(),
        direction.as_dvec3().normalize(),
        20.0,
    )
    .into_iter()
    .next()
    .filter(|c| c.front)
    .expect("the first mesh crossing is a front face");

    let hit = raycast::raycast_surface(grid, origin, direction, 20.0)
        .unwrap_or_else(|| panic!("missed the front crossing at {}", first_front.t));

    assert!(
        (f64::from(hit.distance) - first_front.t).abs() <= TOLERANCE,
        "distance {} vs mesh {}",
        hit.distance,
        first_front.t
    );
    assert!(
        distance_to_mesh(&tris, hit.position.as_dvec3()) <= TOLERANCE,
        "hit {:?} is off the mesh",
        hit.position
    );
}

#[test]
fn surface_ray_crossing_on_a_cell_face_is_not_missed() {
    let mut grid = VoxelGrid::new(16);
    for block in [IVec3::new(2, 2, 3), IVec3::new(3, 2, 3)] {
        grid.mutator().place(block, 1, None);
    }
    let origin = Vec3::new(5.776_340_5, 3.508_795_7, 1.327_194_8);
    let direction = Vec3::new(-0.684_142_05, -0.210_835_49, 0.698_210_6);

    assert_hit_is_first_front_crossing(&grid, origin, direction);
}

#[test]
fn surface_ray_range_is_inclusive() {
    let grid = slab_grid();
    let origin = Vec3::new(6.5, 3.0, 6.5);

    let at_limit = raycast::raycast_surface(&grid, origin, Vec3::NEG_Y, 2.0);
    let just_short = raycast::raycast_surface(&grid, origin, Vec3::NEG_Y, 1.999);

    assert_eq!(at_limit.map(|h| h.distance), Some(2.0));
    assert_eq!(just_short, None);
}

#[test]
fn surface_ray_crossing_on_a_cell_face_from_the_exit_side_is_not_missed() {
    let mut grid = VoxelGrid::new(16);
    for block in [IVec3::new(2, 2, 3), IVec3::new(3, 2, 3)] {
        grid.mutator().place(block, 1, None);
    }
    let origin = Vec3::new(3.654_700_5, 3.154_700_5, 4.654_700_3);
    let direction = Vec3::new(-0.577_350_26, -0.577_350_26, -0.577_350_26);

    assert_hit_is_first_front_crossing(&grid, origin, direction);
}

#[test]
fn surface_ray_starting_just_above_the_surface_hits_it() {
    let grid = slab_grid();
    let origin = Vec3::new(6.5, 1.00005, 6.5);

    let hit = raycast::raycast_surface(&grid, origin, Vec3::NEG_Y, 20.0)
        .expect("the surface lies 5e-5 below the origin");

    assert!(
        hit.distance > 0.0 && hit.distance <= 1e-4,
        "{}",
        hit.distance
    );
}
