//! Which pawn the player has clicked, and the pure picking maths behind it.

use glam::Vec3;
use moho_core::pointer::Ray;

/// Identity of a pawn.
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

/// The pawn's box around its capsule centre.
pub fn pawn_aabb(_center: Vec3) -> Aabb {
    todo!("pawn_aabb")
}

/// Distance along `ray` at which it enters `aabb`, or `None` on a miss.
pub fn ray_box_entry(_ray: Ray, _aabb: &Aabb) -> Option<f32> {
    todo!("ray_box_entry")
}

/// The nearest pawn whose box the ray enters before the terrain.
pub fn pick_pawn(
    _ray: Ray,
    _pawns: impl IntoIterator<Item = (PawnId, Aabb)>,
    _terrain_distance: Option<f32>,
) -> Option<PawnId> {
    todo!("pick_pawn")
}

/// The currently selected pawn, if any.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    selected: Option<PawnId>,
}

impl Selection {
    /// Replaces the selection with what the click hit.
    pub fn click(&mut self, _hit: Option<PawnId>) {
        todo!("Selection::click")
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

        let picked = pick_pawn(ray, [(PLAYER_PAWN, unit_box())], Some(50.0));

        assert_eq!(picked, Some(PLAYER_PAWN));
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
    fn the_nearer_of_two_pawns_wins() {
        let ray = ray(Vec3::new(0.0, 0.0, 20.0), Vec3::NEG_Z);
        let far = (PawnId(1), pawn_box_at(Vec3::new(0.0, 0.0, -5.0)));
        let near = (PawnId(2), pawn_box_at(Vec3::new(0.0, 0.0, 5.0)));

        let picked = pick_pawn(ray, [far, near], None);

        assert_eq!(picked, Some(PawnId(2)));
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
    fn a_click_on_a_pawn_selects_it() {
        let mut selection = Selection::default();

        selection.click(Some(PLAYER_PAWN));

        assert_eq!(selection.selected(), Some(PLAYER_PAWN));
    }
}
