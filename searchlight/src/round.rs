//! A round in flight: one stepped segment at a time, cast through the world.

use glam::Vec3;
use moho_physics::{CharacterHandle, PhysicsWorld};
use searchlight_rules::{Ballistics, Flight};

use crate::shot::{ShotRay, Strike, resolve_shot};

/// A round in the air and what it needs to resolve its hits.
#[derive(Clone, Debug)]
pub struct RoundInFlight {
    pub flight: Flight,
    pub ballistics: Ballistics,
    /// The character whose own volumes never stop the round.
    pub shooter: Option<CharacterHandle>,
    /// Where the shooter stood when firing.
    pub from: Vec3,
    /// The enemies (target numbers) this round has already near-missed.
    pub near_missed: Vec<u16>,
}

/// One step of a round: where it flew to and what it struck on the way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundStep {
    /// The flight after the step.
    pub flight: Flight,
    /// The strike point, or the new position when nothing was struck.
    pub end: Vec3,
    /// The first surface the segment from the old to the new position reached.
    pub strike: Option<Strike>,
}

/// Steps `round` once under `gravity` (metres per second squared, downward) for `dt` seconds
/// and casts the segment it flew through `world`.
pub fn step_round(world: &PhysicsWorld, round: &RoundInFlight, gravity: f32, dt: f32) -> RoundStep {
    let flight = round.flight.step(&round.ballistics, gravity, dt);
    let segment = flight.position - round.flight.position;
    let strike = resolve_shot(
        world,
        ShotRay {
            origin: round.flight.position,
            direction: segment,
            range: segment.length(),
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
    use crate::tag::SurfaceTag;
    use crate::test_support::{DT, world_with};

    fn round_towards_z(origin: Vec3, speed: f32) -> RoundInFlight {
        RoundInFlight {
            flight: Flight {
                position: origin,
                velocity: Vec3::new(0.0, 0.0, speed),
            },
            ballistics: Ballistics {
                muzzle_velocity: speed,
                drag: 0.0,
            },
            shooter: None,
            from: origin,
            near_missed: Vec::new(),
        }
    }

    #[test]
    fn a_step_into_a_wall_ends_at_the_wall() {
        let floor = (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0));
        let wall = (Vec3::new(-5.0, 0.0, 10.0), Vec3::new(5.0, 4.0, 10.5));
        let world = world_with(&[floor, wall]);
        let round = round_towards_z(Vec3::new(0.0, 1.5, 0.0), 900.0);

        let step = step_round(&world, &round, 0.0, DT);

        let strike = step.strike.expect("the segment crosses the wall");
        assert_eq!(strike.tag, SurfaceTag::World);
        assert!(
            (strike.point.z - 10.0).abs() < 1e-3,
            "struck at {:?}",
            strike.point
        );
        assert_eq!(step.end, strike.point);
        assert!(
            (step.flight.position.z - 15.0).abs() < 1e-2,
            "the flight carries on to the unobstructed position: {:?}",
            step.flight.position
        );
    }

    #[test]
    fn a_step_through_open_air_ends_at_the_new_position() {
        let floor = (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0));
        let world = world_with(&[floor]);
        let round = round_towards_z(Vec3::new(0.0, 1.5, 0.0), 900.0);

        let step = step_round(&world, &round, 0.0, DT);

        assert_eq!(step.strike, None);
        assert_eq!(step.end, step.flight.position);
        assert!((step.end.z - 15.0).abs() < 1e-2, "moved to {:?}", step.end);
    }
}
