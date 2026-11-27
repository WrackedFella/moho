//! ENG-F42-01 spike (#1070): ways to ask a game's cost rule during a route query, compared
//! on leaks through partly seen polygons, cost with one and four threats, and determinism.
//! Test-only prototype; no production code calls it.
//!
//! Approaches (the rule answers "is this point seen", the engine never sees threats):
//! - `PolyCentroid`: Detour-style area flag per polygon, asked at its centroid.
//! - `PolyAll`: per polygon, ruled out when its centroid or any corner is seen.
//! - `Segment`: Detour `getCost(pa, pb)`-style, each A* step sampled along its segment, then
//!   the usual funnel.
//! - `Cells(g)`: the navmesh tessellated on a world grid of `g` metres, `PolyAll` per cell.
//! - `Table(g)`: `Cells(g)`, with the rule answered from a cell-to-cell visibility table
//!   (Killzone lines-of-fire style); threats snap to their nearest cell.

use std::collections::{BinaryHeap, HashMap};
use std::time::Instant;

use glam::Vec3;

use super::super::{NavMesh, Polygon};
use super::{Open, Snapped, funnel, priority, snap};

/// A* over portal midpoints as `super::route`, with each step's cost from `step`
/// (`None`: the step cannot be taken).
fn route_by(
    mesh: &NavMesh,
    start: &Snapped,
    goal: &Snapped,
    step: &mut dyn FnMut(usize, usize, Vec3, Vec3) -> Option<f32>,
) -> Option<Vec<(Vec3, Vec3)>> {
    let count = mesh.polygons.len();
    let mut cost = vec![f32::INFINITY; count];
    let mut position = vec![Vec3::ZERO; count];
    let mut parent: Vec<Option<(usize, (Vec3, Vec3))>> = vec![None; count];
    let mut closed = vec![false; count];
    let mut open = BinaryHeap::new();
    cost[start.polygon] = 0.0;
    position[start.polygon] = start.point;
    open.push(Open {
        cost: priority(0.0, start.point, goal.point),
        polygon: start.polygon,
    });
    while let Some(Open { polygon, .. }) = open.pop() {
        if closed[polygon] {
            continue;
        }
        closed[polygon] = true;
        if polygon == goal.polygon {
            let mut portals = Vec::new();
            let mut at = polygon;
            while let Some((before, portal)) = parent[at] {
                portals.push(portal);
                at = before;
            }
            portals.reverse();
            return Some(portals);
        }
        let current = &mesh.polygons[polygon];
        for (edge, neighbour) in current.neighbours.iter().enumerate() {
            let Some(next) = *neighbour else { continue };
            if closed[next] {
                continue;
            }
            let portal = current.portal(edge);
            let midpoint = (portal.0 + portal.1) * 0.5;
            let Some(length) = step(polygon, next, position[polygon], midpoint) else {
                continue;
            };
            let through = cost[polygon] + length;
            if through < cost[next] {
                cost[next] = through;
                position[next] = midpoint;
                parent[next] = Some((polygon, portal));
                open.push(Open {
                    cost: priority(through, midpoint, goal.point),
                    polygon: next,
                });
            }
        }
    }
    None
}

fn centroid(polygon: &Polygon) -> Vec3 {
    polygon.vertices.iter().copied().sum::<Vec3>() / polygon.vertices.len() as f32
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Approach {
    PolyCentroid,
    PolyAll,
    Segment { spacing: f32 },
    Cells { cell: f32 },
    Table { cell: f32 },
}

impl Approach {
    fn name(self) -> String {
        match self {
            Self::PolyCentroid => "per polygon, centroid".into(),
            Self::PolyAll => "per polygon, centroid+corners".into(),
            Self::Segment { spacing } => format!("per step, samples every {spacing} m"),
            Self::Cells { cell } => format!("cells {cell} m, centroid+corners"),
            Self::Table { cell } => format!("cells {cell} m, precomputed table"),
        }
    }
}

/// What a query asked the game and what it answered.
#[derive(Clone, Debug, Default, PartialEq)]
struct Outcome {
    path: Option<Vec<Vec3>>,
    /// Calls an area-shaped API would make (one per polygon the search reached).
    areas: usize,
    /// Points the rule judged.
    points: usize,
}

/// A rule that judges points: `true` when the point is seen (ruled out).
type PointRule<'a> = dyn FnMut(Vec3) -> bool + 'a;

/// A route from `from` to `to` that avoids every area `blocked` rules out, on `mesh`; the
/// verdict is asked at most once per polygon.
fn by_area(
    mesh: &NavMesh,
    from: Vec3,
    to: Vec3,
    blocked: &mut dyn FnMut(usize) -> bool,
) -> (Option<Vec<Vec3>>, usize) {
    let mut verdict: Vec<Option<bool>> = vec![None; mesh.polygons.len()];
    let mut asked = 0;
    let mut judge = |polygon: usize| -> bool {
        *verdict[polygon].get_or_insert_with(|| {
            asked += 1;
            blocked(polygon)
        })
    };
    let (Some(start), Some(goal)) = (snap(mesh, from), snap(mesh, to)) else {
        return (None, 0);
    };
    if judge(start.polygon) || judge(goal.polygon) {
        return (None, asked);
    }
    let portals = route_by(mesh, &start, &goal, &mut |_, next, a, b| {
        (!judge(next)).then(|| a.distance(b))
    });
    (
        portals.map(|portals| funnel(start.point, goal.point, &portals)),
        asked,
    )
}

/// Samples from `a` to `b`, both included, no more than `spacing` apart.
fn samples(a: Vec3, b: Vec3, spacing: f32) -> impl Iterator<Item = Vec3> {
    let steps = ((a.distance(b) / spacing).ceil() as usize).max(1);
    (0..=steps).map(move |i| a.lerp(b, i as f32 / steps as f32))
}

fn ask(
    coarse: &NavMesh,
    refined: Option<&NavMesh>,
    approach: Approach,
    from: Vec3,
    to: Vec3,
    rule: &mut PointRule<'_>,
    table: Option<&dyn Fn(usize) -> bool>,
) -> Outcome {
    let mut points = 0;
    let mut seen = |p: Vec3| {
        points += 1;
        rule(p)
    };
    let (path, areas) = match approach {
        Approach::PolyCentroid => by_area(coarse, from, to, &mut |i| {
            seen(centroid(&coarse.polygons[i]))
        }),
        Approach::PolyAll => by_area(coarse, from, to, &mut |i| {
            let polygon = &coarse.polygons[i];
            seen(centroid(polygon)) || polygon.vertices.iter().any(|&v| seen(v))
        }),
        Approach::Cells { .. } => {
            let mesh = refined.expect("cells need the refined mesh");
            by_area(mesh, from, to, &mut |i| {
                let polygon = &mesh.polygons[i];
                seen(centroid(polygon)) || polygon.vertices.iter().any(|&v| seen(v))
            })
        }
        Approach::Table { .. } => {
            let mesh = refined.expect("a table needs the refined mesh");
            let table = table.expect("a table");
            by_area(mesh, from, to, &mut |i| table(i))
        }
        Approach::Segment { spacing } => {
            let (Some(start), Some(goal)) = (snap(coarse, from), snap(coarse, to)) else {
                return Outcome::default();
            };
            let mut steps = 0;
            let portals = route_by(coarse, &start, &goal, &mut |_, _, a, b| {
                steps += 1;
                samples(a, b, spacing)
                    .all(|p| !seen(p))
                    .then(|| a.distance(b))
            });
            (
                portals.map(|portals| funnel(start.point, goal.point, &portals)),
                steps,
            )
        }
    };
    Outcome {
        path,
        areas,
        points,
    }
}

// ---------------------------------------------------------------------------------------
// Tessellation: every polygon cut by the world grid lines x = k·g and z = k·g. Cut points
// are computed from the edge's endpoints in a fixed order, so both polygons sharing an edge
// get bit-identical points and neighbours can be matched by exact vertex pairs.

fn cut(a: Vec3, b: Vec3, axis: usize, c: f32) -> Vec3 {
    let (lo, hi) = if a[axis] < b[axis] { (a, b) } else { (b, a) };
    let t = (c - lo[axis]) / (hi[axis] - lo[axis]);
    let mut p = lo + (hi - lo) * t;
    p[axis] = c;
    p
}

/// Vertices closer than this to a grid line count as on it, so no sliver cells are cut.
const ON_LINE: f32 = 1e-4;

fn split(piece: &[Vec3], axis: usize, c: f32) -> [Vec<Vec3>; 2] {
    let (mut below, mut above) = (Vec::new(), Vec::new());
    for i in 0..piece.len() {
        let (a, b) = (piece[i], piece[(i + 1) % piece.len()]);
        if a[axis] <= c + ON_LINE {
            below.push(a);
        }
        if a[axis] >= c - ON_LINE {
            above.push(a);
        }
        if (a[axis] < c - ON_LINE && b[axis] > c + ON_LINE)
            || (a[axis] > c + ON_LINE && b[axis] < c - ON_LINE)
        {
            let p = cut(a, b, axis, c);
            below.push(p);
            above.push(p);
        }
    }
    [below, above]
}

fn ground_area(vertices: &[Vec3]) -> f32 {
    let n = vertices.len();
    (0..n)
        .map(|i| {
            let (a, b) = (vertices[i], vertices[(i + 1) % n]);
            a.x * b.z - b.x * a.z
        })
        .sum::<f32>()
        * 0.5
}

fn bits(v: Vec3) -> [u32; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

/// `mesh` with every polygon cut into cells no wider than `cell` metres on the ground plane.
fn tessellate(mesh: &NavMesh, cell: f32) -> NavMesh {
    let mut cells: Vec<Vec<Vec3>> = Vec::new();
    for polygon in &mesh.polygons {
        let mut pieces = vec![polygon.vertices.clone()];
        for axis in [0, 2] {
            let lo = polygon
                .vertices
                .iter()
                .map(|v| v[axis])
                .fold(f32::INFINITY, f32::min);
            let hi = polygon
                .vertices
                .iter()
                .map(|v| v[axis])
                .fold(f32::NEG_INFINITY, f32::max);
            let mut k = (lo / cell).floor() as i64;
            loop {
                let c = k as f32 * cell;
                if c >= hi - ON_LINE {
                    break;
                }
                if c > lo + ON_LINE {
                    pieces = pieces
                        .iter()
                        .flat_map(|piece| split(piece, axis, c))
                        .map(|mut piece| {
                            piece.dedup();
                            if piece.len() > 1 && piece.first() == piece.last() {
                                piece.pop();
                            }
                            piece
                        })
                        .filter(|piece| piece.len() >= 3 && ground_area(piece) > 0.0)
                        .collect();
                }
                k += 1;
            }
        }
        for mut piece in pieces {
            piece.dedup();
            if piece.len() > 1 && piece.first() == piece.last() {
                piece.pop();
            }
            if piece.len() >= 3 && ground_area(&piece) > 0.0 {
                cells.push(piece);
            }
        }
    }
    let mut edges: HashMap<([u32; 3], [u32; 3]), Vec<(usize, usize)>> = HashMap::new();
    for (index, vertices) in cells.iter().enumerate() {
        for edge in 0..vertices.len() {
            let (a, b) = (
                bits(vertices[edge]),
                bits(vertices[(edge + 1) % vertices.len()]),
            );
            let key = if a < b { (a, b) } else { (b, a) };
            edges.entry(key).or_default().push((index, edge));
        }
    }
    let mut polygons: Vec<Polygon> = cells
        .into_iter()
        .map(|vertices| Polygon {
            neighbours: vec![None; vertices.len()],
            vertices,
        })
        .collect();
    for sides in edges.values() {
        if let [(a, ea), (b, eb)] = sides[..] {
            polygons[a].neighbours[ea] = Some(b);
            polygons[b].neighbours[eb] = Some(a);
        }
    }
    NavMesh {
        polygons,
        limits: mesh.limits,
    }
}

/// Connected groups of polygons, to check tessellation keeps every link.
fn components(mesh: &NavMesh) -> usize {
    let mut seen = vec![false; mesh.polygons.len()];
    let mut groups = 0;
    for first in 0..mesh.polygons.len() {
        if seen[first] {
            continue;
        }
        groups += 1;
        let mut stack = vec![first];
        seen[first] = true;
        while let Some(at) = stack.pop() {
            for next in mesh.polygons[at].neighbours.iter().flatten() {
                if !seen[*next] {
                    seen[*next] = true;
                    stack.push(*next);
                }
            }
        }
    }
    groups
}

/// Each polygon's component number.
fn component_of(mesh: &NavMesh) -> Vec<usize> {
    let mut label = vec![usize::MAX; mesh.polygons.len()];
    let mut next = 0;
    for first in 0..mesh.polygons.len() {
        if label[first] != usize::MAX {
            continue;
        }
        let mut stack = vec![first];
        label[first] = next;
        while let Some(at) = stack.pop() {
            for &n in mesh.polygons[at].neighbours.iter().flatten() {
                if label[n] == usize::MAX {
                    label[n] = next;
                    stack.push(n);
                }
            }
        }
        next += 1;
    }
    label
}

/// Metres of `path` whose sample points `seen` rules out, sampled every 5 cm.
fn leaked(path: &[Vec3], seen: &mut PointRule<'_>) -> f32 {
    const STEP: f32 = 0.05;
    let mut metres = 0.0;
    for pair in path.windows(2) {
        let n = ((pair[0].distance(pair[1]) / STEP).ceil() as usize).max(1);
        let piece = pair[0].distance(pair[1]) / n as f32;
        for i in 0..n {
            let mid = pair[0].lerp(pair[1], (i as f32 + 0.5) / n as f32);
            if seen(mid) {
                metres += piece;
            }
        }
    }
    metres
}

fn length(path: &[Vec3]) -> f32 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::sync::LazyLock;

    use moho_physics::{HitTag, PhysicsWorld, RayQuery};
    use moho_render_api::{WorldMesh, WorldMeshId};

    use super::*;
    use crate::nav::AgentProfile;

    const PROFILE: AgentProfile = AgentProfile {
        radius: 0.4,
        height: 2.0,
        max_climb: 0.5,
        max_slope: std::f32::consts::FRAC_PI_4,
    };

    /// Searchlight's `nav_profile` (capsule radius 0.405 + clearance 0.1, hull 1.83).
    const SEARCHLIGHT: AgentProfile = AgentProfile {
        radius: 0.505,
        height: 1.83,
        max_climb: 0.5,
        max_slope: std::f32::consts::FRAC_PI_4,
    };
    const EYE: f32 = 1.63;
    /// Searchlight's `cover_hide_height`: the body point a threat must not see.
    const BODY: f32 = 0.9;

    fn box_mesh(min: Vec3, max: Vec3) -> WorldMesh {
        let positions = vec![
            [min.x, min.y, min.z],
            [max.x, min.y, min.z],
            [max.x, max.y, min.z],
            [min.x, max.y, min.z],
            [min.x, min.y, max.z],
            [max.x, min.y, max.z],
            [max.x, max.y, max.z],
            [min.x, max.y, max.z],
        ];
        let indices = vec![
            0, 3, 2, 0, 2, 1, 4, 5, 6, 4, 6, 7, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6, 5, 0, 1, 5, 0, 5,
            4, 3, 7, 6, 3, 6, 2,
        ];
        WorldMesh::new(
            positions,
            vec![[0.0, 1.0, 0.0]; 8],
            vec![1.0; 8],
            vec![[0.0; 3]; 8],
            vec![1.0; 8],
            vec![0; 8],
            indices,
        )
        .expect("a box")
    }

    // -----------------------------------------------------------------------------------
    // Lane fixture: a 20 m floor split by a divider from the south edge to z = 2. Start and
    // goal sit either side of it. A lane of fire (|x| < half, z < end) covers the divider's
    // end, so the hidden way bends north of the lane through the same open polygons.

    fn lane_world() -> Vec<WorldMesh> {
        vec![
            box_mesh(Vec3::new(-10.0, -1.0, -10.0), Vec3::new(10.0, 0.0, 10.0)),
            box_mesh(Vec3::new(-0.5, 0.0, -10.0), Vec3::new(0.5, 3.0, 2.0)),
        ]
    }

    #[derive(Clone, Copy, Debug)]
    struct Lane {
        half: f32,
        end: f32,
    }

    impl Lane {
        /// The rule the route is asked with.
        fn seen(self, p: Vec3) -> bool {
            p.z < self.end && p.x.abs() < self.half
        }

        /// The judge of a finished route: seen when 1 cm or more inside the lane, so a route
        /// that rides the lane's edge (zero margin) is reported apart from one that enters it.
        fn inside(self, p: Vec3) -> bool {
            p.z < self.end - 0.01 && p.x.abs() < self.half - 0.01
        }
    }

    const LANES: [Lane; 4] = [
        Lane {
            half: 3.0,
            end: 5.0,
        },
        Lane {
            half: 1.5,
            end: 4.0,
        },
        Lane {
            half: 2.0,
            end: 3.0,
        },
        Lane {
            half: 4.0,
            end: 7.0,
        },
    ];
    const LANE: Lane = LANES[0];

    const LANE_FROM: Vec3 = Vec3::new(-6.0, 0.0, -4.0);
    const LANE_TO: Vec3 = Vec3::new(6.0, 0.0, -4.0);

    static LANE_MESH: LazyLock<NavMesh> =
        LazyLock::new(|| NavMesh::build(&lane_world(), PROFILE).expect("the lane builds"));

    fn lane_outcome(mesh: &NavMesh, lane: Lane, approach: Approach) -> Outcome {
        let refined = match approach {
            Approach::Cells { cell } | Approach::Table { cell } => Some(tessellate(mesh, cell)),
            _ => None,
        };
        ask(
            mesh,
            refined.as_ref(),
            approach,
            LANE_FROM,
            LANE_TO,
            &mut |p| lane.seen(p),
            None,
        )
    }

    const LANE_APPROACHES: [Approach; 6] = [
        Approach::PolyCentroid,
        Approach::PolyAll,
        Approach::Segment { spacing: 0.5 },
        Approach::Cells { cell: 2.0 },
        Approach::Cells { cell: 1.0 },
        Approach::Cells { cell: 0.5 },
    ];

    #[test]
    fn lane_report() {
        let mesh = &*LANE_MESH;
        let largest = mesh
            .polygons
            .iter()
            .map(|p| ground_area(&p.vertices))
            .fold(0.0_f32, f32::max);
        println!(
            "lane fixture: {} polygons, largest {largest:.1} m²",
            mesh.polygons.len()
        );
        let shortest = mesh.path(LANE_FROM, LANE_TO).expect("a plain path");
        for lane in LANES {
            let straddling = mesh
                .polygons
                .iter()
                .filter(|p| {
                    let seen = p.vertices.iter().filter(|&&v| lane.seen(v)).count();
                    seen > 0 && seen < p.vertices.len()
                })
                .count();
            println!(
                "\nlane |x| < {}, z < {}: {straddling} polygons straddle its edge; shortest path {:.2} m, {:.2} m inside",
                lane.half,
                lane.end,
                length(&shortest),
                leaked(&shortest, &mut |p| lane.inside(p))
            );
            for approach in LANE_APPROACHES {
                let outcome = lane_outcome(mesh, lane, approach);
                match &outcome.path {
                    Some(path) => println!(
                        "  {:<34} route {:.2} m, inside {:.2} m, on edge {:.2} m, {} areas, {} points",
                        approach.name(),
                        length(path),
                        leaked(path, &mut |p| lane.inside(p)),
                        leaked(path, &mut |p| lane.seen(p)) - leaked(path, &mut |p| lane.inside(p)),
                        outcome.areas,
                        outcome.points
                    ),
                    None => println!(
                        "  {:<34} no path, {} areas, {} points",
                        approach.name(),
                        outcome.areas,
                        outcome.points
                    ),
                }
            }
        }
    }

    #[test]
    fn a_shortest_path_crosses_the_lane() {
        let shortest = LANE_MESH.path(LANE_FROM, LANE_TO).expect("a plain path");
        assert!(leaked(&shortest, &mut |p| LANE.inside(p)) > 1.0);
    }

    #[test]
    fn cells_route_around_every_lane_without_entering_it() {
        for lane in LANES {
            for cell in [1.0, 0.5] {
                let outcome = lane_outcome(&LANE_MESH, lane, Approach::Cells { cell });
                let path = outcome.path.expect("a hidden way exists");
                assert_eq!(
                    leaked(&path, &mut |p| lane.inside(p)),
                    0.0,
                    "{lane:?}, cell {cell}: {path:?}"
                );
            }
        }
    }

    #[test]
    fn tessellation_keeps_every_link_and_roughly_the_shortest_route() {
        let plain = LANE_MESH.path(LANE_FROM, LANE_TO).expect("coarse");
        for cell in [2.0, 1.0, 0.5] {
            let refined = tessellate(&LANE_MESH, cell);
            assert_eq!(components(&refined), components(&LANE_MESH), "cell {cell}");
            let fine = refined.path(LANE_FROM, LANE_TO).expect("fine");
            println!(
                "cell {cell}: shortest {:.3} m coarse, {:.3} m on cells",
                length(&plain),
                length(&fine)
            );
            assert!(length(&fine) <= length(&plain) * 1.10, "cell {cell}");
        }
    }

    #[test]
    fn every_approach_repeats_its_route_and_survives_a_rebuild() {
        let rebuilt = NavMesh::build(&lane_world(), PROFILE).expect("rebuild");
        assert_eq!(rebuilt, *LANE_MESH);
        for lane in LANES {
            for approach in LANE_APPROACHES {
                let first = lane_outcome(&LANE_MESH, lane, approach);
                let second = lane_outcome(&LANE_MESH, lane, approach);
                let third = lane_outcome(&rebuilt, lane, approach);
                assert_eq!(first, second, "{}", approach.name());
                assert_eq!(first, third, "{}", approach.name());
            }
        }
    }

    // -----------------------------------------------------------------------------------
    // Proving ground: real geometry, real rays (moho_physics, as searchlight's cover test).

    fn level(name: &str) -> Vec<WorldMesh> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../searchlight/assets/{name}.glb"));
        crate::load(&path)
            .expect("the level loads")
            .meshes
            .into_iter()
            .map(|m| m.mesh)
            .collect()
    }

    fn physics(meshes: &[WorldMesh]) -> PhysicsWorld {
        let mut world = PhysicsWorld::new();
        for (id, mesh) in meshes.iter().enumerate() {
            world.set_world_mesh(WorldMeshId(id as u64), mesh, HitTag(1));
        }
        world.step(1.0 / 60.0);
        world
    }

    /// A threat at `eye` sees the body point above `p` when no level geometry is between.
    fn sees(world: &PhysicsWorld, eye: Vec3, p: Vec3, rays: &Cell<usize>) -> bool {
        rays.set(rays.get() + 1);
        let target = p + Vec3::Y * BODY;
        let offset = target - eye;
        let distance = offset.length();
        distance < 1e-3
            || world
                .cast_ray(&RayQuery {
                    origin: eye,
                    direction: offset,
                    max_distance: distance,
                    ignore: None,
                })
                .is_none()
    }

    /// A small deterministic generator (no new dependency).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % n
        }
    }

    struct Question {
        from: Vec3,
        to: Vec3,
        threats: Vec<Vec3>,
        /// Whether the plain shortest path is seen somewhere.
        plain_seen: bool,
    }

    /// `count` questions with `threats` eyes each, ends and threats on polygon centroids,
    /// keeping only those whose ends are joined and hidden from every threat.
    fn questions(
        mesh: &NavMesh,
        world: &PhysicsWorld,
        count: usize,
        threats: usize,
        seed: u64,
    ) -> Vec<Question> {
        let mut lcg = Lcg(seed);
        let n = mesh.polygons.len();
        let rays = Cell::new(0);
        let mut out = Vec::new();
        let mut tries = 0;
        while out.len() < count && tries < count * 2000 {
            tries += 1;
            let from = centroid(&mesh.polygons[lcg.next(n)]);
            let to = centroid(&mesh.polygons[lcg.next(n)]);
            let eyes: Vec<Vec3> = (0..threats)
                .map(|_| centroid(&mesh.polygons[lcg.next(n)]) + Vec3::Y * EYE)
                .collect();
            let Some(plain) = mesh.path(from, to) else {
                continue;
            };
            if length(&plain) < 8.0 {
                continue;
            }
            let hidden = |p: Vec3| eyes.iter().all(|&e| !sees(world, e, p, &rays));
            if hidden(from) && hidden(to) {
                let plain_seen = leaked(&plain, &mut |p| !hidden(p)) > 0.0;
                out.push(Question {
                    from,
                    to,
                    threats: eyes,
                    plain_seen,
                });
            }
        }
        out
    }

    struct Table {
        rows: Vec<Vec<u64>>,
    }

    impl Table {
        fn build(mesh: &NavMesh, world: &PhysicsWorld) -> Self {
            let n = mesh.polygons.len();
            let centres: Vec<Vec3> = mesh.polygons.iter().map(centroid).collect();
            let rays = Cell::new(0);
            let rows = centres
                .iter()
                .map(|&from| {
                    let mut row = vec![0_u64; n.div_ceil(64)];
                    for (j, &to) in centres.iter().enumerate() {
                        if sees(world, from + Vec3::Y * EYE, to, &rays) {
                            row[j / 64] |= 1 << (j % 64);
                        }
                    }
                    row
                })
                .collect();
            Self { rows }
        }

        fn seen(&self, from: usize, cell: usize) -> bool {
            self.rows[from][cell / 64] & (1 << (cell % 64)) != 0
        }
    }

    fn median(mut values: Vec<f64>) -> f64 {
        values.sort_by(f64::total_cmp);
        values.get(values.len() / 2).copied().unwrap_or(f64::NAN)
    }

    #[test]
    #[ignore = "spike report; run with --release --ignored --nocapture"]
    fn arena_report() {
        level_report("arena");
    }

    #[test]
    #[ignore = "spike report; run with --release --ignored --nocapture"]
    fn proving_ground_report() {
        level_report("proving_ground");
    }

    fn level_report(name: &str) {
        let meshes = level(name);
        let started = Instant::now();
        let coarse = NavMesh::build(&meshes, SEARCHLIGHT).expect("builds");
        let build_ms = started.elapsed().as_secs_f64() * 1e3;
        let world = physics(&meshes);
        let areas: Vec<f32> = coarse
            .polygons
            .iter()
            .map(|p| ground_area(&p.vertices))
            .collect();
        println!(
            "{name}: {} polygons (build {build_ms:.0} ms), total {:.0} m², largest {:.1} m², median {:.1} m²",
            coarse.polygons.len(),
            areas.iter().sum::<f32>(),
            areas.iter().copied().fold(0.0, f32::max),
            median(areas.iter().map(|&a| f64::from(a)).collect())
        );
        let cells = [2.0_f32, 1.0, 0.5];
        let refined: Vec<(f32, NavMesh, f64)> = cells
            .iter()
            .map(|&cell| {
                let started = Instant::now();
                let mesh = tessellate(&coarse, cell);
                let ms = started.elapsed().as_secs_f64() * 1e3;
                assert_eq!(components(&mesh), components(&coarse), "cell {cell}");
                println!(
                    "  cells {cell} m: {} cells, tessellate {ms:.1} ms, {} components (coarse {})",
                    mesh.polygons.len(),
                    components(&mesh),
                    components(&coarse)
                );
                (cell, mesh, ms)
            })
            .collect();
        let table_cell = 1.0;
        let table_mesh = &refined.iter().find(|r| r.0 == table_cell).expect("1 m").1;
        let started = Instant::now();
        let table = Table::build(table_mesh, &world);
        let n = table_mesh.polygons.len();
        println!(
            "  table over {n} cells of {table_cell} m: {} rays, {:.0} ms, {:.0} KiB",
            n * n,
            started.elapsed().as_secs_f64() * 1e3,
            (n * n.div_ceil(64) * 8) as f64 / 1024.0
        );

        let mut approaches = vec![
            Approach::PolyCentroid,
            Approach::PolyAll,
            Approach::Segment { spacing: 0.5 },
        ];
        approaches.extend(cells.iter().map(|&cell| Approach::Cells { cell }));
        approaches.push(Approach::Table { cell: table_cell });

        let rebuilt = NavMesh::build(&meshes, SEARCHLIGHT).expect("rebuilds");
        println!(
            "  rebuilt navmesh identical: {}; tessellations identical: {}",
            rebuilt == coarse,
            refined
                .iter()
                .all(|(cell, mesh, _)| tessellate(&rebuilt, *cell) == *mesh)
        );

        // Worst case: the goal lies in another component, so the search reaches every area
        // it can, and every point is judged against all four threats (none sees it).
        let label = component_of(&coarse);
        if let Some(other) = (0..coarse.polygons.len()).find(|&i| label[i] != label[0]) {
            let from = centroid(&coarse.polygons[0]);
            let to = centroid(&coarse.polygons[other]);
            let mut lcg = Lcg(0x5eed);
            let eyes: Vec<Vec3> = (0..4)
                .map(|_| {
                    centroid(&coarse.polygons[lcg.next(coarse.polygons.len())]) + Vec3::Y * EYE
                })
                .collect();
            println!("\nworst case (goal unreachable, 4 threats, every ray cast):");
            for &approach in &approaches {
                let mesh = match approach {
                    Approach::Cells { cell } | Approach::Table { cell } => {
                        Some(&refined.iter().find(|r| r.0 == cell).expect("refined").1)
                    }
                    _ => None,
                };
                let rays = Cell::new(0);
                let mut cache: HashMap<[u32; 3], bool> = HashMap::new();
                let mut rule = |p: Vec3| {
                    *cache.entry(bits(p)).or_insert_with(|| {
                        // Cast every ray, then answer "hidden" so nothing prunes the search.
                        let seen_by = eyes.iter().filter(|&&e| sees(&world, e, p, &rays)).count();
                        seen_by > eyes.len()
                    })
                };
                let nothing = |_: usize| false;
                let started = Instant::now();
                let outcome = ask(&coarse, mesh, approach, from, to, &mut rule, Some(&nothing));
                println!(
                    "  {:<34} {:>8.2} ms {:>7} areas {:>7} rays, path {}",
                    approach.name(),
                    started.elapsed().as_secs_f64() * 1e3,
                    outcome.areas,
                    rays.get(),
                    outcome.path.is_some()
                );
            }
        }

        for threats in [1, 4] {
            let set = questions(&coarse, &world, 40, threats, 0x1070 + threats as u64);
            println!(
                "\n{threats} threat(s), {} questions ({} whose shortest path is seen):",
                set.len(),
                set.iter().filter(|q| q.plain_seen).count()
            );
            println!(
                "  {:<34} {:>6} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8} {:>7}",
                "approach",
                "routes",
                "leaky",
                "leak m",
                "med ms",
                "max ms",
                "areas",
                "rays",
                "repeat"
            );
            for &approach in &approaches {
                let mesh = match approach {
                    Approach::Cells { cell } | Approach::Table { cell } => {
                        Some(&refined.iter().find(|r| r.0 == cell).expect("refined").1)
                    }
                    _ => None,
                };
                let (mut times, mut area_calls, mut ray_counts) =
                    (Vec::new(), Vec::new(), Vec::new());
                let (mut routes, mut leaky, mut leak_total, mut repeat) = (0, 0, 0.0_f32, true);
                for q in &set {
                    let rays = Cell::new(0);
                    let mut cache: HashMap<[u32; 3], bool> = HashMap::new();
                    let mut rule = |p: Vec3| {
                        *cache
                            .entry(bits(p))
                            .or_insert_with(|| q.threats.iter().any(|&e| sees(&world, e, p, &rays)))
                    };
                    let snapped: Vec<usize> = match mesh {
                        Some(m) if matches!(approach, Approach::Table { .. }) => q
                            .threats
                            .iter()
                            .filter_map(|&e| snap(m, e - Vec3::Y * EYE).map(|s| s.polygon))
                            .collect(),
                        _ => Vec::new(),
                    };
                    let lookup = |cell: usize| snapped.iter().any(|&from| table.seen(from, cell));
                    let started = Instant::now();
                    let outcome = ask(
                        &coarse,
                        mesh,
                        approach,
                        q.from,
                        q.to,
                        &mut rule,
                        Some(&lookup),
                    );
                    times.push(started.elapsed().as_secs_f64() * 1e3);
                    area_calls.push(outcome.areas as f64);
                    ray_counts.push(rays.get() as f64);
                    let mut again_cache: HashMap<[u32; 3], bool> = HashMap::new();
                    let again_rays = Cell::new(0);
                    let mut again_rule = |p: Vec3| {
                        *again_cache.entry(bits(p)).or_insert_with(|| {
                            q.threats.iter().any(|&e| sees(&world, e, p, &again_rays))
                        })
                    };
                    let again = ask(
                        &coarse,
                        mesh,
                        approach,
                        q.from,
                        q.to,
                        &mut again_rule,
                        Some(&lookup),
                    );
                    repeat &= again == outcome;
                    if let Some(path) = &outcome.path {
                        routes += 1;
                        let truth_rays = Cell::new(0);
                        let mut truth =
                            |p: Vec3| q.threats.iter().any(|&e| sees(&world, e, p, &truth_rays));
                        let metres = leaked(path, &mut truth);
                        if metres > 0.0 {
                            leaky += 1;
                            leak_total += metres;
                        }
                    }
                }
                println!(
                    "  {:<34} {:>6} {:>9} {:>9.1} {:>8.3} {:>8.3} {:>8.0} {:>8.0} {:>7}",
                    approach.name(),
                    routes,
                    leaky,
                    leak_total,
                    median(times.clone()),
                    times.iter().copied().fold(0.0, f64::max),
                    median(area_calls),
                    median(ray_counts),
                    repeat
                );
            }
        }
    }
}
