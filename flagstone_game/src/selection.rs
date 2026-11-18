//! Which pawn the player has selected, and the ray test that picks it.

use glam::Vec3;
use moho_core::pointer::Ray;

/// Identity of a pawn, for picking and for the orders given to it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PawnId(pub u32);

/// The one pawn that exists today.
pub const PLAYER_PAWN: PawnId = PawnId(0);

/// Half extents of the pawn's box, spanning the character capsule.
pub const PAWN_HALF_EXTENTS: Vec3 = Vec3::new(0.3, 1.15, 0.3);

/// An axis-aligned box in world space.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// The box of a pawn whose capsule centre is `center`.
pub fn pawn_aabb(center: Vec3) -> Aabb {
    let _ = center;
    todo!()
}

/// The distance along `ray` at which it enters `aabb`, `None` when it never does.
pub fn ray_box_entry(ray: Ray, aabb: Aabb) -> Option<f32> {
    let _ = (ray, aabb);
    todo!()
}

/// The nearest pawn whose box `ray` enters at a distance `t >= 0` and `t < terrain_distance`.
/// A `None` terrain distance means the ray met no terrain.
pub fn pick_pawn(
    ray: Ray,
    pawns: impl IntoIterator<Item = (PawnId, Aabb)>,
    terrain_distance: Option<f32>,
) -> Option<PawnId> {
    let _ = (ray, pawns.into_iter().count(), terrain_distance);
    todo!()
}

/// The pawn the player has selected, if any.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub selected: Option<PawnId>,
}

impl Selection {
    /// Replaces the selection with what a click hit: select, re-select and clear in one call.
    pub fn click(&mut self, hit: Option<PawnId>) {
        let _ = hit;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const MILLIMETRE: f32 = 0.001;

    fn near(actual: Vec3, expected: Vec3) -> bool {
        actual.distance(expected) < 1e-5
    }

    /// A box that differs on every axis, so a swapped axis is caught.
    fn lopsided() -> Aabb {
        Aabb {
            min: Vec3::new(-1.0, -2.0, -3.0),
            max: Vec3::new(1.0, 2.0, 3.0),
        }
    }

    fn unit_box_at(center: Vec3) -> Aabb {
        Aabb {
            min: center - Vec3::ONE,
            max: center + Vec3::ONE,
        }
    }

    fn ray(origin: Vec3, direction: Vec3) -> Ray {
        Ray {
            origin,
            direction: direction.normalize(),
        }
    }

    const NEAR_PAWN: PawnId = PawnId(1);
    const FAR_PAWN: PawnId = PawnId(2);

    #[test]
    fn pawn_aabb_is_centred_on_the_pawn_with_the_capsule_extents() {
        let center = Vec3::new(1.0, 2.0, 3.0);

        let aabb = pawn_aabb(center);

        assert_eq!(PAWN_HALF_EXTENTS, Vec3::new(0.3, 1.15, 0.3));
        assert!(near(aabb.min, Vec3::new(0.7, 0.85, 2.7)), "{aabb:?}");
        assert!(near(aabb.max, Vec3::new(1.3, 3.15, 3.3)), "{aabb:?}");
    }

    #[test]
    fn a_ray_through_the_pawn_picks_it() {
        let pawns = [(PLAYER_PAWN, unit_box_at(Vec3::new(0.0, 5.0, 0.0)))];
        let through = ray(Vec3::new(0.0, 5.0, -10.0), Vec3::Z);

        let picked = pick_pawn(through, pawns, None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn ray_box_entry_is_the_distance_to_the_first_face_crossed() {
        let aabb = unit_box_at(Vec3::ZERO);
        let toward = ray(Vec3::new(0.0, 0.0, -10.0), Vec3::Z);

        let entry = ray_box_entry(toward, aabb).expect("the ray crosses the box");

        assert!((entry - 9.0).abs() < 1e-4, "entry {entry}");
    }

    #[test]
    fn a_ray_just_outside_each_face_misses() {
        let aabb = lopsided();
        let mut results = Vec::new();

        for axis in 0..3 {
            for (face, outward) in [(aabb.min[axis], -1.0), (aabb.max[axis], 1.0)] {
                // Travels along the next axis, across the face's plane at `face`.
                let travel = (axis + 1) % 3;
                for (offset, should_hit) in [(MILLIMETRE, false), (-MILLIMETRE, true)] {
                    let mut origin = Vec3::ZERO;
                    origin[axis] = face + outward * offset;
                    origin[travel] = aabb.min[travel] - 5.0;
                    let mut direction = Vec3::ZERO;
                    direction[travel] = 1.0;

                    let picked = pick_pawn(ray(origin, direction), [(PLAYER_PAWN, aabb)], None);

                    results.push((axis, outward, should_hit, picked.is_some()));
                }
            }
        }

        let wrong: Vec<_> = results
            .iter()
            .filter(|(_, _, should_hit, hit)| should_hit != hit)
            .collect();
        assert_eq!(results.len(), 12, "six faces, two offsets each");
        assert!(
            wrong.is_empty(),
            "(axis, side, expected hit, hit): {wrong:?}"
        );
    }

    #[test]
    fn terrain_nearer_than_the_pawn_hides_it() {
        let pawns = [(PLAYER_PAWN, unit_box_at(Vec3::ZERO))];
        let toward = ray(Vec3::new(0.0, 0.0, -10.0), Vec3::Z);

        let picked = pick_pawn(toward, pawns, Some(5.0));

        assert_eq!(picked, None);
    }

    #[test]
    fn terrain_behind_the_pawn_does_not_hide_it() {
        let pawns = [(PLAYER_PAWN, unit_box_at(Vec3::ZERO))];
        let toward = ray(Vec3::new(0.0, 0.0, -10.0), Vec3::Z);

        let picked = pick_pawn(toward, pawns, Some(15.0));

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn no_terrain_hit_leaves_the_pawn_pickable() {
        let pawns = [(PLAYER_PAWN, unit_box_at(Vec3::ZERO))];
        let toward = ray(Vec3::new(0.0, 0.0, -10.0), Vec3::Z);

        let picked = pick_pawn(toward, pawns, None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_pawn_behind_the_ray_origin_is_not_picked() {
        let pawns = [(PLAYER_PAWN, unit_box_at(Vec3::ZERO))];
        let away = ray(Vec3::new(0.0, 0.0, 10.0), Vec3::Z);

        let picked = pick_pawn(away, pawns, None);

        assert_eq!(picked, None);
    }

    #[test]
    fn a_ray_starting_inside_the_box_picks_it() {
        let pawns = [(PLAYER_PAWN, unit_box_at(Vec3::ZERO))];
        let from_inside = ray(Vec3::new(0.1, 0.2, 0.3), Vec3::new(1.0, 2.0, 3.0));

        let picked = pick_pawn(from_inside, pawns, Some(100.0));

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_parallel_to_a_slab_outside_it_misses() {
        let pawns = [(PLAYER_PAWN, unit_box_at(Vec3::ZERO))];
        let beside = ray(Vec3::new(5.0, 0.0, -10.0), Vec3::Z);
        let above = ray(Vec3::new(0.0, 5.0, -10.0), Vec3::Z);

        let beside_picked = pick_pawn(beside, pawns, None);
        let above_picked = pick_pawn(above, pawns, None);

        assert_eq!(beside_picked, None, "outside the x slab");
        assert_eq!(above_picked, None, "outside the y slab");
    }

    #[test]
    fn the_nearer_of_two_pawns_wins() {
        let near_box = unit_box_at(Vec3::ZERO);
        let far_box = unit_box_at(Vec3::new(0.0, 0.0, 6.0));
        let toward = ray(Vec3::new(0.0, 0.0, -10.0), Vec3::Z);

        let near_first = pick_pawn(toward, [(NEAR_PAWN, near_box), (FAR_PAWN, far_box)], None);
        let far_first = pick_pawn(toward, [(FAR_PAWN, far_box), (NEAR_PAWN, near_box)], None);

        assert_eq!(near_first, Some(NEAR_PAWN));
        assert_eq!(far_first, Some(NEAR_PAWN));
    }

    #[test]
    fn a_click_that_hits_nothing_clears_the_selection() {
        let mut selection = Selection {
            selected: Some(PLAYER_PAWN),
        };

        selection.click(None);

        assert_eq!(selection.selected, None);
    }

    #[test]
    fn a_click_on_a_pawn_selects_it() {
        let mut selection = Selection::default();

        selection.click(Some(PLAYER_PAWN));

        assert_eq!(selection.selected, Some(PLAYER_PAWN));
    }

    proptest! {
        #[test]
        fn ray_box_entry_agrees_with_a_point_inside_the_box(
            fraction in proptest::array::uniform3(0.01f32..0.99),
            away in proptest::array::uniform3(-1.0f32..1.0),
            distance in 20.0f32..50.0,
        ) {
            prop_assume!(Vec3::from(away).length() > 0.1);
            let aabb = lopsided();
            let inside = aabb.min + (aabb.max - aabb.min) * Vec3::from(fraction);
            let origin = inside + Vec3::from(away).normalize() * distance;
            let aimed = ray(origin, inside - origin);

            let entry = ray_box_entry(aimed, aabb);

            let t = entry.expect("a ray aimed at an interior point enters the box");
            let point = aimed.origin + aimed.direction * t;
            let slack = Vec3::splat(2e-3);
            prop_assert!(t >= 0.0 && t <= distance + 1e-3, "t {}", t);
            prop_assert!(
                point.cmpge(aabb.min - slack).all() && point.cmple(aabb.max + slack).all(),
                "entry {:?} is outside the box",
                point
            );
            let on_a_face = (0..3).any(|axis| {
                (point[axis] - aabb.min[axis]).abs() < 2e-3
                    || (point[axis] - aabb.max[axis]).abs() < 2e-3
            });
            prop_assert!(on_a_face, "entry {:?} is not on the surface", point);
        }
    }
}
