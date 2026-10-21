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

    /// The point of the polygon nearest `p` on the ground plane.
    fn closest_ground_point(&self, p: Vec2) -> Vec2 {
        let inside = self
            .corners()
            .all(|(a, b)| cross(a, b, Vec3::new(p.x, 0.0, p.y)) >= 0.0);
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
        (
            self.vertices[(edge + 1) % self.vertices.len()],
            self.vertices[edge],
        )
    }
}

fn snap(mesh: &NavMesh, p: Vec3) -> Option<Snapped> {
    let limits = mesh.limits;
    mesh.polygons
        .iter()
        .enumerate()
        .filter_map(|(index, polygon)| {
            let q = polygon.closest_ground_point(ground(p));
            let distance = q.distance(ground(p));
            if distance > limits.radius {
                return None;
            }
            let surface = polygon.height_at(q);
            if p.y - surface > limits.below || surface - p.y > limits.above {
                return None;
            }
            let gap = (p.y - surface).abs();
            let point = Vec3::new(q.x, surface, q.y);
            Some((
                distance,
                gap,
                Snapped {
                    polygon: index,
                    point,
                },
            ))
        })
        .min_by(|(d1, g1, _), (d2, g2, _)| d1.total_cmp(d2).then(g1.total_cmp(g2)))
        .map(|(_, _, snapped)| snapped)
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

/// The portals, as (left, right), crossed from `start` to `goal` along the route found by A*
/// over portal midpoints.
fn route(mesh: &NavMesh, start: &Snapped, goal: &Snapped) -> Option<Vec<(Vec3, Vec3)>> {
    let count = mesh.polygons.len();
    let mut cost = vec![f32::INFINITY; count];
    let mut position = vec![Vec3::ZERO; count];
    // Per polygon: the polygon it was reached from and the portal crossed.
    let mut parent: Vec<Option<(usize, (Vec3, Vec3))>> = vec![None; count];
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
            let through = cost[polygon] + position[polygon].distance(midpoint);
            if through < cost[next] {
                cost[next] = through;
                position[next] = midpoint;
                parent[next] = Some((polygon, portal));
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
    let portals = route(mesh, &start, &goal)?;
    Some(funnel(start.point, goal.point, &portals))
}
