//! A player's round in flight: one step of its flight, cast against the world.

use glam::Vec3;
use moho_physics::{CharacterHandle, PhysicsWorld};
use searchlight_rules::{Ballistics, Flight};

use crate::shot::Strike;

/// A round that has left the muzzle and not yet struck anything or ended.
#[derive(Clone, Debug)]
pub struct RoundInFlight {
    /// Where the round is and how it moves.
    pub flight: Flight,
    /// The round's muzzle velocity and drag.
    pub ballistics: Ballistics,
    /// The shooter, whose own volumes never stop the round.
    pub shooter: Option<CharacterHandle>,
    /// Where the shooter stood when it fired.
    pub from: Vec3,
    /// Target numbers of the enemies this round has already counted as a near miss for.
    pub near_missed: Vec<u16>,
}

/// One step of a round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundStep {
    /// The flight one tick later, had nothing stopped it.
    pub flight: Flight,
    /// The strike point, or the new position when the round struck nothing.
    pub end: Vec3,
    /// The first surface the round reached on its way, if any.
    pub strike: Option<Strike>,
}

/// Steps `round` once by `dt` seconds under `gravity` (m/s²) and casts its path through `world`.
pub fn step_round(world: &PhysicsWorld, round: &RoundInFlight, gravity: f32, dt: f32) -> RoundStep {
    let _ = (world, round, gravity, dt);
    todo!("step the flight, then cast position to the new position through resolve_shot")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::SurfaceTag;
    use crate::test_support::{DT, world_with};

    const WALL_NEAR_FACE_Z: f32 = 10.0;

    fn round_at(origin: Vec3, velocity: Vec3) -> RoundInFlight {
        RoundInFlight {
            flight: Flight {
                position: origin,
                velocity,
            },
            ballistics: Ballistics {
                muzzle_velocity: 940.0,
                drag: 0.0,
            },
            shooter: None,
            from: origin,
            near_missed: Vec::new(),
        }
    }

    #[test]
    fn a_step_into_a_wall_ends_at_the_wall() {
        let wall = (
            Vec3::new(-5.0, 0.0, WALL_NEAR_FACE_Z),
            Vec3::new(5.0, 5.0, WALL_NEAR_FACE_Z + 1.0),
        );
        let clear = (Vec3::new(-5.0, 0.0, 40.0), Vec3::new(5.0, 5.0, 41.0));
        let round = round_at(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 940.0));

        let blocked = step_round(&world_with(&[wall]), &round, 0.0, DT);
        let open = step_round(&world_with(&[clear]), &round, 0.0, DT);

        let strike = blocked.strike.expect("the wall is in the way");
        assert_eq!(strike.tag, SurfaceTag::World);
        assert!(
            (strike.point.z - WALL_NEAR_FACE_Z).abs() < 1e-3,
            "{strike:?}"
        );
        assert_eq!(blocked.end, strike.point);
        assert!(
            (blocked.flight.position.z - 940.0 * DT).abs() < 1e-3,
            "the flight is the unobstructed step: {:?}",
            blocked.flight
        );
        assert_eq!(open.strike, None);
        assert_eq!(open.end, open.flight.position);
        assert!((open.end.z - 940.0 * DT).abs() < 1e-3, "{:?}", open.end);
    }
}
