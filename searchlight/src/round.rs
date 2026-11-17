//! A round in flight, stepped as one segment per tick and cast through the physics world.

use glam::Vec3;
use moho_physics::{CharacterHandle, PhysicsWorld};
use searchlight_rules::{Ballistics, Flight};

use crate::shot::{ShotRay, Strike, resolve_shot};

/// A round that has left its muzzle and not yet struck anything.
#[derive(Clone, Debug)]
pub struct RoundInFlight {
    /// Where it is and how fast it moves.
    pub flight: Flight,
    /// How it moves through the air.
    pub ballistics: Ballistics,
    /// Whose volumes never stop it.
    pub shooter: Option<CharacterHandle>,
    /// Where the shooter stood when firing.
    pub from: Vec3,
    /// Target numbers of the enemies it has already near-missed.
    pub near_missed: Vec<u16>,
}

/// One tick of a round's flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundStep {
    /// The round after the tick.
    pub flight: Flight,
    /// Where the tick's segment ends: the strike point, or the new position.
    pub end: Vec3,
    /// What the segment struck first, if anything.
    pub strike: Option<Strike>,
}

/// Steps `round` by `dt` seconds under `gravity` and casts the segment it flew through `world`.
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

    fn level_round(velocity: Vec3) -> RoundInFlight {
        RoundInFlight {
            flight: Flight {
                position: Vec3::new(0.0, 1.0, 0.0),
                velocity,
            },
            ballistics: Ballistics {
                muzzle_velocity: velocity.length(),
                drag: 0.0,
            },
            shooter: None,
            from: Vec3::ZERO,
            near_missed: Vec::new(),
        }
    }

    #[test]
    fn a_step_into_a_wall_ends_at_the_wall() {
        let world = world_with(&[(Vec3::new(-5.0, 0.0, 5.0), Vec3::new(5.0, 5.0, 6.0))]);
        let round = level_round(Vec3::new(0.0, 0.0, 600.0));

        let step = step_round(&world, &round, 0.0, DT);

        let strike = step.strike.expect("the segment crosses the wall");
        assert_eq!(strike.tag, SurfaceTag::World);
        assert!((strike.point.z - 5.0).abs() < 1e-3, "{strike:?}");
        assert_eq!(step.end, strike.point);
        assert!((step.flight.position.z - 10.0).abs() < 1e-3, "{step:?}");
    }

    #[test]
    fn a_step_through_open_air_ends_at_the_new_position() {
        let world = world_with(&[(Vec3::new(-5.0, 0.0, 50.0), Vec3::new(5.0, 5.0, 51.0))]);
        let round = level_round(Vec3::new(0.0, 0.0, 600.0));

        let step = step_round(&world, &round, 0.0, DT);

        assert_eq!(step.strike, None);
        assert_eq!(step.end, step.flight.position);
        assert!((step.end.z - 10.0).abs() < 1e-3, "{step:?}");
    }
}
