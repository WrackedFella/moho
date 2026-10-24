//! Hitscan shot resolution against the physics world's tagged surfaces.

use glam::Vec3;
use moho_physics::{CharacterHandle, PhysicsWorld, RayQuery};

use crate::tag::SurfaceTag;

/// A shot leaving `origin` along `direction`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotRay {
    pub origin: Vec3,
    /// Need not be normalised.
    pub direction: Vec3,
    /// How far the shot reaches, in metres.
    pub range: f32,
    /// The shooter, whose own volumes never stop the shot.
    pub ignore: Option<CharacterHandle>,
}

/// The first surface a shot reached.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub tag: SurfaceTag,
    pub point: Vec3,
    /// Metres from the shot's origin.
    pub distance: f32,
}

/// The first tagged surface along `ray`, or `None` if nothing is within its range.
///
/// Reads the world as the last `step` left it. Hits on level geometry come back as
/// [`SurfaceTag::World`]; a surface whose engine tag is not a [`SurfaceTag`] counts as a miss.
pub fn resolve_shot(world: &PhysicsWorld, ray: ShotRay) -> Option<Strike> {
    let hit = world.cast_ray(&RayQuery {
        origin: ray.origin,
        direction: ray.direction,
        max_distance: ray.range,
        ignore: ray.ignore,
    })?;
    let tag = match SurfaceTag::try_from(hit.tag) {
        Ok(tag) => tag,
        Err(error) => {
            tracing::warn!(%error, "shot reached a surface with an unknown tag, counted as a miss");
            return None;
        }
    };
    Some(Strike {
        tag,
        point: hit.point,
        distance: hit.distance,
    })
}

/// Whether the segment from `origin` to `end` comes within `radius` of `point`.
pub fn passes_within(origin: Vec3, end: Vec3, point: Vec3, radius: f32) -> bool {
    let segment = end - origin;
    let length_squared = segment.length_squared();
    let along = if length_squared > 0.0 {
        ((point - origin).dot(segment) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    point.distance(origin + segment * along) <= radius
}

#[cfg(test)]
mod tests {
    use searchlight_rules::BodyPart;

    use super::*;
    use crate::sim::{dummy_hit_volumes, player_hit_volumes};
    use crate::test_support::{DT, world_with};

    const DUMMY_CENTRE: Vec3 = Vec3::new(0.0, 1.15, 8.0);

    fn add_dummy(world: &mut PhysicsWorld, centre: Vec3) -> CharacterHandle {
        let dummy = world.add_character(centre);
        world
            .set_hit_volumes(dummy, &dummy_hit_volumes(0))
            .expect("the dummy was just added");
        world.step(DT);
        dummy
    }

    fn part_centre(part: BodyPart) -> Vec3 {
        let tag = SurfaceTag::Part { target: 0, part };
        let volume = dummy_hit_volumes(0)
            .into_iter()
            .find(|v| v.tag == tag.into())
            .expect("the dummy has a volume for every part");
        DUMMY_CENTRE + volume.offset
    }

    fn straight_at(target: Vec3) -> ShotRay {
        ShotRay {
            origin: target - Vec3::Z * 6.0,
            direction: Vec3::Z,
            range: 50.0,
            ignore: None,
        }
    }

    #[test]
    fn a_shot_at_a_dummy_part_reports_that_part() {
        for part in [BodyPart::Head, BodyPart::Torso, BodyPart::LeftLeg] {
            let mut world = world_with(&[]);
            add_dummy(&mut world, DUMMY_CENTRE);

            let strike = resolve_shot(&world, straight_at(part_centre(part)));

            let strike = strike.unwrap_or_else(|| panic!("{part:?}: the shot hit nothing"));
            assert_eq!(strike.tag, SurfaceTag::Part { target: 0, part }, "{part:?}");
        }
    }

    fn add_player(world: &mut PhysicsWorld, centre: Vec3) -> CharacterHandle {
        let player = world.add_character(centre);
        world
            .set_hit_volumes(player, &player_hit_volumes())
            .expect("the player was just added");
        world.step(DT);
        player
    }

    fn player_part_centre(part: BodyPart) -> Vec3 {
        let volume = player_hit_volumes()
            .into_iter()
            .find(|v| v.tag == SurfaceTag::Player(part).into())
            .expect("the player has a volume for every part");
        DUMMY_CENTRE + volume.offset
    }

    #[test]
    fn a_shot_at_the_player_reports_the_part() {
        for part in [BodyPart::Head, BodyPart::Torso, BodyPart::LeftLeg] {
            let mut world = world_with(&[]);
            add_player(&mut world, DUMMY_CENTRE);

            let strike = resolve_shot(&world, straight_at(player_part_centre(part)));

            let strike = strike.unwrap_or_else(|| panic!("{part:?}: the shot hit nothing"));
            assert_eq!(strike.tag, SurfaceTag::Player(part), "{part:?}");
        }
    }

    #[test]
    fn a_shot_ignores_the_shooting_player() {
        let aim = player_part_centre(BodyPart::Torso);
        let mut world = world_with(&[]);
        let player = add_player(&mut world, DUMMY_CENTRE);
        let mut ray = straight_at(aim);

        let blocked = resolve_shot(&world, ray).expect("the player is hit");
        ray.ignore = Some(player);
        let passed = resolve_shot(&world, ray);

        assert_eq!(blocked.tag, SurfaceTag::Player(BodyPart::Torso));
        assert_eq!(passed, None);
    }

    #[test]
    fn a_shot_at_a_wall_reports_no_part() {
        let world = world_with(&[(Vec3::new(-5.0, 0.0, 6.0), Vec3::new(5.0, 4.0, 6.5))]);
        let ray = ShotRay {
            origin: Vec3::new(0.0, 1.5, 0.0),
            direction: Vec3::Z,
            range: 50.0,
            ignore: None,
        };

        let strike = resolve_shot(&world, ray).expect("the wall stops the shot");

        assert_eq!(strike.tag, SurfaceTag::World);
        assert!((strike.distance - 6.0).abs() < 1e-3, "{strike:?}");
        assert!(
            (strike.point - Vec3::new(0.0, 1.5, 6.0)).length() < 1e-3,
            "{strike:?}"
        );
    }

    #[test]
    fn cover_in_front_of_a_dummy_takes_the_shot() {
        let aim = part_centre(BodyPart::Torso);
        let mut open = world_with(&[]);
        add_dummy(&mut open, DUMMY_CENTRE);
        let mut covered = world_with(&[(Vec3::new(-3.0, 0.0, 4.0), Vec3::new(3.0, 3.0, 4.2))]);
        add_dummy(&mut covered, DUMMY_CENTRE);

        let without_cover = resolve_shot(&open, straight_at(aim)).expect("the dummy is hit");
        let with_cover = resolve_shot(&covered, straight_at(aim)).expect("the cover is hit");

        assert!(matches!(without_cover.tag, SurfaceTag::Part { .. }));
        assert_eq!(with_cover.tag, SurfaceTag::World);
        assert!(
            (with_cover.distance - (4.0 - (aim.z - 6.0))).abs() < 1e-3,
            "{with_cover:?}"
        );
    }

    #[test]
    fn a_shot_ignores_the_shooter() {
        let aim = part_centre(BodyPart::Torso);
        let mut world = world_with(&[(Vec3::new(-5.0, 0.0, 12.0), Vec3::new(5.0, 4.0, 12.5))]);
        let dummy = add_dummy(&mut world, DUMMY_CENTRE);
        let mut ray = straight_at(aim);

        let blocked = resolve_shot(&world, ray).expect("the dummy is hit");
        ray.ignore = Some(dummy);
        let passed = resolve_shot(&world, ray).expect("the wall behind is hit");

        assert!(matches!(blocked.tag, SurfaceTag::Part { .. }));
        assert_eq!(passed.tag, SurfaceTag::World);
    }

    #[test]
    fn a_shot_reaches_exactly_as_far_as_its_range() {
        let world = world_with(&[(Vec3::new(-5.0, 0.0, 5.0), Vec3::new(5.0, 4.0, 5.5))]);
        let at_range = |range| {
            resolve_shot(
                &world,
                ShotRay {
                    origin: Vec3::new(0.0, 1.5, 0.0),
                    direction: Vec3::Z,
                    range,
                    ignore: None,
                },
            )
        };

        let short = at_range(4.9);
        let long = at_range(5.1);

        assert_eq!(short, None);
        assert!(long.is_some());
    }

    #[test]
    fn passes_within_measures_to_the_closest_point_of_the_segment() {
        let origin = Vec3::ZERO;
        let end = Vec3::new(10.0, 0.0, 0.0);
        let cases = [
            ("beside the middle, inside", Vec3::new(5.0, 0.9, 0.0), true),
            (
                "beside the middle, outside",
                Vec3::new(5.0, 1.1, 0.0),
                false,
            ),
            ("beside the middle, across", Vec3::new(5.0, 0.0, -0.9), true),
            ("exactly at the radius", Vec3::new(5.0, 1.0, 0.0), true),
            ("beyond the end, near it", Vec3::new(10.8, 0.5, 0.0), true),
            (
                "beyond the end, off the line",
                Vec3::new(11.2, 0.0, 0.0),
                false,
            ),
            (
                "far beyond the end, on the line",
                Vec3::new(12.0, 0.0, 0.0),
                false,
            ),
            (
                "behind the origin, near it",
                Vec3::new(-0.8, 0.5, 0.0),
                true,
            ),
            (
                "behind the origin, off the line",
                Vec3::new(-2.0, 0.0, 0.0),
                false,
            ),
        ];

        for (name, point, expected) in cases {
            assert_eq!(passes_within(origin, end, point, 1.0), expected, "{name}");
        }
    }

    #[test]
    fn passes_within_uses_the_closest_point_of_a_slanted_segment() {
        let origin = Vec3::ZERO;
        let end = Vec3::new(10.0, 10.0, 0.0);
        let point = Vec3::new(10.0, 0.0, 0.0);

        let inside = passes_within(origin, end, point, 7.1);
        let outside = passes_within(origin, end, point, 7.0);

        assert!(inside, "the point is 7.07 m from the segment");
        assert!(!outside);
    }

    #[test]
    fn passes_within_a_zero_length_segment_is_a_distance_to_a_point() {
        let at = Vec3::new(3.0, 1.0, 2.0);

        let near = passes_within(at, at, at + Vec3::new(0.0, 0.9, 0.0), 1.0);
        let far = passes_within(at, at, at + Vec3::new(0.0, 1.1, 0.0), 1.0);

        assert!(near);
        assert!(!far);
    }

    #[test]
    fn a_shot_into_the_open_hits_nothing() {
        let world = world_with(&[]);
        let ray = ShotRay {
            origin: Vec3::new(0.0, 1.5, 0.0),
            direction: Vec3::Y,
            range: 50.0,
            ignore: None,
        };

        assert_eq!(resolve_shot(&world, ray), None);
    }
}
