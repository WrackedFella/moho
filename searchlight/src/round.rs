//! A round in the air: one flight step cast against the world.

use glam::Vec3;
use moho_physics::{CharacterHandle, PhysicsWorld};
use searchlight_rules::{Ballistics, Flight};

use crate::shot::{ShotRay, Strike, resolve_shot};

/// A round flying: where it is, what it flies with, and who fired it.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundInFlight {
    pub flight: Flight,
    pub ballistics: Ballistics,
    /// The shooter, whose own volumes never stop the round.
    pub shooter: Option<CharacterHandle>,
    /// Where the shooter stood when firing.
    pub from: Vec3,
    /// Target numbers of the enemies this round has already counted as a near miss for.
    pub near_missed: Vec<u16>,
}

/// One step of a round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundStep {
    /// The flight after the step.
    pub flight: Flight,
    /// The strike point if the round struck anything, else the new position.
    pub end: Vec3,
    /// What the round struck on the way, if anything.
    pub strike: Option<Strike>,
}

/// Steps `round` once under `gravity` for `dt` seconds and casts the way it went.
pub fn step_round(world: &PhysicsWorld, round: &RoundInFlight, gravity: f32, dt: f32) -> RoundStep {
    let flight = round.flight.step(&round.ballistics, gravity, dt);
    let way = flight.position - round.flight.position;
    let strike = resolve_shot(
        world,
        ShotRay {
            origin: round.flight.position,
            direction: way,
            range: way.length(),
            ignore: round.shooter,
        },
    );
    RoundStep {
        flight,
        end: strike.map_or(flight.position, |strike| strike.point),
        strike,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{dummy_hit_volumes, player_hit_volumes};
    use crate::tag::SurfaceTag;
    use crate::test_support::{DT, world_with};
    use searchlight_rules::BodyPart;

    fn ball() -> Ballistics {
        Ballistics {
            muzzle_velocity: 940.0,
            drag: 1.0,
        }
    }

    fn flying(origin: Vec3, direction: Vec3) -> RoundInFlight {
        RoundInFlight {
            flight: Flight::fired(origin, direction, &ball()),
            ballistics: ball(),
            shooter: None,
            from: origin,
            near_missed: Vec::new(),
        }
    }

    fn wall_world() -> PhysicsWorld {
        world_with(&[
            (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 60.0)),
            (Vec3::new(-5.0, 0.0, 10.0), Vec3::new(5.0, 4.0, 11.0)),
        ])
    }

    #[test]
    fn a_step_into_a_wall_ends_at_the_wall() {
        let world = wall_world();
        let round = flying(Vec3::new(0.0, 1.5, 5.0), Vec3::Z);

        let step = step_round(&world, &round, 0.0, DT);

        let strike = step.strike.expect("the step crosses the wall");
        assert_eq!(strike.tag, SurfaceTag::World);
        assert!((strike.point.z - 10.0).abs() < 1e-3, "{strike:?}");
        assert!((strike.point.y - 1.5).abs() < 1e-3, "{strike:?}");
        assert_eq!(step.end, strike.point);
    }

    #[test]
    fn a_step_through_open_air_moves_the_flight_and_ends_at_its_new_position() {
        let world = wall_world();
        let round = flying(Vec3::new(0.0, 1.5, -20.0), Vec3::Z);

        let step = step_round(&world, &round, 9.81, DT);

        let expected = round.flight.step(&round.ballistics, 9.81, DT);
        assert_eq!(step.strike, None);
        assert_eq!(step.flight, expected);
        assert_eq!(step.end, expected.position);
        assert_ne!(step.flight, round.flight);
    }

    #[test]
    fn a_step_through_a_dummy_strikes_the_part_it_crosses() {
        let mut world = world_with(&[(Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 60.0))]);
        let dummy = world.add_character(Vec3::new(0.0, 0.915, 8.0));
        world
            .set_hit_volumes(dummy, &dummy_hit_volumes(0))
            .expect("the dummy was just added");
        world.step(DT);
        let round = flying(Vec3::new(0.0, 0.915 + 0.25, 2.0), Vec3::Z);

        let step = step_round(&world, &round, 0.0, DT);

        let strike = step.strike.expect("the step crosses the dummy");
        assert_eq!(
            strike.tag,
            SurfaceTag::Part {
                target: 0,
                part: BodyPart::Torso
            }
        );
        assert_eq!(step.end, strike.point);
    }

    #[test]
    fn a_round_never_strikes_its_shooters_own_body() {
        let mut world = world_with(&[(Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 60.0))]);
        let player = world.add_character(Vec3::new(0.0, 0.915, 0.0));
        world
            .set_hit_volumes(player, &player_hit_volumes())
            .expect("the player was just added");
        world.step(DT);
        let from_the_eye = Vec3::new(0.0, 1.6, 0.0);
        let own = RoundInFlight {
            shooter: Some(player),
            ..flying(from_the_eye, Vec3::NEG_Y)
        };
        let anyone_elses = flying(from_the_eye, Vec3::NEG_Y);

        let own_step = step_round(&world, &own, 0.0, DT);
        let other_step = step_round(&world, &anyone_elses, 0.0, DT);

        let strike = own_step.strike.expect("the round reaches the floor");
        assert_eq!(strike.tag, SurfaceTag::World, "{strike:?}");
        assert!(strike.point.y.abs() < 1e-3, "{strike:?}");
        let other = other_step.strike.expect("the round strikes the player");
        assert!(matches!(other.tag, SurfaceTag::Player(_)), "{other:?}");
    }

    #[test]
    fn a_step_that_stops_short_of_a_wall_does_not_strike_it() {
        let world = wall_world();
        let round = flying(Vec3::new(0.0, 1.5, 5.0), Vec3::Z);

        let step = step_round(&world, &round, 0.0, 0.001);

        assert_eq!(step.strike, None);
        assert!(step.end.z > 5.0 && step.end.z < 10.0, "{:?}", step.end);
    }
}
