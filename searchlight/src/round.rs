//! A round in flight and one step of it against the physics world.

use glam::Vec3;
use moho_physics::{CharacterHandle, PhysicsWorld};
use searchlight_rules::{Ballistics, Flight};

use crate::shot::Strike;

/// A fired round on its way.
#[derive(Clone, Debug)]
pub struct RoundInFlight {
    pub flight: Flight,
    pub ballistics: Ballistics,
    /// Whose volumes never stop the round.
    pub shooter: Option<CharacterHandle>,
    /// Where the shooter stood when firing.
    pub from: Vec3,
    /// Target numbers of the enemies already counted as near misses.
    pub near_missed: Vec<u16>,
}

/// What one step of a round did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundStep {
    /// The round after the step.
    pub flight: Flight,
    /// The strike point, or the new position when nothing was struck.
    pub end: Vec3,
    /// The first surface reached between the old and new position.
    pub strike: Option<Strike>,
}

/// Steps `round` once by `dt` seconds under `gravity` and casts its path through `world`.
#[must_use]
pub fn step_round(
    _world: &PhysicsWorld,
    round: &RoundInFlight,
    _gravity: f32,
    _dt: f32,
) -> RoundStep {
    RoundStep {
        flight: round.flight,
        end: round.flight.position,
        strike: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::SurfaceTag;
    use crate::test_support::{DT, world_with};

    #[test]
    fn a_step_into_a_wall_ends_at_the_wall() {
        let wall = (Vec3::new(-5.0, 0.0, 10.0), Vec3::new(5.0, 5.0, 11.0));
        let world = world_with(&[wall]);
        let ballistics = Ballistics {
            muzzle_velocity: 940.0,
            drag: 0.0,
        };
        let round = RoundInFlight {
            flight: Flight::fired(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, &ballistics),
            ballistics,
            shooter: None,
            from: Vec3::ZERO,
            near_missed: Vec::new(),
        };

        let step = step_round(&world, &round, 0.0, DT);

        let strike = step.strike.expect("the step crosses the wall");
        assert_eq!(strike.tag, SurfaceTag::World);
        assert!((strike.point.z - 10.0).abs() < 1e-2, "{strike:?}");
        assert_eq!(step.end, strike.point);
        assert!(
            step.flight.position.z > 11.0,
            "the stepped flight is the unobstructed one: {:?}",
            step.flight
        );
    }
}
