//! Round flight: a stepped integrator under gravity and drag, and the box a round may fly in.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Standard gravity, in m/s².
pub const STANDARD_GRAVITY: f32 = 9.81;

/// The largest gravity scale the tester may ask for.
pub const MAX_GRAVITY_SCALE: f32 = 1000.0;

/// The speed, in m/s, at or below which a flight is spent.
pub const MIN_SPEED: f32 = 100.0;

/// What a round's flight depends on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ballistics {
    /// Speed at the muzzle, in m/s.
    pub muzzle_velocity: f32,
    /// Drag, in 1/s.
    pub drag: f32,
}

/// A round in the air: where it is and how it moves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// Position, in metres.
    pub position: Vec3,
    /// Velocity, in m/s.
    pub velocity: Vec3,
}

impl Flight {
    /// A round leaving `origin` along `direction` (any length) at the muzzle velocity.
    pub fn fired(origin: Vec3, direction: Vec3, ballistics: &Ballistics) -> Self {
        todo!("{origin} {direction} {ballistics:?}")
    }

    /// The flight one step of `dt` seconds later, under `gravity` in m/s².
    pub fn step(&self, ballistics: &Ballistics, gravity: f32, dt: f32) -> Self {
        todo!("{ballistics:?} {gravity} {dt}")
    }

    /// Whether the round has slowed to [`MIN_SPEED`] or below.
    pub fn is_spent(&self) -> bool {
        false
    }
}

/// A multiplier on standard gravity for rounds only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GravityScale(f32);

/// Why a gravity scale was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[non_exhaustive]
pub enum GravityScaleError {
    /// Negative, not finite, or above [`MAX_GRAVITY_SCALE`].
    #[error("a gravity scale is from 0 to {MAX_GRAVITY_SCALE}, got {value}")]
    OutOfRange {
        /// The value given.
        value: f32,
    },
}

impl GravityScale {
    /// A scale of `value`.
    ///
    /// # Errors
    ///
    /// [`GravityScaleError`] when `value` is negative, not finite or above
    /// [`MAX_GRAVITY_SCALE`].
    pub fn new(value: f32) -> Result<Self, GravityScaleError> {
        Ok(Self(value))
    }

    /// The round gravity this scale gives, in m/s².
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
    /// The low corner.
    pub min: Vec3,
    /// The high corner.
    pub max: Vec3,
}

impl FlightBounds {
    /// The box around `points`, grown by `margin` on every side.
    pub fn around(points: impl IntoIterator<Item = Vec3>, margin: f32) -> Self {
        let _ = (points.into_iter().count(), margin);
        todo!()
    }

    /// Whether `point` is inside the box or on its surface.
    pub fn contains(&self, point: Vec3) -> bool {
        todo!("{point}")
    }

    /// The length of the box's diagonal.
    pub fn reach(&self) -> f32 {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const DT: f32 = 1.0 / 64.0;

    fn ball() -> Ballistics {
        Ballistics {
            muzzle_velocity: 940.0,
            drag: 1.0,
        }
    }

    fn no_drag() -> Ballistics {
        Ballistics {
            muzzle_velocity: 940.0,
            drag: 0.0,
        }
    }

    fn scale(value: f32) -> GravityScale {
        GravityScale::new(value).expect("the scale is valid")
    }

    #[test]
    fn a_step_without_gravity_or_drag_moves_at_the_muzzle_velocity() {
        let flight = Flight::fired(Vec3::new(1.0, 2.0, 3.0), Vec3::X, &no_drag());

        let next = flight.step(&no_drag(), 0.0, DT);

        assert_eq!(next.velocity, Vec3::new(940.0, 0.0, 0.0));
        assert_eq!(next.position, Vec3::new(1.0 + 940.0 * DT, 2.0, 3.0));
    }

    #[test]
    fn gravity_changes_only_the_vertical_velocity() {
        let flight = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(300.0, 5.0, -200.0),
        };

        let next = flight.step(&no_drag(), 9.81, DT);

        assert_eq!(next.velocity.x, 300.0);
        assert_eq!(next.velocity.z, -200.0);
        assert_eq!(next.velocity.y, 5.0 - 9.81 * DT);
    }

    #[test]
    fn a_step_with_drag_and_gravity_applies_drag_before_gravity() {
        let flight = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(600.0, 0.0, 0.0),
        };
        let ballistics = Ballistics {
            muzzle_velocity: 600.0,
            drag: 2.0,
        };

        let next = flight.step(&ballistics, 10.0, 0.25);

        assert_eq!(next.velocity, Vec3::new(300.0, -2.5, 0.0));
        assert_eq!(next.position, Vec3::new(75.0, -0.625, 0.0));
    }

    #[test]
    fn drag_past_one_step_stops_the_round_and_never_reverses_it() {
        let heavy = Ballistics {
            muzzle_velocity: 940.0,
            drag: 100.0,
        };
        let flight = Flight::fired(Vec3::ZERO, Vec3::X, &heavy);

        let next = flight.step(&heavy, 0.0, 0.05);

        assert_eq!(next.velocity, Vec3::ZERO);
        assert_eq!(next.position, Vec3::ZERO);
    }

    #[test]
    fn a_flight_at_the_minimum_speed_is_spent() {
        let at = |speed: f32| Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(speed, 0.0, 0.0),
        };

        assert!(at(MIN_SPEED).is_spent());
        assert!(at(0.0).is_spent());
        assert!(!at(MIN_SPEED + 0.01).is_spent());
        assert!(!at(940.0).is_spent());
    }

    #[test]
    fn a_flight_is_spent_by_its_speed_not_by_one_axis() {
        let moving = |velocity: Vec3| Flight {
            position: Vec3::ZERO,
            velocity,
        };

        assert!(!moving(Vec3::new(80.0, 0.0, 80.0)).is_spent());
        assert!(moving(Vec3::new(60.0, 0.0, 80.0)).is_spent());
    }

    #[test]
    fn gravity_scale_accepts_0_and_refuses_negative_and_non_finite() {
        assert_eq!(scale(0.0), scale(0.0));
        assert!(GravityScale::new(0.0).is_ok());
        assert!(GravityScale::new(30.0).is_ok());
        for bad in [-0.001, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let result = GravityScale::new(bad);

            assert!(result.is_err(), "{bad} was accepted: {result:?}");
        }
    }

    #[test]
    fn gravity_scale_refuses_more_than_1000() {
        assert!(GravityScale::new(MAX_GRAVITY_SCALE).is_ok());
        assert!(GravityScale::new(MAX_GRAVITY_SCALE + 0.1).is_err());
        assert!(GravityScale::new(1.0e9).is_err());
    }

    #[test]
    fn gravity_scale_defaults_to_1() {
        assert_eq!(GravityScale::default(), scale(1.0));
    }

    #[test]
    fn round_gravity_is_standard_gravity_times_the_scale() {
        assert_eq!(scale(1.0).gravity(), STANDARD_GRAVITY);
        assert_eq!(scale(30.0).gravity(), STANDARD_GRAVITY * 30.0);
        assert_eq!(scale(0.0).gravity(), 0.0);
    }

    #[test]
    fn flight_bounds_contain_their_points_grown_by_the_margin() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 20.0, 30.0);

        let bounds = FlightBounds::around([b, Vec3::new(0.0, 40.0, 0.0), a], 5.0);

        assert_eq!(bounds.min, Vec3::new(-5.0, -5.0, -5.0));
        assert_eq!(bounds.max, Vec3::new(15.0, 45.0, 35.0));
        assert!(bounds.contains(a));
        assert!(bounds.contains(b));
        assert!(bounds.contains(Vec3::new(-5.0, -5.0, -5.0)), "on a face");
        assert!(bounds.contains(Vec3::new(15.0, 45.0, 35.0)), "on a face");
        assert!(!bounds.contains(Vec3::new(-5.01, 0.0, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, -5.01, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, 0.0, -5.01)));
        assert!(!bounds.contains(Vec3::new(15.01, 0.0, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, 45.01, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, 0.0, 35.01)));
    }

    #[test]
    fn reach_is_the_box_diagonal() {
        let bounds = FlightBounds::around([Vec3::ZERO, Vec3::new(3.0, 4.0, 12.0)], 0.0);

        assert_eq!(bounds.reach(), 13.0);
    }

    #[test]
    fn a_fired_flight_has_the_muzzle_speed_along_a_non_unit_direction() {
        let origin = Vec3::new(1.0, 2.0, 3.0);

        let flight = Flight::fired(origin, Vec3::new(3.0, 0.0, 4.0), &ball());

        assert_eq!(flight.position, origin);
        assert!(
            (flight.velocity.length() - 940.0).abs() < 0.01,
            "{flight:?}"
        );
        assert!((flight.velocity - Vec3::new(564.0, 0.0, 752.0)).length() < 0.01);
    }

    proptest! {
        #[test]
        fn without_gravity_speed_never_grows(
            speed in 150.0_f32..1500.0,
            drag in 0.0_f32..40.0,
            dir in (-1.0_f32..1.0, -1.0_f32..1.0, -1.0_f32..1.0),
            steps in 1usize..30,
        ) {
            let direction = Vec3::new(dir.0, dir.1, dir.2) + Vec3::X * 2.0;
            let ballistics = Ballistics { muzzle_velocity: speed, drag };
            let mut flight = Flight::fired(Vec3::ZERO, direction, &ballistics);

            for _ in 0..steps {
                let next = flight.step(&ballistics, 0.0, DT);

                prop_assert!(
                    next.velocity.length() <= flight.velocity.length() * 1.000_01,
                    "{flight:?} sped up to {next:?}"
                );
                flight = next;
            }
        }

        #[test]
        fn a_larger_gravity_scale_drops_a_round_further(
            low in 0.0_f32..50.0,
            extra in 0.5_f32..50.0,
            steps in 1usize..20,
        ) {
            let fly = |scale: f32| {
                let gravity = GravityScale::new(scale).expect("in range").gravity();
                let mut flight = Flight::fired(Vec3::ZERO, Vec3::X, &ball());
                for _ in 0..steps {
                    flight = flight.step(&ball(), gravity, DT);
                }
                flight.position.y
            };

            let lighter = fly(low);
            let heavier = fly(low + extra);

            prop_assert!(heavier < lighter, "{heavier} is not below {lighter}");
        }
    }
}
