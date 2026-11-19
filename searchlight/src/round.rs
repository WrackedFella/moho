//! Rounds in flight: one fixed step of a round, cast through the world as a segment.

use glam::Vec3;
use moho_physics::{CharacterHandle, PhysicsWorld};
use searchlight_rules::{Ballistics, Flight};

use crate::shot::Strike;

/// A round that has left the muzzle and not yet struck anything.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundInFlight {
    /// Where the round is and how it moves.
    pub flight: Flight,
    /// What its flight depends on.
    pub ballistics: Ballistics,
    /// The shooter, whose own volumes never stop the round.
    pub shooter: Option<CharacterHandle>,
    /// Where the shooter stood when firing.
    pub from: Vec3,
    /// Targets this round has already passed close enough to count as a near miss.
    pub near_missed: Vec<u16>,
}

/// What one step of a round did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundStep {
    /// The round after the step, whether or not it struck something on the way.
    pub flight: Flight,
    /// The strike point, or the new position when nothing was struck.
    pub end: Vec3,
    /// The first surface the step's segment reached.
    pub strike: Option<Strike>,
}

/// Steps `round` once by `dt` seconds under `gravity` and casts the segment from where it was
/// to where it is.
pub fn step_round(world: &PhysicsWorld, round: &RoundInFlight, gravity: f32, dt: f32) -> RoundStep {
    let _ = (world, round, gravity, dt);
    todo!("step the flight, then resolve the segment as a shot")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::SurfaceTag;
    use crate::test_support::{DT, world_with};

    const BALL: Ballistics = Ballistics {
        muzzle_velocity: 940.0,
        drag: 0.0,
    };

    fn level_round(from: Vec3) -> RoundInFlight {
        RoundInFlight {
            flight: Flight::fired(from, Vec3::Z, &BALL),
            ballistics: BALL,
            shooter: None,
            from,
            near_missed: Vec::new(),
        }
    }

    #[test]
    fn a_step_into_a_wall_ends_at_the_wall() {
        let wall = (Vec3::new(-5.0, 0.0, 5.0), Vec3::new(5.0, 4.0, 6.0));
        let world = world_with(&[wall]);
        let round = level_round(Vec3::new(0.0, 1.5, 0.0));

        let step = step_round(&world, &round, 0.0, DT);

        let strike = step.strike.expect("the segment crosses the wall");
        assert_eq!(strike.tag, SurfaceTag::World);
        assert!((strike.point.z - 5.0).abs() < 1e-3, "{strike:?}");
        assert!((strike.distance - 5.0).abs() < 1e-3, "{strike:?}");
        assert_eq!(step.end, strike.point);
        assert_eq!(step.flight, round.flight.step(&BALL, 0.0, DT));
        assert!(step.flight.position.z > 6.0, "{:?}", step.flight);
    }

    #[test]
    fn a_step_in_the_open_ends_at_the_new_position() {
        let wall = (Vec3::new(-5.0, 0.0, 50.0), Vec3::new(5.0, 4.0, 51.0));
        let world = world_with(&[wall]);
        let round = level_round(Vec3::new(0.0, 1.5, 0.0));

        let step = step_round(&world, &round, 0.0, DT);

        assert_eq!(step.strike, None);
        assert_eq!(step.end, step.flight.position);
        assert!((step.end.z - 940.0 * DT).abs() < 1e-3, "{:?}", step.end);
    }

    #[test]
    fn a_step_under_gravity_casts_along_the_dropped_segment() {
        let floor = (Vec3::new(-5.0, -1.0, -5.0), Vec3::new(5.0, 0.0, 60.0));
        let world = world_with(&[floor]);
        let round = RoundInFlight {
            flight: Flight {
                position: Vec3::new(0.0, 0.5, 0.0),
                velocity: Vec3::new(0.0, 0.0, 100.0),
            },
            ballistics: Ballistics {
                muzzle_velocity: 100.0,
                drag: 0.0,
            },
            shooter: None,
            from: Vec3::ZERO,
            near_missed: Vec::new(),
        };

        let step = step_round(&world, &round, 1000.0, 0.1);

        let strike = step.strike.expect("the round falls onto the floor");
        assert_eq!(strike.tag, SurfaceTag::World);
        assert!(strike.point.y.abs() < 1e-3, "{strike:?}");
        assert!(strike.point.z > 0.0 && strike.point.z < 10.0, "{strike:?}");
        assert_eq!(step.end, strike.point);
    }
}
