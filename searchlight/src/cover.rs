//! Cover for an enemy under fire: choosing a free cover point, and checking that a tagged
//! point works as cover against the player right now.

use glam::Vec3;
use moho_physics::PhysicsWorld;

use crate::brain::{CoverOffer, EnemyTuning};

/// Where the player's eye is and the point on the player's body the enemies aim at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threat {
    pub eye: Vec3,
    pub aim: Vec3,
}

/// The nearest of `points` within `range` metres of `from` that is not in `taken` and that
/// `accepts`; ties go to the lower index.
pub fn pick(
    _points: &[Vec3],
    _taken: &[usize],
    _from: Vec3,
    _range: f32,
    _accepts: impl Fn(usize) -> bool,
) -> Option<CoverOffer> {
    None
}

/// The nearest free firing position among `points` for an enemy at `from`: a point the
/// player cannot see an enemy's body at, and from which an enemy whose eye is `enemy_eye`
/// metres above the point can shoot the player.
pub fn offer(
    _world: &PhysicsWorld,
    _points: &[Vec3],
    _taken: &[usize],
    _from: Vec3,
    _threat: Threat,
    _enemy_eye: f32,
    _tuning: &EnemyTuning,
) -> Option<CoverOffer> {
    None
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::test_support::{enemy_tuning, world_with};

    const FLOOR: (Vec3, Vec3) = (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0));

    fn at(x: f32) -> Vec3 {
        Vec3::new(x, 0.0, 0.0)
    }

    fn accept_all(_: usize) -> bool {
        true
    }

    fn offered(index: usize, position: Vec3) -> Option<CoverOffer> {
        Some(CoverOffer { index, position })
    }

    #[test]
    fn pick_takes_the_nearest_acceptable_free_point() {
        let points = [at(10.0), at(3.0), at(6.0), at(-4.0)];

        let picked = pick(&points, &[], Vec3::ZERO, 15.0, accept_all);

        assert_eq!(picked, offered(1, at(3.0)));
    }

    #[test]
    fn pick_measures_from_the_given_position() {
        let points = [at(0.0), at(9.0), at(20.0)];

        let picked = pick(&points, &[], at(12.0), 15.0, accept_all);

        assert_eq!(picked, offered(1, at(9.0)));
    }

    #[test]
    fn pick_skips_taken_rejected_and_out_of_range_points() {
        let points = [at(1.0), at(2.0), at(3.0), at(20.0), at(9.0)];

        let picked = pick(&points, &[0], Vec3::ZERO, 15.0, |index| {
            index != 1 && index != 2
        });

        assert_eq!(picked, offered(4, at(9.0)));
    }

    #[test]
    fn pick_gives_nothing_when_every_point_is_taken_rejected_or_out_of_range() {
        let points = [at(1.0), at(2.0), at(20.0)];

        let picked = pick(&points, &[0], Vec3::ZERO, 15.0, |index| index != 1);

        assert_eq!(picked, None);
        assert_eq!(pick(&[], &[], Vec3::ZERO, 15.0, accept_all), None);
    }

    #[test]
    fn pick_includes_a_point_exactly_at_the_range_and_excludes_one_beyond() {
        let on_the_edge = pick(&[at(5.0)], &[], Vec3::ZERO, 5.0, accept_all);
        let just_beyond = pick(&[at(5.01)], &[], Vec3::ZERO, 5.0, accept_all);

        assert_eq!(on_the_edge, offered(0, at(5.0)));
        assert_eq!(just_beyond, None);
    }

    #[test]
    fn pick_ties_go_to_the_lower_index() {
        let forward_first = [at(3.0), at(-3.0), at(3.0)];
        let backward_first = [at(-3.0), at(3.0)];

        let first = pick(&forward_first, &[], Vec3::ZERO, 15.0, accept_all);
        let second = pick(&backward_first, &[], Vec3::ZERO, 15.0, accept_all);
        let lower_rejected = pick(&forward_first, &[], Vec3::ZERO, 15.0, |index| index != 0);

        assert_eq!(first, offered(0, at(3.0)));
        assert_eq!(second, offered(0, at(-3.0)));
        assert_eq!(lower_rejected, offered(1, at(-3.0)));
    }

    proptest! {
        #[test]
        fn pick_never_returns_a_taken_rejected_or_out_of_range_point_and_returns_the_nearest(
            xs in prop::collection::vec(-40.0_f32..40.0, 0..12),
            taken in prop::collection::vec(0_usize..14, 0..6),
            accepted in prop::collection::vec(any::<bool>(), 12),
            from_x in -40.0_f32..40.0,
            range in 0.0_f32..50.0,
        ) {
            let points: Vec<Vec3> = xs.iter().map(|&x| at(x)).collect();
            let from = at(from_x);
            let distance = |index: usize| (points[index].x - from_x).abs();
            let valid = |index: usize| !taken.contains(&index) && accepted[index];

            let picked = pick(&points, &taken, from, range, |index| accepted[index]);

            let candidates: Vec<usize> = (0..points.len())
                .filter(|&index| valid(index) && distance(index) <= range - 1e-3)
                .collect();
            match picked {
                Some(offer) => {
                    prop_assert!(offer.index < points.len());
                    prop_assert!(!taken.contains(&offer.index), "{offer:?} is taken");
                    prop_assert!(accepted[offer.index], "{offer:?} is rejected");
                    prop_assert!(distance(offer.index) <= range + 1e-3, "{offer:?} is out of range");
                    prop_assert_eq!(offer.position, points[offer.index]);
                    for &other in &candidates {
                        prop_assert!(
                            distance(offer.index) <= distance(other) + 1e-3,
                            "{} is nearer than {offer:?}", other
                        );
                    }
                }
                None => prop_assert!(candidates.is_empty(), "{candidates:?} were available"),
            }
        }
    }

    const PLAYER: Threat = Threat {
        eye: Vec3::new(0.0, 1.8, 0.0),
        aim: Vec3::new(0.0, 1.15, 0.0),
    };
    const ENEMY_EYE: f32 = 1.5;
    const MARKER: Vec3 = Vec3::new(0.0, 0.0, 10.0);

    /// A wall 8 m from the player and 2 m short of the marker.
    fn wall_of_height(top: f32) -> (Vec3, Vec3) {
        (Vec3::new(-5.0, 0.0, 8.0), Vec3::new(5.0, top, 8.5))
    }

    fn offer_at_the_marker(
        boxes: &[(Vec3, Vec3)],
        taken: &[usize],
        enemy_eye: f32,
        tuning: &EnemyTuning,
    ) -> Option<CoverOffer> {
        let world = world_with(boxes);
        offer(
            &world,
            &[MARKER],
            taken,
            Vec3::new(0.0, 1.15, 12.0),
            PLAYER,
            enemy_eye,
            tuning,
        )
    }

    #[test]
    fn a_point_behind_a_low_wall_is_a_firing_position() {
        let boxes = [FLOOR, wall_of_height(1.2)];

        let offered_point = offer_at_the_marker(&boxes, &[], ENEMY_EYE, &enemy_tuning());

        assert_eq!(offered_point, offered(0, MARKER));
    }

    #[test]
    fn a_point_in_the_open_is_not_cover() {
        let open = [FLOOR];
        let shielded = [FLOOR, wall_of_height(1.2)];

        let in_the_open = offer_at_the_marker(&open, &[], ENEMY_EYE, &enemy_tuning());
        let behind_a_wall = offer_at_the_marker(&shielded, &[], ENEMY_EYE, &enemy_tuning());

        assert_eq!(in_the_open, None);
        assert!(
            behind_a_wall.is_some(),
            "the control point is not cover either"
        );
    }

    #[test]
    fn a_point_behind_a_full_wall_gives_no_shot() {
        let full = [FLOOR, wall_of_height(4.0)];
        let low = [FLOOR, wall_of_height(1.2)];

        let behind_full = offer_at_the_marker(&full, &[], ENEMY_EYE, &enemy_tuning());
        let behind_low = offer_at_the_marker(&low, &[], ENEMY_EYE, &enemy_tuning());

        assert_eq!(behind_full, None);
        assert!(
            behind_low.is_some(),
            "the control point is not cover either"
        );
    }

    #[test]
    fn a_point_whose_shooting_eye_sits_below_the_wall_top_gives_no_shot() {
        let boxes = [FLOOR, wall_of_height(1.2)];

        let eye_over_the_wall = offer_at_the_marker(&boxes, &[], 1.5, &enemy_tuning());
        let eye_under_the_wall = offer_at_the_marker(&boxes, &[], 0.6, &enemy_tuning());

        assert!(eye_over_the_wall.is_some());
        assert_eq!(eye_under_the_wall, None);
    }

    #[test]
    fn a_hide_height_above_the_wall_top_is_not_hidden() {
        let boxes = [FLOOR, wall_of_height(1.2)];
        let mut tall = enemy_tuning();
        tall.cover_hide_height = 1.6;

        let hidden = offer_at_the_marker(&boxes, &[], ENEMY_EYE, &enemy_tuning());
        let seen = offer_at_the_marker(&boxes, &[], ENEMY_EYE, &tall);

        assert!(hidden.is_some());
        assert_eq!(seen, None);
    }

    #[test]
    fn a_firing_position_another_enemy_holds_is_not_offered() {
        let boxes = [FLOOR, wall_of_height(1.2)];

        let free = offer_at_the_marker(&boxes, &[], ENEMY_EYE, &enemy_tuning());
        let held = offer_at_the_marker(&boxes, &[0], ENEMY_EYE, &enemy_tuning());

        assert!(free.is_some());
        assert_eq!(held, None);
    }

    #[test]
    fn a_firing_position_beyond_the_cover_range_is_not_offered() {
        let boxes = [FLOOR, wall_of_height(1.2)];
        let world = world_with(&boxes);
        let mut tuning = enemy_tuning();
        tuning.cover_range = 5.0;
        let from = |z: f32| Vec3::new(0.0, 0.0, z);

        let near = offer(
            &world,
            &[MARKER],
            &[],
            from(15.0),
            PLAYER,
            ENEMY_EYE,
            &tuning,
        );
        let far = offer(
            &world,
            &[MARKER],
            &[],
            from(15.5),
            PLAYER,
            ENEMY_EYE,
            &tuning,
        );

        assert_eq!(near, offered(0, MARKER));
        assert_eq!(far, None);
    }

    #[test]
    fn the_nearest_of_two_firing_positions_is_offered() {
        let boxes = [FLOOR, wall_of_height(1.2)];
        let world = world_with(&boxes);
        let points = [Vec3::new(4.0, 0.0, 10.0), Vec3::new(1.0, 0.0, 10.0)];

        let picked = offer(
            &world,
            &points,
            &[],
            Vec3::new(0.0, 1.15, 12.0),
            PLAYER,
            ENEMY_EYE,
            &enemy_tuning(),
        );

        assert_eq!(picked, offered(1, points[1]));
    }
}
