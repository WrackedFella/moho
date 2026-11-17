//! Which pawn the player has selected, and the pick that selects it.

use glam::Vec3;
use moho_core::pointer::Ray;

/// Which pawn an order or a pick is about.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PawnId(pub u32);

/// The one pawn that exists today.
pub const PLAYER_PAWN: PawnId = PawnId(0);

/// An axis-aligned box in world space.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// Half the pawn box's size per axis: spans the character capsule.
pub const PAWN_HALF_EXTENTS: Vec3 = Vec3::new(0.3, 1.15, 0.3);

/// The pawn's box when its capsule centre is at `center`.
pub fn pawn_aabb(center: Vec3) -> Aabb {
    Aabb {
        min: center,
        max: center,
    }
}

/// The distance along `ray` at which it enters `aabb`, `None` when it never does.
pub fn ray_box_entry(_ray: Ray, _aabb: Aabb) -> Option<f32> {
    None
}

/// The nearest pawn whose box `ray` enters at `t >= 0` and `t < terrain_distance`.
pub fn pick_pawn(
    _ray: Ray,
    _pawns: impl IntoIterator<Item = (PawnId, Aabb)>,
    _terrain_distance: Option<f32>,
) -> Option<PawnId> {
    None
}

/// The player's current selection.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    selected: Option<PawnId>,
}

impl Selection {
    /// The selected pawn, if any.
    pub fn selected(&self) -> Option<PawnId> {
        self.selected
    }

    /// Replaces the selection with what the click hit.
    pub fn click(&mut self, _hit: Option<PawnId>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const MM: f32 = 0.001;

    fn centre() -> Vec3 {
        Vec3::new(10.0, 20.0, 30.0)
    }

    fn ray(origin: Vec3, direction: Vec3) -> Ray {
        Ray {
            origin,
            direction: direction.normalize(),
        }
    }

    fn pawn_box() -> Aabb {
        pawn_aabb(centre())
    }

    fn pick(ray: Ray, terrain: Option<f32>) -> Option<PawnId> {
        pick_pawn(ray, [(PLAYER_PAWN, pawn_box())], terrain)
    }

    #[test]
    fn pawn_aabb_is_centred_on_the_pawn_with_the_capsule_extents() {
        let aabb = pawn_aabb(centre());

        assert_eq!(aabb.min, centre() - Vec3::new(0.3, 1.15, 0.3));
        assert_eq!(aabb.max, centre() + Vec3::new(0.3, 1.15, 0.3));
        assert_eq!(PAWN_HALF_EXTENTS, Vec3::new(0.3, 1.15, 0.3));
    }

    #[test]
    fn a_ray_through_the_pawn_picks_it() {
        let from_above = ray(
            centre() + Vec3::new(5.0, 8.0, 5.0),
            Vec3::new(-5.0, -8.0, -5.0),
        );

        assert_eq!(pick(from_above, None), Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_just_outside_each_face_misses() {
        let half = PAWN_HALF_EXTENTS;
        let faces = [
            (Vec3::X, half.x, Vec3::Z),
            (Vec3::NEG_X, half.x, Vec3::Z),
            (Vec3::Y, half.y, Vec3::Z),
            (Vec3::NEG_Y, half.y, Vec3::Z),
            (Vec3::Z, half.z, Vec3::X),
            (Vec3::NEG_Z, half.z, Vec3::X),
        ];

        for (normal, extent, across) in faces {
            for (offset, expected) in [(MM, None), (-MM, Some(PLAYER_PAWN))] {
                // A ray parallel to the face, `offset` outside (positive) or inside
                // (negative) the plane, running across the box along `across`.
                let origin = centre() + normal * (extent + offset) - across * 5.0;
                let grazing = ray(origin, across);

                assert_eq!(
                    pick(grazing, None),
                    expected,
                    "face {normal:?}, offset {offset}"
                );
            }
        }
    }

    #[test]
    fn terrain_nearer_than_the_pawn_hides_it() {
        let from = centre() + Vec3::new(0.0, 0.0, 10.0);
        let toward = ray(from, Vec3::NEG_Z);

        let entry_distance = 10.0 - PAWN_HALF_EXTENTS.z;

        assert_eq!(pick(toward, Some(5.0)), None);
        assert_eq!(pick(toward, Some(entry_distance - 0.01)), None);
        assert_eq!(
            pick(toward, Some(entry_distance + 0.01)),
            Some(PLAYER_PAWN),
            "the same ray picks the pawn once the terrain is farther than its entry"
        );
    }

    #[test]
    fn terrain_behind_the_pawn_does_not_hide_it() {
        let from = centre() + Vec3::new(0.0, 0.0, 10.0);
        let toward = ray(from, Vec3::NEG_Z);

        assert_eq!(pick(toward, Some(50.0)), Some(PLAYER_PAWN));
    }

    #[test]
    fn no_terrain_hit_leaves_the_pawn_pickable() {
        let from = centre() + Vec3::new(0.0, 0.0, 10.0);
        let toward = ray(from, Vec3::NEG_Z);

        assert_eq!(pick(toward, None), Some(PLAYER_PAWN));
    }

    #[test]
    fn a_pawn_behind_the_ray_origin_is_not_picked() {
        let from = centre() + Vec3::new(0.0, 0.0, 10.0);
        let away = ray(from, Vec3::Z);

        let toward = ray(from, Vec3::NEG_Z);

        assert_eq!(pick(away, None), None);
        assert_eq!(
            pick(toward, None),
            Some(PLAYER_PAWN),
            "the opposite direction picks it"
        );
    }

    #[test]
    fn a_ray_starting_inside_the_box_picks_it() {
        let inside = ray(centre(), Vec3::new(1.0, 2.0, 3.0));

        assert_eq!(pick(inside, None), Some(PLAYER_PAWN));
        assert_eq!(pick(inside, Some(0.5)), Some(PLAYER_PAWN));
        assert_eq!(pick(inside, Some(0.2)), Some(PLAYER_PAWN));
        assert_eq!(ray_box_entry(inside, pawn_box()), Some(0.0));
    }

    #[test]
    fn a_ray_parallel_to_a_slab_outside_it_misses() {
        let above = centre() + Vec3::new(-5.0, PAWN_HALF_EXTENTS.y + 0.5, 0.0);
        let level = ray(above, Vec3::X);

        let through = ray(centre() + Vec3::new(-5.0, 0.0, 0.0), Vec3::X);

        assert_eq!(pick(level, None), None);
        assert_eq!(
            pick(through, None),
            Some(PLAYER_PAWN),
            "the same direction at the pawn's height picks it"
        );
    }

    #[test]
    fn the_nearer_of_two_pawns_wins() {
        let near = PawnId(1);
        let far = PawnId(2);
        let origin = Vec3::new(0.0, 0.0, 20.0);
        let toward = ray(origin, Vec3::NEG_Z);
        let at = |z: f32| pawn_aabb(Vec3::new(0.0, 0.0, z));

        let far_first = pick_pawn(toward, [(far, at(0.0)), (near, at(10.0))], None);
        let near_first = pick_pawn(toward, [(near, at(10.0)), (far, at(0.0))], None);

        assert_eq!(far_first, Some(near));
        assert_eq!(near_first, Some(near));
    }

    #[test]
    fn a_click_that_hits_nothing_clears_the_selection() {
        let mut selection = Selection::default();
        selection.click(Some(PLAYER_PAWN));
        let before = selection.selected();

        selection.click(None);

        assert_eq!(before, Some(PLAYER_PAWN));
        assert_eq!(selection.selected(), None);
    }

    #[test]
    fn a_click_on_a_pawn_selects_it() {
        let mut selection = Selection::default();

        selection.click(Some(PLAYER_PAWN));
        let first = selection.selected();
        selection.click(Some(PawnId(7)));

        assert_eq!(first, Some(PLAYER_PAWN));
        assert_eq!(selection.selected(), Some(PawnId(7)), "re-select replaces");
    }

    proptest! {
        #[test]
        fn ray_box_entry_agrees_with_a_point_inside_the_box(
            fx in 0.01f32..0.99,
            fy in 0.01f32..0.99,
            fz in 0.01f32..0.99,
            dir in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0)
                .prop_filter("a usable direction", |(x, y, z)| x * x + y * y + z * z > 0.1),
            distance in 5.0f32..40.0,
        ) {
            let aabb = pawn_box();
            let size = aabb.max - aabb.min;
            let target = aabb.min + size * Vec3::new(fx, fy, fz);
            let back = Vec3::new(dir.0, dir.1, dir.2).normalize();
            let aimed = Ray {
                origin: target + back * distance,
                direction: -back,
            };

            let t = ray_box_entry(aimed, aabb);

            prop_assert!(t.is_some(), "a ray aimed at an interior point must hit");
            let t = t.unwrap();
            prop_assert!(t >= 0.0 && t <= distance + 1e-3, "entry {t} past the aim point");
            let entry = aimed.origin + aimed.direction * t;
            let eps = 1e-3;
            let inside = (0..3).all(|a| entry[a] >= aabb.min[a] - eps && entry[a] <= aabb.max[a] + eps);
            let on_surface = (0..3)
                .any(|a| (entry[a] - aabb.min[a]).abs() < eps || (entry[a] - aabb.max[a]).abs() < eps);
            prop_assert!(inside && on_surface, "entry {entry:?} is not on the box surface");
        }
    }
}
