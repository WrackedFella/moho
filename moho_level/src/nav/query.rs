//! Path queries: snap both ends, A* over polygons, funnel the portals into corners.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use glam::{Vec2, Vec3};

use super::{NavMesh, Polygon};

/// A query point moved onto a polygon.
struct Snapped {
    polygon: usize,
    point: Vec3,
}

fn ground(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

/// Twice the signed area of triangle `a`, `b`, `c` on the ground plane; positive when `c`
/// lies left of `a` to `b`.
fn cross(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    (b.x - a.x) * (c.z - a.z) - (b.z - a.z) * (c.x - a.x)
}

impl Polygon {
    fn corners(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        let n = self.vertices.len();
        (0..n).map(move |i| (self.vertices[i], self.vertices[(i + 1) % n]))
    }

    /// `1.0` for a polygon wound counter-clockwise on the ground plane (x right, z up),
    /// `-1.0` clockwise, `0.0` degenerate.
    fn winding(&self) -> f32 {
        let area: f32 = self.corners().map(|(a, b)| a.x * b.z - b.x * a.z).sum();
        if area > 0.0 {
            1.0
        } else if area < 0.0 {
            -1.0
        } else {
            0.0
        }
    }

    /// The point of the polygon nearest `p` on the ground plane.
    fn closest_ground_point(&self, p: Vec2) -> Vec2 {
        let winding = self.winding();
        let inside = winding != 0.0
            && self
                .corners()
                .all(|(a, b)| winding * cross(a, b, Vec3::new(p.x, 0.0, p.y)) >= 0.0);
        if inside {
            return p;
        }
        let mut best = (f32::INFINITY, p);
        for (a, b) in self.corners() {
            let (a, b) = (ground(a), ground(b));
            let edge = b - a;
            let length_sq = edge.length_squared();
            let t = if length_sq > 0.0 {
                ((p - a).dot(edge) / length_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let point = a + edge * t;
            let distance = point.distance_squared(p);
            if distance.total_cmp(&best.0).is_lt() {
                best = (distance, point);
            }
        }
        best.1
    }

    /// The surface height above `ground_point`, interpolated over the triangle fan from the
    /// first corner (polygons are not guaranteed planar).
    fn height_at(&self, ground_point: Vec2) -> f32 {
        let corners = &self.vertices;
        let at = Vec3::new(ground_point.x, 0.0, ground_point.y);
        let mut best: Option<(f32, f32)> = None;
        for i in 1..corners.len().saturating_sub(1) {
            let [first, second, third] = [corners[0], corners[i], corners[i + 1]];
            let area = cross(first, second, third);
            if area.abs() < 1e-9 {
                continue;
            }
            let weight_first = cross(at, second, third) / area;
            let weight_second = cross(first, at, third) / area;
            let weight_third = 1.0 - weight_first - weight_second;
            let inside = weight_first.min(weight_second).min(weight_third);
            if best.is_none_or(|(score, _)| inside > score) {
                let height =
                    weight_first * first.y + weight_second * second.y + weight_third * third.y;
                best = Some((inside, height));
            }
        }
        best.map_or_else(
            || corners.iter().map(|p| p.y).sum::<f32>() / corners.len() as f32,
            |(_, height)| height,
        )
    }

    /// The portal across edge `edge` as (left, right) for a traveller leaving the polygon.
    fn portal(&self, edge: usize) -> (Vec3, Vec3) {
        let (a, b) = self.corners().nth(edge).expect("edge index is in range");
        if self.winding() >= 0.0 {
            (b, a)
        } else {
            (a, b)
        }
    }
}

fn snap(mesh: &NavMesh, p: Vec3) -> Option<Snapped> {
    let limits = mesh.limits;
    let mut best: Option<(f32, f32, Snapped)> = None;
    for (index, polygon) in mesh.polygons.iter().enumerate() {
        let q = polygon.closest_ground_point(ground(p));
        let distance = q.distance(ground(p));
        if distance > limits.radius {
            continue;
        }
        let surface = polygon.height_at(q);
        if p.y - surface > limits.below || surface - p.y > limits.above {
            continue;
        }
        let gap = (p.y - surface).abs();
        let better = best
            .as_ref()
            .is_none_or(|(d, g, _)| distance.total_cmp(d).then(gap.total_cmp(g)).is_lt());
        if better {
            let point = Vec3::new(q.x, surface, q.y);
            best = Some((
                distance,
                gap,
                Snapped {
                    polygon: index,
                    point,
                },
            ));
        }
    }
    best.map(|(_, _, snapped)| snapped)
}

/// An open-list entry; the heap pops the lowest cost, then the lowest polygon index.
struct Open {
    cost: f32,
    polygon: usize,
}

impl PartialEq for Open {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Open {}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then(other.polygon.cmp(&self.polygon))
    }
}

/// The polygons from `start` to `goal`, found by A* over portal midpoints.
fn route(mesh: &NavMesh, start: &Snapped, goal: &Snapped) -> Option<Vec<usize>> {
    let count = mesh.polygons.len();
    let mut cost = vec![f32::INFINITY; count];
    let mut position = vec![Vec3::ZERO; count];
    let mut parent: Vec<Option<usize>> = vec![None; count];
    let mut closed = vec![false; count];
    let mut open = BinaryHeap::new();
    cost[start.polygon] = 0.0;
    position[start.polygon] = start.point;
    open.push(Open {
        cost: start.point.distance(goal.point),
        polygon: start.polygon,
    });
    while let Some(Open { polygon, .. }) = open.pop() {
        if closed[polygon] {
            continue;
        }
        closed[polygon] = true;
        if polygon == goal.polygon {
            let mut path = vec![polygon];
            let mut at = polygon;
            while let Some(before) = parent[at] {
                path.push(before);
                at = before;
            }
            path.reverse();
            return Some(path);
        }
        let current = &mesh.polygons[polygon];
        for (edge, neighbour) in current.neighbours.iter().enumerate() {
            let Some(next) = *neighbour else { continue };
            if closed[next] {
                continue;
            }
            let (left, right) = current.portal(edge);
            let midpoint = (left + right) * 0.5;
            let through = cost[polygon] + position[polygon].distance(midpoint);
            if through < cost[next] {
                cost[next] = through;
                position[next] = midpoint;
                parent[next] = Some(polygon);
                open.push(Open {
                    cost: through + midpoint.distance(goal.point),
                    polygon: next,
                });
            }
        }
    }
    None
}

/// Pulls the shortest corner list through `portals` (each `(left, right)`), from `start` to
/// `goal`, both included.
fn funnel(start: Vec3, goal: Vec3, portals: &[(Vec3, Vec3)]) -> Vec<Vec3> {
    let same = |a: Vec3, b: Vec3| a.x == b.x && a.z == b.z;
    let mut all = Vec::with_capacity(portals.len() + 2);
    all.push((start, start));
    all.extend_from_slice(portals);
    all.push((goal, goal));

    let mut path = vec![start];
    let (mut apex, mut left, mut right) = (start, start, start);
    let (mut left_at, mut right_at) = (0, 0);
    let mut i = 1;
    while i < all.len() {
        let (next_left, next_right) = all[i];
        if cross(apex, right, next_right) >= 0.0 {
            if same(apex, right) || cross(apex, left, next_right) < 0.0 {
                right = next_right;
                right_at = i;
            } else {
                apex = left;
                push_corner(&mut path, apex);
                (left, right, right_at) = (apex, apex, left_at);
                i = left_at + 1;
                continue;
            }
        }
        if cross(apex, left, next_left) <= 0.0 {
            if same(apex, left) || cross(apex, right, next_left) > 0.0 {
                left = next_left;
                left_at = i;
            } else {
                apex = right;
                push_corner(&mut path, apex);
                (left, right, left_at) = (apex, apex, right_at);
                i = right_at + 1;
                continue;
            }
        }
        i += 1;
    }
    if path.len() == 1 || !same(path[path.len() - 1], goal) {
        path.push(goal);
    }
    path
}

/// Adds `corner` unless the funnel already turned there; an apex can land on the previous
/// corner or on the goal itself.
fn push_corner(path: &mut Vec<Vec3>, corner: Vec3) {
    if path.last() != Some(&corner) {
        path.push(corner);
    }
}

pub(super) fn path(mesh: &NavMesh, from: Vec3, to: Vec3) -> Option<Vec<Vec3>> {
    let start = snap(mesh, from)?;
    let goal = snap(mesh, to)?;
    let polygons = route(mesh, &start, &goal)?;
    let portals: Vec<(Vec3, Vec3)> = polygons
        .windows(2)
        .map(|pair| {
            let from = &mesh.polygons[pair[0]];
            let edge = from
                .neighbours
                .iter()
                .position(|n| *n == Some(pair[1]))
                .expect("route steps follow neighbour links");
            from.portal(edge)
        })
        .collect();
    Some(funnel(start.point, goal.point, &portals))
}
