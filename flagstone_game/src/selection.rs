//! Picking a pawn with the pointer ray, and the player's selection.

use glam::Vec3;
use moho_core::pointer::Ray;

/// Half extents of the pawn's pick and draw box, matching the character capsule.
pub const PAWN_HALF_EXTENTS: Vec3 = Vec3::new(0.3, 1.15, 0.3);

/// Identifies a pawn for selection.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PawnId(pub u32);

/// The one pawn in play.
pub const PLAYER_PAWN: PawnId = PawnId(0);

/// An axis-aligned box.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// The pick box of a pawn whose capsule centre is `center`.
pub fn pawn_aabb(center: Vec3) -> Aabb {
    // TODO: centre +- PAWN_HALF_EXTENTS; this stub is a point box.
    Aabb {
        min: center,
        max: center,
    }
}

/// The distance along `ray` at which it enters `aabb` (the slab method); `Some(0.0)` when
/// it starts inside, `None` when it misses or the box lies behind the origin.
pub fn ray_box_entry(_ray: Ray, _aabb: Aabb) -> Option<f32> {
    // TODO: the slab method; this stub never hits.
    None
}

/// The nearest pawn whose box `ray` enters at `t >= 0` and `t < terrain_distance`; a `None`
/// terrain distance means the ray met no terrain.
pub fn pick_pawn(
    _ray: Ray,
    _pawns: impl IntoIterator<Item = (PawnId, Aabb)>,
    _terrain_distance: Option<f32>,
) -> Option<PawnId> {
    // TODO: the nearest box entry in front of the terrain; this stub picks nothing.
    None
}

/// Which pawn the player has selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    selected: Option<PawnId>,
}

impl Selection {
    /// The selected pawn, if any.
    pub fn selected(&self) -> Option<PawnId> {
        self.selected
    }

    /// Replaces the selection with what a click hit: a pawn selects it, nothing clears.
    pub fn click(&mut self, _hit: Option<PawnId>) {
        // TODO: replace the selection with the hit; this stub changes nothing.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const MM: f32 = 0.001;

    fn ray(origin: Vec3, direction: Vec3) -> Ray {
        Ray {
            origin,
            direction: direction.normalize(),
        }
    }

    /// Extents 2, 4 and 6, so a swapped axis changes the answer.
    fn crate_box() -> Aabb {
        Aabb {
            min: Vec3::new(1.0, 2.0, 3.0),
            max: Vec3::new(3.0, 6.0, 9.0),
        }
    }

    /// A box the +Z ray from the origin at `(0, 1, 0)` enters at exactly 4.
    fn ahead_box() -> Aabb {
        Aabb {
            min: Vec3::new(-0.5, 0.0, 4.0),
            max: Vec3::new(0.5, 2.0, 5.0),
        }
    }

    fn forward_ray() -> Ray {
        ray(Vec3::new(0.0, 1.0, 0.0), Vec3::Z)
    }

    const PAWN: PawnId = PLAYER_PAWN;
    const OTHER: PawnId = PawnId(7);

    #[test]
    fn pawn_aabb_is_centred_on_the_pawn_with_the_capsule_extents() {
        let center = Vec3::new(10.0, 20.0, 30.0);

        let aabb = pawn_aabb(center);

        assert!(
            (aabb.min - (center - Vec3::new(0.3, 1.15, 0.3)))
                .abs()
                .max_element()
                < 1e-5
        );
        assert!(
            (aabb.max - (center + Vec3::new(0.3, 1.15, 0.3)))
                .abs()
                .max_element()
                < 1e-5
        );
        assert_eq!(PAWN_HALF_EXTENTS, Vec3::new(0.3, 1.15, 0.3));
    }

    #[test]
    fn a_ray_through_the_pawn_picks_it() {
        let center = Vec3::new(0.0, 1.0, 5.0);
        let aim = ray(Vec3::new(0.0, 1.0, -10.0), Vec3::Z);

        let picked = pick_pawn(aim, [(PAWN, pawn_aabb(center))], None);

        assert_eq!(picked, Some(PAWN));
    }

    #[test]
    fn a_ray_through_the_pawn_reports_its_entry_distance() {
        let entry = ray_box_entry(forward_ray(), ahead_box());

        assert_eq!(entry, Some(4.0));
    }

    #[test]
    fn a_ray_just_outside_each_face_misses() {
        let aabb = crate_box();
        let center = (aabb.min + aabb.max) * 0.5;

        for face_axis in 0..3 {
            for (side, plane) in [("min", aabb.min[face_axis]), ("max", aabb.max[face_axis])] {
                let toward_inside = if side == "min" { 1.0 } else { -1.0 };
                let travel_axis = (face_axis + 1) % 3;
                let mut travel = Vec3::ZERO;
                travel[travel_axis] = 1.0;
                for (label, offset, expected_hit) in [
                    ("outside", -toward_inside * MM, false),
                    ("inside", toward_inside * MM, true),
                ] {
                    let mut origin = center;
                    origin[face_axis] = plane + offset;
                    origin[travel_axis] = aabb.min[travel_axis] - 5.0;

                    let hit = pick_pawn(ray(origin, travel), [(PAWN, aabb)], None);

                    assert_eq!(
                        hit.is_some(),
                        expected_hit,
                        "axis {face_axis} {side} face, 1 mm {label}"
                    );
                }
            }
        }
    }

    #[test]
    fn terrain_nearer_than_the_pawn_hides_it() {
        for (terrain, expected) in [
            (None, Some(PAWN)),
            (Some(3.9), None),
            (Some(4.0), None),
            (Some(4.1), Some(PAWN)),
        ] {
            let picked = pick_pawn(forward_ray(), [(PAWN, ahead_box())], terrain);

            assert_eq!(
                picked, expected,
                "terrain at {terrain:?}, pawn entered at 4"
            );
        }
    }

    #[test]
    fn terrain_behind_the_pawn_does_not_hide_it() {
        let picked = pick_pawn(forward_ray(), [(PAWN, ahead_box())], Some(4.1));

        assert_eq!(picked, Some(PAWN));
    }

    #[test]
    fn no_terrain_hit_leaves_the_pawn_pickable() {
        let picked = pick_pawn(forward_ray(), [(PAWN, ahead_box())], None);

        assert_eq!(picked, Some(PAWN));
    }

    #[test]
    fn a_pawn_behind_the_ray_origin_is_not_picked() {
        let behind = ray(Vec3::new(0.0, 1.0, 10.0), Vec3::Z);
        let facing = ray(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z);

        let picked = pick_pawn(behind, [(PAWN, ahead_box())], None);
        let turned_round = pick_pawn(facing, [(PAWN, ahead_box())], None);

        assert_eq!(picked, None);
        assert_eq!(
            turned_round,
            Some(PAWN),
            "the same pawn is picked facing it"
        );
    }

    #[test]
    fn a_ray_starting_inside_the_box_picks_it() {
        let inside = ray(Vec3::new(0.0, 1.0, 4.5), Vec3::Z);

        let picked = pick_pawn(inside, [(PAWN, ahead_box())], Some(1.0));

        assert_eq!(picked, Some(PAWN));
    }

    #[test]
    fn a_ray_parallel_to_a_slab_outside_it_misses() {
        let aabb = crate_box();
        let center = (aabb.min + aabb.max) * 0.5;
        for travel_axis in 0..3 {
            let parallel_axis = (travel_axis + 1) % 3;
            let mut direction = Vec3::ZERO;
            direction[travel_axis] = 1.0;
            for zero in [0.0_f32, -0.0] {
                direction[parallel_axis] = zero;
                for (outside_by, expected_hit) in [(5.0, false), (-5.0, false), (0.0, true)] {
                    let mut origin = center;
                    origin[travel_axis] = aabb.min[travel_axis] - 5.0;
                    origin[parallel_axis] = if outside_by > 0.0 {
                        aabb.max[parallel_axis] + outside_by
                    } else if outside_by < 0.0 {
                        aabb.min[parallel_axis] + outside_by
                    } else {
                        center[parallel_axis]
                    };

                    let hit = pick_pawn(Ray { origin, direction }, [(PAWN, aabb)], None);

                    assert_eq!(
                        hit.is_some(),
                        expected_hit,
                        "travel {travel_axis}, parallel {parallel_axis} ({zero:?}), offset {outside_by}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_nearer_of_two_pawns_wins() {
        let near = Aabb {
            min: Vec3::new(-0.5, 0.0, 4.0),
            max: Vec3::new(0.5, 2.0, 5.0),
        };
        let far = Aabb {
            min: Vec3::new(-0.5, 0.0, 8.0),
            max: Vec3::new(0.5, 2.0, 9.0),
        };

        let near_first = pick_pawn(forward_ray(), [(PAWN, near), (OTHER, far)], None);
        let far_first = pick_pawn(forward_ray(), [(OTHER, far), (PAWN, near)], None);

        assert_eq!(near_first, Some(PAWN));
        assert_eq!(far_first, Some(PAWN));
    }

    #[test]
    fn a_pawn_the_ray_misses_is_not_picked_beside_one_it_hits() {
        let hit = ahead_box();
        let missed = Aabb {
            min: Vec3::new(5.0, 0.0, 2.0),
            max: Vec3::new(6.0, 2.0, 3.0),
        };

        let picked = pick_pawn(forward_ray(), [(OTHER, missed), (PAWN, hit)], None);

        assert_eq!(picked, Some(PAWN));
    }

    proptest! {
        #[test]
        fn ray_box_entry_agrees_with_a_point_inside_the_box(
            min in proptest::array::uniform3(-10.0f32..10.0),
            size in proptest::array::uniform3(0.1f32..5.0),
            inner in proptest::array::uniform3(0.01f32..0.99),
            toward in proptest::array::uniform3(-1.0f32..1.0),
            distance in 10.0f32..50.0,
        ) {
            let toward = Vec3::from(toward);
            prop_assume!(toward.length() > 0.1);
            let direction = toward.normalize();
            let (min, size, inner) = (Vec3::from(min), Vec3::from(size), Vec3::from(inner));
            let aabb = Aabb { min, max: min + size };
            let target = min + size * inner;
            let origin = target - direction * distance;

            let entry = ray_box_entry(Ray { origin, direction }, aabb);

            let t = entry.expect("a ray aimed at an interior point enters the box");
            prop_assert!(t >= 0.0 && t <= distance + 1e-3, "t {t} vs distance {distance}");
            let point = origin + direction * t;
            let slack = Vec3::splat(2e-3);
            prop_assert!(
                point.cmpge(aabb.min - slack).all() && point.cmple(aabb.max + slack).all(),
                "entry {point:?} outside {aabb:?}"
            );
            let on_a_face = (0..3).any(|axis| {
                (point[axis] - aabb.min[axis]).abs() < 2e-3
                    || (point[axis] - aabb.max[axis]).abs() < 2e-3
            });
            prop_assert!(on_a_face, "entry {point:?} is not on the surface of {aabb:?}");
        }
    }

    #[test]
    fn a_click_that_hits_nothing_clears_the_selection() {
        let mut selection = Selection::default();
        selection.click(Some(PAWN));
        assert_eq!(selection.selected(), Some(PAWN), "the pawn starts selected");

        selection.click(None);

        assert_eq!(selection.selected(), None);
    }

    #[test]
    fn a_click_on_a_pawn_selects_it() {
        let mut selection = Selection::default();

        selection.click(Some(PAWN));
        let first = selection.selected();
        selection.click(Some(PAWN));
        let again = selection.selected();
        selection.click(Some(OTHER));

        assert_eq!(first, Some(PAWN));
        assert_eq!(again, Some(PAWN), "re-selecting keeps it selected");
        assert_eq!(selection.selected(), Some(OTHER), "a click replaces");
    }
}
