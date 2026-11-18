//! Picking the pawn with a mouse ray and remembering which pawn is selected.

use glam::Vec3;
use moho_core::pointer::Ray;

/// Identifies a selectable pawn.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PawnId(pub u32);

/// The pawn the player controls.
pub const PLAYER_PAWN: PawnId = PawnId(0);

/// Half the pawn's box size on each axis; matches the physics capsule.
pub const PAWN_HALF_EXTENTS: Vec3 = Vec3::new(0.3, 1.15, 0.3);

/// An axis-aligned box in world space.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// The pawn's box when its centre is at `center`.
pub fn pawn_aabb(_center: Vec3) -> Aabb {
    todo!("pawn_aabb: box centred on the pawn with PAWN_HALF_EXTENTS")
}

/// Distance along `ray` at which it enters `aabb`; 0 when the origin is inside,
/// `None` when it misses or the box lies behind the origin.
pub fn ray_box_entry(_ray: Ray, _aabb: Aabb) -> Option<f32> {
    todo!("ray_box_entry: slab method")
}

/// The nearest pawn whose box `ray` enters before `terrain_distance`.
/// `None` terrain means nothing blocks the ray.
pub fn pick_pawn(
    _ray: Ray,
    _pawns: impl IntoIterator<Item = (PawnId, Aabb)>,
    _terrain_distance: Option<f32>,
) -> Option<PawnId> {
    todo!("pick_pawn: nearest box entry in front of the terrain")
}

/// Which pawn, if any, is selected.
#[derive(Debug, Default)]
pub struct Selection {
    selected: Option<PawnId>,
}

impl Selection {
    /// Replaces the selection with the pawn a click hit, or clears it on `None`.
    pub fn click(&mut self, _hit: Option<PawnId>) {
        todo!("Selection::click: replace the selection with the hit")
    }

    pub fn selected(&self) -> Option<PawnId> {
        self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const CENTER: Vec3 = Vec3::new(10.0, 5.0, -3.0);

    fn ray(origin: Vec3, direction: Vec3) -> Ray {
        Ray {
            origin,
            direction: direction.normalize(),
        }
    }

    fn pawn_at_center() -> Aabb {
        Aabb {
            min: CENTER - PAWN_HALF_EXTENTS,
            max: CENTER + PAWN_HALF_EXTENTS,
        }
    }

    fn ray_at_center(distance: f32) -> Ray {
        ray(CENTER + Vec3::new(0.0, 0.0, distance), Vec3::NEG_Z)
    }

    #[test]
    fn pawn_aabb_is_centred_on_the_pawn_with_the_capsule_extents() {
        let aabb = pawn_aabb(CENTER);

        assert_eq!(aabb.min, CENTER - Vec3::new(0.3, 1.15, 0.3));
        assert_eq!(aabb.max, CENTER + Vec3::new(0.3, 1.15, 0.3));
    }

    #[test]
    fn a_ray_through_the_pawn_picks_it() {
        let origin = CENTER + Vec3::new(-6.0, 8.0, 6.0);
        let from_above = ray(origin, CENTER - origin);

        let picked = pick_pawn(from_above, [(PLAYER_PAWN, pawn_at_center())], None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    /// Rays along `(1, 0, -1)` and `(1, -1, 0)` that pass an edge of the box `offset` metres
    /// (measured along the sum of the two moving axes) beyond it; positive misses.
    fn assert_oblique_edges_decide_by_interval_overlap() {
        let aabb = pawn_at_center();
        let delta = 0.0014;
        let xz_edge = |offset: f32| {
            let sum = aabb.max.x + aabb.max.z + offset;
            let x = CENTER.x - 10.0;
            ray(Vec3::new(x, CENTER.y, sum - x), Vec3::new(1.0, 0.0, -1.0))
        };
        let top_edge = |offset: f32| {
            let sum = aabb.max.x + aabb.max.y + offset;
            let x = CENTER.x - 10.0;
            ray(Vec3::new(x, sum - x, CENTER.z), Vec3::new(1.0, -1.0, 0.0))
        };

        for (name, line) in [
            ("xz edge", &xz_edge as &dyn Fn(f32) -> Ray),
            ("top edge", &top_edge),
        ] {
            let outside = pick_pawn(line(delta), [(PLAYER_PAWN, aabb)], None);
            let inside = pick_pawn(line(-delta), [(PLAYER_PAWN, aabb)], None);

            assert_eq!(outside, None, "{name}: 1 mm outside must miss");
            assert_eq!(inside, Some(PLAYER_PAWN), "{name}: 1 mm inside must hit");
        }
    }

    #[test]
    fn a_ray_just_outside_each_face_misses() {
        let aabb = pawn_at_center();
        let mm = 0.001;
        for axis in 0..3 {
            let travel = (axis + 1) % 3;
            for sign in [-1.0_f32, 1.0] {
                let face = CENTER[axis] + sign * PAWN_HALF_EXTENTS[axis];
                let mut direction = Vec3::ZERO;
                direction[travel] = 1.0;
                let start = |offset: f32| {
                    let mut origin = CENTER;
                    origin[axis] = face + sign * offset;
                    origin[travel] -= 10.0;
                    ray(origin, direction)
                };

                let outside = pick_pawn(start(mm), [(PLAYER_PAWN, aabb)], None);
                let inside = pick_pawn(start(-mm), [(PLAYER_PAWN, aabb)], None);

                assert_eq!(outside, None, "axis {axis} sign {sign}: outside must miss");
                assert_eq!(
                    inside,
                    Some(PLAYER_PAWN),
                    "axis {axis} sign {sign}: inside must hit"
                );
            }
        }

        assert_oblique_edges_decide_by_interval_overlap();
    }

    #[test]
    fn terrain_nearer_than_the_pawn_hides_it() {
        let picked = pick_pawn(
            ray_at_center(10.0),
            [(PLAYER_PAWN, pawn_at_center())],
            Some(5.0),
        );

        assert_eq!(picked, None);
    }

    #[test]
    fn terrain_behind_the_pawn_does_not_hide_it() {
        // The box is entered at 9.7 and left at 10.3.
        for terrain in [9.8, 10.0, 10.2, 20.0] {
            let picked = pick_pawn(
                ray_at_center(10.0),
                [(PLAYER_PAWN, pawn_at_center())],
                Some(terrain),
            );

            assert_eq!(picked, Some(PLAYER_PAWN), "terrain at {terrain}");
        }
    }

    #[test]
    fn no_terrain_hit_leaves_the_pawn_pickable() {
        let picked = pick_pawn(ray_at_center(10.0), [(PLAYER_PAWN, pawn_at_center())], None);

        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_pawn_behind_the_ray_origin_is_not_picked() {
        let looking_away = ray(CENTER + Vec3::new(0.0, 0.0, 10.0), Vec3::Z);

        let picked = pick_pawn(looking_away, [(PLAYER_PAWN, pawn_at_center())], None);

        assert_eq!(picked, None);
    }

    #[test]
    fn a_ray_starting_inside_the_box_picks_it() {
        let inside = ray(CENTER, Vec3::new(1.0, 2.0, 3.0));

        let entry = ray_box_entry(inside, pawn_at_center());
        let picked = pick_pawn(inside, [(PLAYER_PAWN, pawn_at_center())], Some(5.0));

        assert_eq!(entry, Some(0.0));
        assert_eq!(picked, Some(PLAYER_PAWN));
    }

    #[test]
    fn a_ray_parallel_to_a_slab_outside_it_misses() {
        let beside = ray(CENTER + Vec3::new(1.0, 0.0, 10.0), Vec3::NEG_Z);

        let picked = pick_pawn(beside, [(PLAYER_PAWN, pawn_at_center())], None);

        assert_eq!(picked, None);
    }

    #[test]
    fn the_nearer_of_two_pawns_wins() {
        let other = PawnId(7);
        let near = Aabb {
            min: pawn_at_center().min + Vec3::Z * 4.0,
            max: pawn_at_center().max + Vec3::Z * 4.0,
        };
        let far = pawn_at_center();

        let near_listed_last = pick_pawn(
            ray_at_center(10.0),
            [(PLAYER_PAWN, far), (other, near)],
            None,
        );
        let near_listed_first = pick_pawn(
            ray_at_center(10.0),
            [(other, near), (PLAYER_PAWN, far)],
            None,
        );

        assert_eq!(near_listed_last, Some(other));
        assert_eq!(near_listed_first, Some(other));
    }

    proptest! {
        #[test]
        fn ray_box_entry_agrees_with_a_point_inside_the_box(
            fx in 0.05_f32..0.95,
            fy in 0.05_f32..0.95,
            fz in 0.05_f32..0.95,
            dx in -1.0_f32..1.0,
            dy in -1.0_f32..1.0,
            dz in -1.0_f32..1.0,
            back in 3.0_f32..50.0,
        ) {
            let direction = Vec3::new(dx, dy, dz);
            prop_assume!(direction.length() > 0.1);
            let aabb = pawn_at_center();
            let interior = aabb.min + (aabb.max - aabb.min) * Vec3::new(fx, fy, fz);
            let aimed = ray(interior - direction.normalize() * back, direction);

            let t = ray_box_entry(aimed, aabb);

            let t = t.expect("a ray aimed at an interior point must hit the box");
            prop_assert!(t >= 0.0 && t <= back + 1e-3);
            let entry = aimed.origin + aimed.direction * t;
            let eps = 2e-3;
            prop_assert!(entry.cmpge(aabb.min - eps).all() && entry.cmple(aabb.max + eps).all());
            let on_surface = (0..3).any(|a| {
                (entry[a] - aabb.min[a]).abs() < eps || (entry[a] - aabb.max[a]).abs() < eps
            });
            prop_assert!(on_surface, "entry {entry:?} is not on the box surface");
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
        let first = selection.selected();
        selection.click(Some(PLAYER_PAWN));

        assert_eq!(first, Some(PLAYER_PAWN));
        assert_eq!(
            selection.selected(),
            Some(PLAYER_PAWN),
            "clicking again must not toggle"
        );
    }
}
