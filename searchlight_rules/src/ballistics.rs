//! A round in flight: stepped under gravity and drag, platform-free.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Standard gravity, m/s squared.
pub const STANDARD_GRAVITY: f32 = 9.81;

/// The largest gravity scale a tester may set.
pub const MAX_GRAVITY_SCALE: f32 = 1000.0;

/// A flight at or below this speed, m/s, is spent.
pub const MIN_SPEED: f32 = 100.0;

/// What a round definition says about its flight.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ballistics {
    /// Speed at the muzzle, m/s.
    pub muzzle_velocity: f32,
    /// Speed lost to drag, 1/s.
    pub drag: f32,
}

/// Where a round is and how fast it moves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// Position, m.
    pub position: Vec3,
    /// Velocity, m/s.
    pub velocity: Vec3,
}

impl Flight {
    /// A round leaving `origin` along `direction` (any length) at the muzzle velocity.
    #[must_use]
    pub fn fired(origin: Vec3, _direction: Vec3, _ballistics: &Ballistics) -> Self {
        Self {
            position: origin,
            velocity: Vec3::ZERO,
        }
    }

    /// One step of `dt` seconds under `gravity` (m/s squared, downwards) and the round's drag.
    #[must_use]
    pub fn step(&self, _ballistics: &Ballistics, _gravity: f32, _dt: f32) -> Self {
        *self
    }

    /// Whether the round has slowed to [`MIN_SPEED`] or below.
    #[must_use]
    pub fn is_spent(&self) -> bool {
        false
    }
}

/// A tester's multiplier on gravity for rounds in flight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GravityScale(f32);

/// Why a gravity scale was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[error("gravity scale {0} must be from 0 to 1000")]
pub struct GravityScaleError(pub f32);

impl GravityScale {
    /// A gravity scale of `scale`.
    ///
    /// # Errors
    ///
    /// [`GravityScaleError`] when `scale` is negative, not finite or above
    /// [`MAX_GRAVITY_SCALE`].
    pub fn new(scale: f32) -> Result<Self, GravityScaleError> {
        Ok(Self(scale))
    }

    /// The gravity this scale gives, m/s squared.
    #[must_use]
    pub fn gravity(self) -> f32 {
        0.0
    }
}

impl Default for GravityScale {
    fn default() -> Self {
        Self(0.0)
    }
}

/// The box a round may fly in; one that leaves it is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightBounds {
    /// Lowest corner.
    pub min: Vec3,
    /// Highest corner.
    pub max: Vec3,
}

impl FlightBounds {
    /// The box around `points`, grown by `margin` on every side.
    #[must_use]
    pub fn around(_points: &[Vec3], _margin: f32) -> Self {
        Self {
            min: Vec3::ZERO,
            max: Vec3::ZERO,
        }
    }

    /// Whether `point` is inside the box or on its surface.
    #[must_use]
    pub fn contains(&self, _point: Vec3) -> bool {
        false
    }

    /// The box diagonal: farther than any straight line inside it.
    #[must_use]
    pub fn reach(&self) -> f32 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const DT: f32 = 1.0 / 60.0;

    fn rifle_ball() -> Ballistics {
        Ballistics {
            muzzle_velocity: 940.0,
            drag: 0.0,
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn a_step_without_gravity_or_drag_moves_at_the_muzzle_velocity() {
        let ballistics = rifle_ball();
        let flight = Flight::fired(Vec3::new(1.0, 2.0, 3.0), Vec3::X, &ballistics);

        let next = flight.step(&ballistics, 0.0, DT);

        assert!(near(next.velocity.x, 940.0), "{next:?}");
        assert!(near(next.velocity.y, 0.0), "{next:?}");
        assert!(near(next.velocity.z, 0.0), "{next:?}");
        assert!(near(next.position.x, 1.0 + 940.0 * DT), "{next:?}");
        assert!(near(next.position.y, 2.0), "{next:?}");
        assert!(near(next.position.z, 3.0), "{next:?}");
    }

    #[test]
    fn gravity_changes_only_the_vertical_velocity() {
        let ballistics = rifle_ball();
        let direction = Vec3::new(1.0, 0.0, 1.0);
        let flight = Flight::fired(Vec3::ZERO, direction, &ballistics);

        let next = flight.step(&ballistics, STANDARD_GRAVITY, DT);

        assert_eq!(next.velocity.x, flight.velocity.x);
        assert_eq!(next.velocity.z, flight.velocity.z);
        assert!(near(next.velocity.x, 940.0 / 2.0_f32.sqrt()), "{next:?}");
        assert!(near(next.velocity.y, -STANDARD_GRAVITY * DT), "{next:?}");
    }

    #[test]
    fn drag_past_one_step_stops_the_round_and_never_reverses_it() {
        let ballistics = Ballistics {
            muzzle_velocity: 940.0,
            drag: 100.0,
        };
        let flight = Flight {
            position: Vec3::new(5.0, 0.0, 0.0),
            velocity: Vec3::new(940.0, 0.0, 0.0),
        };

        let next = flight.step(&ballistics, 0.0, DT);
        let after = next.step(&ballistics, 0.0, DT);

        assert_eq!(next.velocity, Vec3::ZERO);
        assert_eq!(next.position, flight.position);
        assert_eq!(after.velocity, Vec3::ZERO);
    }

    #[test]
    fn drag_slows_the_round_by_the_fraction_of_a_step() {
        let ballistics = Ballistics {
            muzzle_velocity: 940.0,
            drag: 6.0,
        };
        let flight = Flight::fired(Vec3::ZERO, Vec3::X, &ballistics);

        let next = flight.step(&ballistics, 0.0, DT);

        assert!(near(next.velocity.x, 940.0 * (1.0 - 6.0 * DT)), "{next:?}");
        assert!(
            near(next.position.x, 940.0 * (1.0 - 6.0 * DT) * DT),
            "{next:?}"
        );
    }

    proptest! {
        #[test]
        fn without_gravity_speed_never_grows(
            speed in 100.0f32..2000.0,
            drag in 0.0f32..200.0,
            x in -1.0f32..1.0,
            y in -1.0f32..1.0,
            z in -1.0f32..1.0,
            steps in 1usize..40,
        ) {
            prop_assume!(Vec3::new(x, y, z).length() > 0.1);
            let ballistics = Ballistics { muzzle_velocity: speed, drag };
            let mut flight = Flight {
                position: Vec3::ZERO,
                velocity: Vec3::new(x, y, z).normalize() * speed,
            };

            for _ in 0..steps {
                let before = flight.velocity.length();
                flight = flight.step(&ballistics, 0.0, DT);
                prop_assert!(flight.velocity.length() <= before + 1e-3);
            }
            if drag >= 1.0 {
                prop_assert!(flight.velocity.length() < speed);
            }
        }

        #[test]
        fn a_larger_gravity_scale_drops_a_round_further(
            low in 0.0f32..500.0,
            extra in 1.0f32..500.0,
            drag in 0.0f32..5.0,
        ) {
            let ballistics = Ballistics { muzzle_velocity: 940.0, drag };
            let gravity = |scale: f32| GravityScale::new(scale).expect("in range").gravity();
            let mut slight = Flight::fired(Vec3::ZERO, Vec3::X, &ballistics);
            let mut heavy = slight;

            for _ in 0..30 {
                slight = slight.step(&ballistics, gravity(low), DT);
                heavy = heavy.step(&ballistics, gravity(low + extra), DT);
            }

            prop_assert!(heavy.position.y < slight.position.y);
        }
    }

    #[test]
    fn a_flight_at_the_minimum_speed_is_spent() {
        let at = |speed: f32| Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(0.0, speed, 0.0),
        };

        assert!(at(MIN_SPEED).is_spent());
        assert!(at(MIN_SPEED - 1.0).is_spent());
        assert!(at(0.0).is_spent());
        assert!(!at(MIN_SPEED + 1.0).is_spent());
        assert_eq!(MIN_SPEED, 100.0);
    }

    #[test]
    fn gravity_scale_accepts_0_and_refuses_negative_and_non_finite() {
        let none = GravityScale::new(0.0).expect("0 turns gravity off for rounds");

        assert_eq!(none.gravity(), 0.0);
        assert!(GravityScale::new(30.0).is_ok());
        assert!(GravityScale::new(-0.5).is_err());
        assert!(GravityScale::new(f32::NAN).is_err());
        assert!(GravityScale::new(f32::INFINITY).is_err());
        assert!(GravityScale::new(f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn gravity_scale_refuses_more_than_1000() {
        assert!(GravityScale::new(MAX_GRAVITY_SCALE).is_ok());
        assert!(GravityScale::new(1000.5).is_err());
        assert_eq!(MAX_GRAVITY_SCALE, 1000.0);
    }

    #[test]
    fn gravity_scale_defaults_to_1() {
        let default = GravityScale::default();

        assert_eq!(default, GravityScale::new(1.0).expect("1 is in range"));
        assert_eq!(default.gravity(), STANDARD_GRAVITY);
    }

    #[test]
    fn round_gravity_is_standard_gravity_times_the_scale() {
        let one = GravityScale::new(1.0).expect("in range");
        let thirty = GravityScale::new(30.0).expect("in range");

        assert_eq!(STANDARD_GRAVITY, 9.81);
        assert_eq!(one.gravity(), 9.81);
        assert_eq!(thirty.gravity(), STANDARD_GRAVITY * 30.0);
    }

    #[test]
    fn flight_bounds_contain_their_points_grown_by_the_margin() {
        let points = [Vec3::ZERO, Vec3::new(10.0, 5.0, -20.0)];

        let bounds = FlightBounds::around(&points, 3.0);

        assert_eq!(bounds.min, Vec3::new(-3.0, -3.0, -23.0));
        assert_eq!(bounds.max, Vec3::new(13.0, 8.0, 3.0));
        assert!(points.iter().all(|p| bounds.contains(*p)));
        assert!(
            bounds.contains(Vec3::new(13.0, 8.0, 3.0)),
            "a point on a face"
        );
        assert!(
            bounds.contains(Vec3::new(-3.0, -3.0, -23.0)),
            "a point on a face"
        );
        assert!(!bounds.contains(Vec3::new(13.01, 0.0, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, -3.01, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, 0.0, 3.01)));
    }

    #[test]
    fn reach_is_the_box_diagonal() {
        let flat = FlightBounds::around(&[Vec3::ZERO, Vec3::new(3.0, 4.0, 12.0)], 0.0);
        let grown = FlightBounds::around(&[Vec3::ZERO], 1.0);

        assert!(near(flat.reach(), 13.0), "{}", flat.reach());
        assert!(near(grown.reach(), 12.0_f32.sqrt()), "{}", grown.reach());
    }

    #[test]
    fn a_fired_flight_has_the_muzzle_speed_along_a_non_unit_direction() {
        let origin = Vec3::new(1.0, 2.0, 3.0);

        let flight = Flight::fired(origin, Vec3::new(3.0, 0.0, 4.0), &rifle_ball());

        assert_eq!(flight.position, origin);
        assert!(near(flight.velocity.x, 940.0 * 0.6), "{flight:?}");
        assert!(near(flight.velocity.y, 0.0), "{flight:?}");
        assert!(near(flight.velocity.z, 940.0 * 0.8), "{flight:?}");
    }
}
