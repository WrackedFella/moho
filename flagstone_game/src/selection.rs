//! Picking and selecting the player's pawn from a strategy-view click.

use glam::Vec3;
use moho_core::pointer::Ray;

/// Which pawn. One value is in use today, [`PLAYER_PAWN`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PawnId(pub u32);

/// The player's pawn, the only one that exists.
pub const PLAYER_PAWN: PawnId = PawnId(0);

/// Half extents of the pawn's box: the character capsule's footprint and height.
pub const PAWN_HALF_EXTENTS: Vec3 = Vec3::new(0.3, 1.15, 0.3);

/// An axis-aligned box in world space.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// The pawn's box when the capsule centre is at `center`.
pub fn pawn_aabb(_center: Vec3) -> Aabb {
    todo!()
}

/// The distance along `ray` at which it first enters `aabb`, `None` when it never does or the
/// box lies wholly behind the origin. A ray starting inside the box enters it at `0.0`.
pub fn ray_box_entry(_ray: Ray, _aabb: Aabb) -> Option<f32> {
    todo!()
}

/// The nearest pawn whose box `ray` enters at `t >= 0` and `t < terrain_distance`.
/// `None` for `terrain_distance` means the ray hit no terrain.
pub fn pick_pawn(
    _ray: Ray,
    _pawns: impl IntoIterator<Item = (PawnId, Aabb)>,
    _terrain_distance: Option<f32>,
) -> Option<PawnId> {
    todo!()
}

/// Which pawn the player has selected, if any.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    selected: Option<PawnId>,
}

impl Selection {
    pub fn selected(&self) -> Option<PawnId> {
        self.selected
    }

    /// Replaces the selection with what a click hit: a pawn selects it, nothing clears it.
    pub fn click(&mut self, _hit: Option<PawnId>) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const TOLERANCE: f32 = 1e-4;

    fn ray(origin: Vec3, direction: Vec3) -> Ray {
        Ray {
            origin,
            direction: direction.normalize(),
        }
    }

    fn pawn_at(center: Vec3) -> Vec<(PawnId, Aabb)> {
        vec![(PLAYER_PAWN, pawn_aabb(center))]
    }

    /// A ray along +Z from 10 m in front of a pawn standing at the origin, shifted by `offset`.
    fn ray_at_the_origin_pawn(offset: Vec3) -> Ray {
        ray(Vec3::new(0.0, 0.0, -10.0) + offset, Vec3::Z)
    }

    #[test]
    fn pawn_aabb_is_centred_on_the_pawn_with_the_capsule_extents() {
        let center = Vec3::new(1.0, 2.0, 3.0);

        let aabb = pawn_aabb(center);

        assert_eq!(PAWN_HALF_EXTENTS, Vec3::new(0.3, 1.15, 0.3));
        assert!(
            aabb.min.abs_diff_eq(Vec3::new(0.7, 0.85, 2.7), TOLERANCE),
            "min {:?}",
            aabb.min
        );
        assert!(
            aabb.max.abs_diff_eq(Vec3::new(1.3, 3.15, 3.3), TOLERANCE),
            "max {:?}",
            aabb.max
        );
    }

    #[test]
    fn a_ray_through_the_pawn_picks_it() {
        let pawns = pawn_at(Vec3::ZERO);

        let picked = pick_pawn(ray_at_the_origin_pawn(Vec3::ZERO), pawns, None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn ray_box_entry_returns_the_distance_to_the_near_face() {
        let aabb = pawn_aabb(Vec3::ZERO);

        let entry = ray_box_entry(ray_at_the_origin_pawn(Vec3::ZERO), aabb);

        let entry = entry.expect("the ray runs through the box");
        assert!((entry - 9.7).abs() < TOLERANCE, "entry {entry}");
    }

    #[test]
    fn a_ray_just_outside_each_face_misses() {
        const MILLIMETRE: f32 = 0.001;
        let half = PAWN_HALF_EXTENTS;
        // (face, direction of travel, the axis offset that reaches the face)
        let faces = [
            ("+x", Vec3::Z, Vec3::X * half.x),
            ("-x", Vec3::Z, Vec3::NEG_X * half.x),
            ("+y", Vec3::Z, Vec3::Y * half.y),
            ("-y", Vec3::Z, Vec3::NEG_Y * half.y),
            ("+z", Vec3::X, Vec3::Z * half.z),
            ("-z", Vec3::X, Vec3::NEG_Z * half.z),
        ];

        for (face, travel, to_face) in faces {
            let normal = to_face.normalize();
            let start = -travel * 10.0;
            let outside = ray(start + to_face + normal * MILLIMETRE, travel);
            let inside = ray(start + to_face - normal * MILLIMETRE, travel);

            let missed = pick_pawn(outside, pawn_at(Vec3::ZERO), None);
            let hit = pick_pawn(inside, pawn_at(Vec3::ZERO), None);

            assert_eq!(missed, None, "{face}: 1 mm outside");
            assert_eq!(hit, Some(PLAYER_PAWN), "{face}: 1 mm inside");
        }
    }

    #[test]
    fn terrain_nearer_than_the_pawn_hides_it() {
        let pawns = pawn_at(Vec3::ZERO);

        let picked = pick_pawn(ray_at_the_origin_pawn(Vec3::ZERO), pawns, Some(5.0));

        assert_eq!(picked, None);
    }

    #[test]
    fn terrain_behind_the_pawn_does_not_hide_it() {
        let pawns = pawn_at(Vec3::ZERO);

        let picked = pick_pawn(ray_at_the_origin_pawn(Vec3::ZERO), pawns, Some(20.0));

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn no_terrain_hit_leaves_the_pawn_pickable() {
        let pawns = pawn_at(Vec3::ZERO);

        let picked = pick_pawn(ray_at_the_origin_pawn(Vec3::ZERO), pawns, None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn terrain_exactly_at_the_pawns_entry_hides_it() {
        let aim = ray_at_the_origin_pawn(Vec3::ZERO);
        let entry = ray_box_entry(aim, pawn_aabb(Vec3::ZERO)).expect("the ray runs through");

        let at_entry = pick_pawn(aim, pawn_at(Vec3::ZERO), Some(entry));
        let just_beyond = pick_pawn(aim, pawn_at(Vec3::ZERO), Some(entry + 0.01));

        assert_eq!(at_entry, None);
        assert_eq!(just_beyond, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_pawn_behind_the_ray_origin_is_not_picked() {
        let aim = ray(Vec3::new(0.0, 0.0, 10.0), Vec3::Z);

        let picked = pick_pawn(aim, pawn_at(Vec3::ZERO), None);

        assert_eq!(picked, None);
        assert_eq!(ray_box_entry(aim, pawn_aabb(Vec3::ZERO)), None);
    }

    #[test]
    fn a_ray_starting_inside_the_box_picks_it() {
        let aim = ray(Vec3::new(0.1, 0.2, 0.0), Vec3::new(1.0, 1.0, 1.0));

        let picked = pick_pawn(aim, pawn_at(Vec3::ZERO), Some(3.0));

        assert_eq!(picked, Some(PLAYER_PAWN));
        assert_eq!(ray_box_entry(aim, pawn_aabb(Vec3::ZERO)), Some(0.0));
    }

    #[test]
    fn a_ray_parallel_to_a_slab_outside_it_misses() {
        let above = ray(Vec3::new(0.0, 5.0, -10.0), Vec3::Z);
        let beside = ray(Vec3::new(5.0, 0.0, -10.0), Vec3::Z);
        let below_along_x = ray(Vec3::new(-10.0, -5.0, 0.0), Vec3::X);

        for (case, aim) in [
            ("above", above),
            ("beside", beside),
            ("below", below_along_x),
        ] {
            let picked = pick_pawn(aim, pawn_at(Vec3::ZERO), None);

            assert_eq!(picked, None, "{case}");
        }
    }

    #[test]
    fn the_nearer_of_two_pawns_wins() {
        let (near, far) = (PawnId(7), PawnId(9));
        let near_box = pawn_aabb(Vec3::new(0.0, 0.0, -4.0));
        let far_box = pawn_aabb(Vec3::new(0.0, 0.0, 6.0));
        let aim = ray_at_the_origin_pawn(Vec3::ZERO);

        let near_listed_first = pick_pawn(aim, [(near, near_box), (far, far_box)], None);
        let far_listed_first = pick_pawn(aim, [(far, far_box), (near, near_box)], None);

        assert_eq!(near_listed_first, Some(near));
        assert_eq!(far_listed_first, Some(near));
    }

    #[test]
    fn a_click_on_a_pawn_selects_it() {
        let mut selection = Selection::default();

        selection.click(Some(PLAYER_PAWN));

        assert_eq!(selection.selected(), Some(PLAYER_PAWN));
    }

    #[test]
    fn a_click_that_hits_nothing_clears_the_selection() {
        let mut selection = Selection::default();
        selection.click(Some(PLAYER_PAWN));

        selection.click(None);

        assert_eq!(selection.selected(), None);
    }

    #[test]
    fn a_click_on_another_pawn_replaces_the_selection() {
        let mut selection = Selection::default();
        selection.click(Some(PLAYER_PAWN));

        selection.click(Some(PawnId(5)));

        assert_eq!(selection.selected(), Some(PawnId(5)));
    }

    proptest! {
        #[test]
        fn ray_box_entry_agrees_with_a_point_inside_the_box(
            center in prop::array::uniform3(-50.0f32..50.0),
            inside in prop::array::uniform3(0.05f32..0.95),
            away in prop::array::uniform3(-1.0f32..1.0),
            range in 5.0f32..50.0,
        ) {
            let away = Vec3::from(away);
            prop_assume!(away.length() > 0.1);
            let aabb = pawn_aabb(Vec3::from(center));
            let target = aabb.min + (aabb.max - aabb.min) * Vec3::from(inside);
            let origin = target + away.normalize() * range;
            let aim = ray(origin, target - origin);

            let entry = ray_box_entry(aim, aabb);

            let t = entry.expect("a ray aimed at an interior point enters the box");
            prop_assert!(t >= 0.0 && t <= range + TOLERANCE, "t {t} of range {range}");
            let point = aim.origin + aim.direction * t;
            let slack = Vec3::splat(1e-3);
            prop_assert!(
                point.cmpge(aabb.min - slack).all() && point.cmple(aabb.max + slack).all(),
                "entry {point:?} outside {aabb:?}"
            );
            let on_a_face = (0..3).any(|axis| {
                (point[axis] - aabb.min[axis]).abs() < 1e-3
                    || (point[axis] - aabb.max[axis]).abs() < 1e-3
            });
            prop_assert!(on_a_face, "entry {point:?} is not on the surface of {aabb:?}");
        }
    }
}
