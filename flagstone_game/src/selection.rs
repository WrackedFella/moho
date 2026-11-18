//! Which pawn the player has clicked, and the pure picking maths behind it.

use glam::Vec3;
use moho_core::pointer::Ray;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PawnId(pub u32);

/// The one pawn that exists today.
pub const PLAYER_PAWN: PawnId = PawnId(0);

/// Half extents of the pawn's box; spans the character capsule.
pub const PAWN_HALF_EXTENTS: Vec3 = Vec3::new(0.3, 1.15, 0.3);

/// An axis-aligned box in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

pub fn pawn_aabb(center: Vec3) -> Aabb {
    Aabb {
        min: center - PAWN_HALF_EXTENTS,
        max: center + PAWN_HALF_EXTENTS,
    }
}

/// Distance along `ray` at which it enters `aabb`, or `None` on a miss.
///
/// A ray that starts inside the box enters at `0.0`; a box wholly behind the
/// origin is a miss. Uses the slab method.
pub fn ray_box_entry(ray: Ray, aabb: &Aabb) -> Option<f32> {
    let mut t_near = f32::NEG_INFINITY;
    let mut t_far = f32::INFINITY;
    for axis in 0..3 {
        let origin = ray.origin[axis];
        let direction = ray.direction[axis];
        if direction == 0.0 {
            // Parallel to this slab: it either lies inside it for all t or never.
            if origin < aabb.min[axis] || origin > aabb.max[axis] {
                return None;
            }
            continue;
        }
        let t1 = (aabb.min[axis] - origin) / direction;
        let t2 = (aabb.max[axis] - origin) / direction;
        t_near = t_near.max(t1.min(t2));
        t_far = t_far.min(t1.max(t2));
    }
    (t_near <= t_far && t_far >= 0.0).then(|| t_near.max(0.0))
}

/// The nearest pawn whose box the ray enters before the terrain.
///
/// `terrain_distance` is the distance to the terrain the ray hit; `None`
/// means the ray hit no terrain, so nothing hides a pawn.
pub fn pick_pawn(
    ray: Ray,
    pawns: impl IntoIterator<Item = (PawnId, Aabb)>,
    terrain_distance: Option<f32>,
) -> Option<PawnId> {
    let limit = terrain_distance.unwrap_or(f32::INFINITY);
    pawns
        .into_iter()
        .filter_map(|(id, aabb)| ray_box_entry(ray, &aabb).map(|t| (id, t)))
        .filter(|&(_, t)| t < limit)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    selected: Option<PawnId>,
}

impl Selection {
    /// A click that hit no pawn (`None`) clears the selection.
    pub fn click(&mut self, hit: Option<PawnId>) {
        self.selected = hit;
    }

    pub fn selected(&self) -> Option<PawnId> {
        self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn unit_box() -> Aabb {
        Aabb {
            min: -PAWN_HALF_EXTENTS,
            max: PAWN_HALF_EXTENTS,
        }
    }

    fn ray(origin: Vec3, direction: Vec3) -> Ray {
        Ray {
            origin,
            direction: direction.normalize(),
        }
    }

    #[test]
    fn pawn_aabb_is_centred_on_the_pawn_with_the_capsule_extents() {
        let center = Vec3::new(10.0, 20.0, -5.0);

        let aabb = pawn_aabb(center);

        assert_eq!(aabb.min, center - Vec3::new(0.3, 1.15, 0.3));
        assert_eq!(aabb.max, center + Vec3::new(0.3, 1.15, 0.3));
    }

    #[test]
    fn a_ray_through_the_pawn_picks_it() {
        let ray = ray(Vec3::new(0.0, 0.0, 10.0), Vec3::NEG_Z);

        let picked = pick_pawn(ray, [(PLAYER_PAWN, unit_box())], None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_just_outside_each_face_misses() {
        let half = PAWN_HALF_EXTENTS.to_array();
        for face_axis in 0..3 {
            let travel_axis = (face_axis + 1) % 3;
            for sign in [-1.0_f32, 1.0] {
                for (offset, should_hit) in [(0.001_f32, false), (-0.001, true)] {
                    let mut origin = Vec3::ZERO;
                    origin[face_axis] = sign * (half[face_axis] + offset);
                    origin[travel_axis] = 10.0;
                    let mut direction = Vec3::ZERO;
                    direction[travel_axis] = -1.0;

                    let picked =
                        pick_pawn(Ray { origin, direction }, [(PLAYER_PAWN, unit_box())], None);

                    assert_eq!(
                        picked.is_some(),
                        should_hit,
                        "face axis {face_axis} sign {sign} offset {offset}"
                    );
                }
            }
        }

        for face_axis in 0..3 {
            let travel_axis = (face_axis + 1) % 3;
            for sign in [-1.0_f32, 1.0] {
                for (offset, should_hit) in [(-0.001_f32, false), (0.001, true)] {
                    // Crosses the face plane at the box's far edge along the travel axis,
                    // offset outside (miss) or inside (hit), moving at 45 degrees.
                    let mut crossing = Vec3::ZERO;
                    crossing[face_axis] = sign * half[face_axis];
                    crossing[travel_axis] = -half[travel_axis] + offset;
                    let mut direction = Vec3::ZERO;
                    direction[travel_axis] = -1.0;
                    direction[face_axis] = -sign;
                    let direction = direction.normalize();
                    let oblique = Ray {
                        origin: crossing - direction * 10.0,
                        direction,
                    };

                    let picked = pick_pawn(oblique, [(PLAYER_PAWN, unit_box())], None);

                    assert_eq!(
                        picked.is_some(),
                        should_hit,
                        "oblique face axis {face_axis} sign {sign} offset {offset}"
                    );
                }
            }
        }
    }

    #[test]
    fn terrain_nearer_than_the_pawn_hides_it() {
        let ray = ray(Vec3::new(0.0, 0.0, 10.0), Vec3::NEG_Z);

        let picked = pick_pawn(ray, [(PLAYER_PAWN, unit_box())], Some(5.0));

        assert_eq!(picked, None);
    }

    #[test]
    fn terrain_behind_the_pawn_does_not_hide_it() {
        let ray = ray(Vec3::new(0.0, 0.0, 10.0), Vec3::NEG_Z);

        let far_behind = pick_pawn(ray, [(PLAYER_PAWN, unit_box())], Some(50.0));
        let between_entry_and_exit = pick_pawn(ray, [(PLAYER_PAWN, unit_box())], Some(10.0));

        assert_eq!(far_behind, Some(PLAYER_PAWN));
        assert_eq!(between_entry_and_exit, Some(PLAYER_PAWN));
    }

    #[test]
    fn no_terrain_hit_leaves_the_pawn_pickable() {
        let ray = ray(Vec3::new(0.0, 0.0, 10.0), Vec3::NEG_Z);

        let picked = pick_pawn(ray, [(PLAYER_PAWN, unit_box())], None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_pawn_behind_the_ray_origin_is_not_picked() {
        let ray = ray(Vec3::new(0.0, 0.0, 10.0), Vec3::Z);

        let picked = pick_pawn(ray, [(PLAYER_PAWN, unit_box())], None);

        assert_eq!(picked, None);
    }

    #[test]
    fn a_ray_starting_inside_the_box_picks_it() {
        let ray = ray(Vec3::new(0.1, 0.2, 0.0), Vec3::NEG_Z);

        let picked = pick_pawn(ray, [(PLAYER_PAWN, unit_box())], Some(100.0));

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_parallel_to_a_slab_outside_it_misses() {
        let outside = ray(Vec3::new(-10.0, 2.0, 0.0), Vec3::X);
        let inside = ray(Vec3::new(-10.0, 0.5, 0.0), Vec3::X);

        let missed = pick_pawn(outside, [(PLAYER_PAWN, unit_box())], None);
        let hit = pick_pawn(inside, [(PLAYER_PAWN, unit_box())], None);

        assert_eq!(missed, None);
        assert_eq!(hit, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_parallel_to_a_slab_and_on_its_face_plane_still_hits() {
        let aabb = Aabb {
            min: Vec3::splat(-1.0),
            max: Vec3::splat(1.0),
        };
        for slab_axis in [1, 2] {
            for plane in [-1.0_f32, 1.0] {
                let mut origin = Vec3::new(-10.0, 0.0, 0.0);
                origin[slab_axis] = plane;

                let entry = ray_box_entry(
                    Ray {
                        origin,
                        direction: Vec3::X,
                    },
                    &aabb,
                );

                assert_eq!(entry, Some(9.0), "slab axis {slab_axis} plane {plane}");
            }
        }
    }

    #[test]
    fn terrain_exactly_at_the_pawn_entry_distance_hides_it() {
        let aabb = Aabb {
            min: Vec3::splat(-1.0),
            max: Vec3::splat(1.0),
        };
        let ray = Ray {
            origin: Vec3::new(0.0, 0.0, 10.0),
            direction: Vec3::NEG_Z,
        };

        let at_entry = pick_pawn(ray, [(PLAYER_PAWN, aabb)], Some(9.0));
        let just_beyond = pick_pawn(ray, [(PLAYER_PAWN, aabb)], Some(9.5));

        assert_eq!(at_entry, None);
        assert_eq!(just_beyond, Some(PLAYER_PAWN));
    }

    #[test]
    fn the_nearer_of_two_pawns_wins() {
        let ray = ray(Vec3::new(0.0, 0.0, 20.0), Vec3::NEG_Z);
        let far = (PawnId(1), pawn_box_at(Vec3::new(0.0, 0.0, -5.0)));
        let near = (PawnId(2), pawn_box_at(Vec3::new(0.0, 0.0, 5.0)));

        let far_first = pick_pawn(ray, [far, near], None);
        let near_first = pick_pawn(ray, [near, far], None);

        assert_eq!(far_first, Some(PawnId(2)));
        assert_eq!(near_first, Some(PawnId(2)));
    }

    fn pawn_box_at(center: Vec3) -> Aabb {
        Aabb {
            min: center - PAWN_HALF_EXTENTS,
            max: center + PAWN_HALF_EXTENTS,
        }
    }

    proptest! {
        #[test]
        fn ray_box_entry_agrees_with_a_point_inside_the_box(
            fx in 0.05_f32..0.95,
            fy in 0.05_f32..0.95,
            fz in 0.05_f32..0.95,
            dir in (-1.0_f32..1.0, -1.0_f32..1.0, -1.0_f32..1.0)
                .prop_filter("non-degenerate", |(x, y, z)| x * x + y * y + z * z > 0.04),
            distance in 5.0_f32..40.0,
        ) {
            let aabb = unit_box();
            let target = aabb.min + (aabb.max - aabb.min) * Vec3::new(fx, fy, fz);
            let origin = target + Vec3::new(dir.0, dir.1, dir.2).normalize() * distance;
            let ray = ray(origin, target - origin);

            let entry = ray_box_entry(ray, &aabb);

            prop_assert!(entry.is_some());
            let t = entry.unwrap();
            prop_assert!(t >= 0.0);
            let p = ray.origin + ray.direction * t;
            let eps = 1e-3;
            prop_assert!(t <= (target - origin).length() + eps);
            for axis in 0..3 {
                prop_assert!(p[axis] >= aabb.min[axis] - eps && p[axis] <= aabb.max[axis] + eps);
            }
            let on_surface = (0..3).any(|axis| {
                (p[axis] - aabb.min[axis]).abs() < eps || (p[axis] - aabb.max[axis]).abs() < eps
            });
            prop_assert!(on_surface);
        }
    }

    #[test]
    fn a_click_that_hits_nothing_clears_the_selection() {
        let mut selection = Selection::default();
        selection.click(Some(PLAYER_PAWN));

        selection.click(None);

        assert_eq!(selection.selected(), None);
    }

    #[test]
    fn a_click_on_another_pawn_moves_the_selection_to_it() {
        let mut selection = Selection::default();
        selection.click(Some(PawnId(1)));

        selection.click(Some(PawnId(2)));

        assert_eq!(selection.selected(), Some(PawnId(2)));
    }

    #[test]
    fn a_click_on_the_selected_pawn_keeps_it_selected() {
        let mut selection = Selection::default();
        selection.click(Some(PawnId(1)));

        selection.click(Some(PawnId(1)));

        assert_eq!(selection.selected(), Some(PawnId(1)));
    }

    #[test]
    fn a_click_on_a_pawn_selects_it() {
        let mut selection = Selection::default();

        selection.click(Some(PLAYER_PAWN));

        assert_eq!(selection.selected(), Some(PLAYER_PAWN));
    }
}
