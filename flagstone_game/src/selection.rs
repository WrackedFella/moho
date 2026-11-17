//! Which pawn the player has selected, and the pure picking that decides it.

use glam::Vec3;
use moho_core::pointer::Ray;

/// Which pawn an order or a click refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PawnId(pub u32);

/// The one pawn that exists today.
pub const PLAYER_PAWN: PawnId = PawnId(0);

/// Half extents of the pawn box; spans the character capsule.
pub const PAWN_HALF_EXTENTS: Vec3 = Vec3::new(0.0, 0.0, 0.0);

/// An axis-aligned box in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// The pawn's box around its capsule centre.
pub fn pawn_aabb(_center: Vec3) -> Aabb {
    Aabb {
        min: Vec3::ZERO,
        max: Vec3::ZERO,
    }
}

/// The ray parameter at which `ray` enters `aabb` (0 when it starts inside), or `None`.
pub fn ray_box_entry(_ray: &Ray, _aabb: &Aabb) -> Option<f32> {
    None
}

/// The nearest pawn whose box the ray enters nearer than the terrain.
pub fn pick_pawn(
    _ray: &Ray,
    _pawns: impl IntoIterator<Item = (PawnId, Aabb)>,
    _terrain_distance: Option<f32>,
) -> Option<PawnId> {
    None
}

/// The selected pawn, if any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    selected: Option<PawnId>,
}

impl Selection {
    /// Replaces the selection with what the click hit.
    pub fn click(&mut self, _hit: Option<PawnId>) {}

    pub fn selected(&self) -> Option<PawnId> {
        self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const EPS: f32 = 1e-4;

    fn ray(origin: Vec3, direction: Vec3) -> Ray {
        Ray {
            origin,
            direction: direction.normalize(),
        }
    }

    fn unit_box() -> Aabb {
        Aabb {
            min: Vec3::splat(-1.0),
            max: Vec3::splat(1.0),
        }
    }

    #[test]
    fn pawn_aabb_is_centred_on_the_pawn_with_the_capsule_extents() {
        let center = Vec3::new(10.0, 20.0, -5.0);

        let aabb = pawn_aabb(center);

        assert_eq!(PAWN_HALF_EXTENTS, Vec3::new(0.3, 1.15, 0.3));
        assert!((aabb.min - (center - PAWN_HALF_EXTENTS)).length() < EPS);
        assert!((aabb.max - (center + PAWN_HALF_EXTENTS)).length() < EPS);
    }

    #[test]
    fn a_ray_through_the_pawn_picks_it() {
        let aabb = pawn_aabb(Vec3::new(0.0, 5.0, 0.0));
        let ray = ray(Vec3::new(0.0, 5.0, 10.0), Vec3::NEG_Z);

        let picked = pick_pawn(&ray, [(PLAYER_PAWN, aabb)], None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_just_outside_each_face_misses() {
        let aabb = unit_box();
        let mm = 0.001;
        // (origin offset on the face plane, direction); the ray runs parallel to the
        // face normal axis, displaced along a perpendicular axis.
        let cases: [(&str, Vec3, Vec3); 6] = [
            ("+x", Vec3::new(5.0, 0.0, 1.0), Vec3::NEG_X),
            ("-x", Vec3::new(-5.0, 0.0, 1.0), Vec3::X),
            ("+y", Vec3::new(0.0, 5.0, 1.0), Vec3::NEG_Y),
            ("-y", Vec3::new(0.0, -5.0, 1.0), Vec3::Y),
            ("+z", Vec3::new(1.0, 0.0, 5.0), Vec3::NEG_Z),
            ("-z", Vec3::new(1.0, 0.0, -5.0), Vec3::Z),
        ];

        for (face, edge_origin, direction) in cases {
            let mut outside = edge_origin;
            let mut inside = edge_origin;
            // Displace along the axis that holds the 1.0 (the face edge).
            if edge_origin.z == 1.0 {
                outside.z += mm;
                inside.z -= mm;
            } else {
                outside.x += mm;
                inside.x -= mm;
            }

            let miss = ray_box_entry(&self::ray(outside, direction), &aabb);
            let hit = ray_box_entry(&self::ray(inside, direction), &aabb);

            assert!(miss.is_none(), "{face}: 1 mm outside must miss");
            assert!(hit.is_some(), "{face}: 1 mm inside must hit");
        }
    }

    #[test]
    fn terrain_nearer_than_the_pawn_hides_it() {
        let aabb = pawn_aabb(Vec3::new(0.0, 5.0, 0.0));
        let ray = ray(Vec3::new(0.0, 5.0, 10.0), Vec3::NEG_Z);

        let hidden = pick_pawn(&ray, [(PLAYER_PAWN, aabb)], Some(5.0));
        let visible = pick_pawn(&ray, [(PLAYER_PAWN, aabb)], None);

        assert_eq!(hidden, None);
        assert_eq!(visible, Some(PLAYER_PAWN), "only the terrain hides it");
    }

    #[test]
    fn terrain_behind_the_pawn_does_not_hide_it() {
        let aabb = pawn_aabb(Vec3::new(0.0, 5.0, 0.0));
        let ray = ray(Vec3::new(0.0, 5.0, 10.0), Vec3::NEG_Z);

        let picked = pick_pawn(&ray, [(PLAYER_PAWN, aabb)], Some(20.0));

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn no_terrain_hit_leaves_the_pawn_pickable() {
        let aabb = pawn_aabb(Vec3::new(0.0, 5.0, 0.0));
        let ray = ray(Vec3::new(0.0, 5.0, 10.0), Vec3::NEG_Z);

        let picked = pick_pawn(&ray, [(PLAYER_PAWN, aabb)], None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_pawn_behind_the_ray_origin_is_not_picked() {
        let behind = pawn_aabb(Vec3::new(0.0, 5.0, 20.0));
        let ahead = pawn_aabb(Vec3::new(0.0, 5.0, 0.0));
        let ray = ray(Vec3::new(0.0, 5.0, 10.0), Vec3::NEG_Z);

        let picked_behind = pick_pawn(&ray, [(PLAYER_PAWN, behind)], None);
        let picked_ahead = pick_pawn(&ray, [(PLAYER_PAWN, ahead)], None);

        assert_eq!(picked_behind, None);
        assert_eq!(picked_ahead, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_starting_inside_the_box_picks_it() {
        let aabb = unit_box();
        let ray = ray(Vec3::ZERO, Vec3::X);

        let entry = ray_box_entry(&ray, &aabb);
        let picked = pick_pawn(&ray, [(PLAYER_PAWN, aabb)], Some(3.0));

        assert_eq!(entry, Some(0.0));
        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_parallel_to_a_slab_outside_it_misses() {
        let aabb = unit_box();
        let above = ray(Vec3::new(-5.0, 2.0, 0.0), Vec3::X);
        let beside = ray(Vec3::new(-5.0, 0.0, -2.0), Vec3::X);
        let through = ray(Vec3::new(-5.0, 0.5, 0.0), Vec3::X);

        assert_eq!(ray_box_entry(&above, &aabb), None);
        assert_eq!(ray_box_entry(&beside, &aabb), None);
        let entry = ray_box_entry(&through, &aabb).expect("a parallel ray inside the slab hits");
        assert!((entry - 4.0).abs() < EPS, "entry {entry}");
    }

    #[test]
    fn the_nearer_of_two_pawns_wins() {
        let near = (PawnId(7), pawn_aabb(Vec3::new(0.0, 5.0, 0.0)));
        let far = (PawnId(9), pawn_aabb(Vec3::new(0.0, 5.0, -10.0)));
        let ray = ray(Vec3::new(0.0, 5.0, 10.0), Vec3::NEG_Z);

        let far_first = pick_pawn(&ray, [far, near], None);
        let near_first = pick_pawn(&ray, [near, far], None);

        assert_eq!(far_first, Some(PawnId(7)));
        assert_eq!(near_first, Some(PawnId(7)));
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

        assert_eq!(selection.selected(), Some(PLAYER_PAWN));
    }

    proptest! {
        #[test]
        fn ray_box_entry_agrees_with_a_point_inside_the_box(
            center in prop::array::uniform3(-50.0f32..50.0),
            half in prop::array::uniform3(0.1f32..5.0),
            fraction in prop::array::uniform3(-0.95f32..0.95),
            dir in prop::array::uniform3(-1.0f32..1.0),
            distance in 0.0f32..100.0,
        ) {
            let dir = Vec3::from(dir);
            prop_assume!(dir.length() > 0.1);
            let dir = dir.normalize();
            let center = Vec3::from(center);
            let half = Vec3::from(half);
            let aabb = Aabb { min: center - half, max: center + half };
            let interior = center + half * Vec3::from(fraction);
            let ray = Ray { origin: interior - dir * distance, direction: dir };

            let entry = ray_box_entry(&ray, &aabb);

            let t = entry.expect("a ray aimed at an interior point hits the box");
            prop_assert!(t >= 0.0);
            prop_assert!(t <= distance + 1e-3);
            let p = ray.origin + ray.direction * t;
            let slack = 1e-3 * (1.0 + p.abs().max_element());
            let inside = (p - aabb.min).min_element() >= -slack
                && (aabb.max - p).min_element() >= -slack;
            prop_assert!(inside, "entry point {p:?} is outside the box");
            let on_surface = (p - aabb.min).abs().min_element() <= slack
                || (p - aabb.max).abs().min_element() <= slack;
            prop_assert!(t == 0.0 || on_surface, "entry point {p:?} is not on the surface");
        }
    }
}
