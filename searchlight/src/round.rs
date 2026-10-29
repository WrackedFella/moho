//! A player's round in flight: one step of its flight, cast as a segment through the same
//! path a hitscan shot takes.

use glam::Vec3;
use moho_physics::{CharacterHandle, PhysicsWorld};
use searchlight_rules::{Ballistics, Flight};

use crate::shot::{ShotRay, Strike, resolve_shot};

/// A round the sim is stepping.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundInFlight {
    pub flight: Flight,
    pub ballistics: Ballistics,
    /// Whose own volumes never stop the round.
    pub shooter: Option<CharacterHandle>,
    /// Where the shooter stood when firing.
    pub from: Vec3,
    /// Target numbers of the enemies this round has already passed close to.
    pub near_missed: Vec<u16>,
}

/// What one step of a round came to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundStep {
    /// The flight after the step, as if nothing had stopped it.
    pub flight: Flight,
    /// Where the step ended: the strike point, or the new position.
    pub end: Vec3,
    /// The first surface the step's segment reached.
    pub strike: Option<Strike>,
}

/// Steps `round` once under `gravity` for `dt` seconds and casts the segment from where it was
/// to where it got.
#[must_use]
pub fn step_round(world: &PhysicsWorld, round: &RoundInFlight, gravity: f32, dt: f32) -> RoundStep {
    let flight = round.flight.step(&round.ballistics, gravity, dt);
    let segment = flight.position - round.flight.position;
    let range = segment.length();
    let strike = if range > 0.0 {
        resolve_shot(
            world,
            ShotRay {
                origin: round.flight.position,
                direction: segment,
                range,
                ignore: round.shooter,
            },
        )
    } else {
        None
    };
    RoundStep {
        flight,
        end: strike.map_or(flight.position, |strike| strike.point),
        strike,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::SurfaceTag;
    use crate::test_support::{DT, world_with};

    const BALL: Ballistics = Ballistics {
        muzzle_velocity: 940.0,
        drag: 1.0,
    };

    fn round_at(position: Vec3, direction: Vec3) -> RoundInFlight {
        RoundInFlight {
            flight: Flight::fired(position, direction, &BALL),
            ballistics: BALL,
            shooter: None,
            from: position,
            near_missed: Vec::new(),
        }
    }

    #[test]
    fn a_step_into_a_wall_ends_at_the_wall() {
        let world = world_with(&[(Vec3::new(-5.0, 0.0, 10.0), Vec3::new(5.0, 5.0, 11.0))]);
        let round = round_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Z);

        let step = step_round(&world, &round, 9.81, DT);

        let strike = step.strike.expect("the wall is inside the step");
        assert_eq!(strike.tag, SurfaceTag::World);
        assert!((strike.point.z - 10.0).abs() < 1e-3, "{strike:?}");
        assert_eq!(step.end, strike.point);
        assert_eq!(step.flight, round.flight.step(&BALL, 9.81, DT));
        assert!(step.flight.position.z > 15.0, "{:?}", step.flight);
    }

    #[test]
    fn a_step_through_open_air_ends_at_the_new_position() {
        let world = world_with(&[(Vec3::new(-5.0, 0.0, 30.0), Vec3::new(5.0, 5.0, 31.0))]);
        let round = round_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Z);

        let step = step_round(&world, &round, 9.81, DT);

        assert_eq!(step.strike, None);
        assert_eq!(step.flight, round.flight.step(&BALL, 9.81, DT));
        assert_eq!(step.end, step.flight.position);
    }
}
