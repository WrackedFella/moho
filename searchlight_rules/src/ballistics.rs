//! A round in flight: one integration step under gravity and drag, and the bounds it flies in.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Standard gravity, in m/s^2, that a gravity scale multiplies.
pub const STANDARD_GRAVITY: f32 = 9.81;

/// The largest gravity scale a tester can set.
pub const MAX_GRAVITY_SCALE: f32 = 1000.0;

/// Speed, in m/s, at or below which a round is spent.
pub const MIN_SPEED: f32 = 100.0;

/// What a round flies with: how fast it leaves the muzzle and how quickly it slows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ballistics {
    /// Speed at the muzzle, in m/s.
    pub muzzle_velocity: f32,
    /// Fraction of its velocity a round sheds per second, in 1/s.
    pub drag: f32,
}

/// A round in the air.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// Where the round is, in metres.
    pub position: Vec3,
    /// How it moves, in m/s.
    pub velocity: Vec3,
}

impl Flight {
    /// A round leaving `origin` along `direction` (any length) at the muzzle velocity.
    #[must_use]
    pub fn fired(origin: Vec3, direction: Vec3, ballistics: &Ballistics) -> Self {
        Self {
            position: origin,
            velocity: direction.normalize_or_zero() * ballistics.muzzle_velocity,
        }
    }

    /// The flight `dt` seconds later under `gravity` (m/s^2, downwards).
    ///
    /// Semi-implicit Euler with linear drag, using only correctly rounded IEEE operations so
    /// replays do not differ between platforms.
    #[must_use]
    pub fn step(&self, ballistics: &Ballistics, gravity: f32, dt: f32) -> Self {
        let keep = (1.0 - ballistics.drag * dt).max(0.0);
        let velocity = self.velocity * keep - Vec3::Y * (gravity * dt);
        Self {
            position: self.position + velocity * dt,
            velocity,
        }
    }

    /// Whether the round has slowed to [`MIN_SPEED`] or below.
    #[must_use]
    pub fn is_spent(&self) -> bool {
        self.velocity.length_squared() <= MIN_SPEED * MIN_SPEED
    }
}

/// Why a gravity scale was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[non_exhaustive]
pub enum GravityScaleError {
    /// Negative, not a number, infinite or above [`MAX_GRAVITY_SCALE`].
    #[error("gravity scale must be from 0 to {MAX_GRAVITY_SCALE}, got {0}")]
    OutOfRange(f32),
}

/// How many times standard gravity pulls on rounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GravityScale(f32);

impl Default for GravityScale {
    /// Standard gravity.
    fn default() -> Self {
        Self(1.0)
    }
}

impl GravityScale {
    /// A scale of `scale`.
    ///
    /// # Errors
    ///
    /// [`GravityScaleError::OutOfRange`] for a negative, non-finite or too large scale.
    pub fn new(scale: f32) -> Result<Self, GravityScaleError> {
        if (0.0..=MAX_GRAVITY_SCALE).contains(&scale) {
            Ok(Self(scale))
        } else {
            Err(GravityScaleError::OutOfRange(scale))
        }
    }

    /// The pull on a round, in m/s^2.
    #[must_use]
    pub fn gravity(self) -> f32 {
        STANDARD_GRAVITY * self.0
    }
}

/// The box rounds may fly in; one that leaves it is gone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl FlightBounds {
    /// The box around `points`, grown by `margin` on every side. `points` must not be empty:
    /// the box is then inverted and contains nothing.
    #[must_use]
    pub fn around(points: impl IntoIterator<Item = Vec3>, margin: f32) -> Self {
        let (min, max) = points.into_iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), point| (min.min(point), max.max(point)),
        );
        Self {
            min: min - Vec3::splat(margin),
            max: max + Vec3::splat(margin),
        }
    }

    /// Whether `point` is inside the box or on its surface.
    #[must_use]
    pub fn contains(&self, point: Vec3) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }

    /// The box's diagonal, the farthest a ray inside it can go.
    #[must_use]
    pub fn reach(&self) -> f32 {
        (self.max - self.min).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn ball(muzzle_velocity: f32, drag: f32) -> Ballistics {
        Ballistics {
            muzzle_velocity,
            drag,
        }
    }

    #[test]
    fn a_step_without_gravity_or_drag_moves_at_the_muzzle_velocity() {
        let ballistics = ball(940.0, 0.0);
        let flight = Flight::fired(Vec3::new(1.0, 2.0, 3.0), Vec3::X, &ballistics);

        let next = flight.step(&ballistics, 0.0, DT);

        assert_eq!(next.velocity, Vec3::new(940.0, 0.0, 0.0));
        assert_eq!(next.position, Vec3::new(1.0 + 940.0 * DT, 2.0, 3.0));
    }

    #[test]
    fn gravity_changes_only_the_vertical_velocity() {
        let ballistics = ball(500.0, 0.0);
        let flight = Flight {
            position: Vec3::new(1.0, 2.0, 3.0),
            velocity: Vec3::new(100.0, 20.0, -30.0),
        };

        let next = flight.step(&ballistics, 10.0, 0.5);

        assert_eq!(next.velocity, Vec3::new(100.0, 15.0, -30.0));
        assert_eq!(next.position, Vec3::new(51.0, 9.5, -12.0));
    }

    #[test]
    fn drag_scales_the_velocity_by_one_minus_drag_times_the_step() {
        let ballistics = ball(500.0, 1.0);
        let flight = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(100.0, 40.0, -20.0),
        };

        let next = flight.step(&ballistics, 0.0, 0.5);

        assert_eq!(next.velocity, Vec3::new(50.0, 20.0, -10.0));
        assert_eq!(next.position, Vec3::new(25.0, 10.0, -5.0));
    }

    #[test]
    fn drag_past_one_step_stops_the_round_and_never_reverses_it() {
        let ballistics = ball(500.0, 100.0);
        let flight = Flight {
            position: Vec3::new(4.0, 5.0, 6.0),
            velocity: Vec3::new(500.0, -200.0, 50.0),
        };

        let next = flight.step(&ballistics, 0.0, 0.1);

        assert_eq!(next.velocity, Vec3::ZERO);
        assert_eq!(next.position, Vec3::new(4.0, 5.0, 6.0));
    }

    #[test]
    fn gravity_is_applied_after_the_drag_factor() {
        let ballistics = ball(500.0, 1.0);
        let flight = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(100.0, 40.0, 0.0),
        };

        let next = flight.step(&ballistics, 10.0, 0.5);

        assert_eq!(next.velocity, Vec3::new(50.0, 15.0, 0.0));
    }

    #[test]
    fn a_flight_at_the_minimum_speed_is_spent() {
        let at = |velocity: Vec3| Flight {
            position: Vec3::ZERO,
            velocity,
        };

        assert!(at(Vec3::new(60.0, 80.0, 0.0)).is_spent(), "exactly 100");
        assert!(at(Vec3::new(0.0, 0.0, -99.99)).is_spent(), "just below");
        assert!(at(Vec3::ZERO).is_spent(), "at rest");
        assert!(!at(Vec3::new(100.01, 0.0, 0.0)).is_spent(), "just above");
        assert!(!at(Vec3::new(0.0, -940.0, 0.0)).is_spent(), "falling fast");
    }

    #[test]
    fn gravity_scale_accepts_0_and_refuses_negative_and_non_finite() {
        assert!(GravityScale::new(0.0).is_ok());
        assert!(GravityScale::new(1.0).is_ok());
        assert!(GravityScale::new(30.0).is_ok());
        assert!(GravityScale::new(-0.001).is_err());
        assert!(GravityScale::new(-30.0).is_err());
        assert!(GravityScale::new(f32::NAN).is_err());
        assert!(GravityScale::new(f32::INFINITY).is_err());
        assert!(GravityScale::new(f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn gravity_scale_defaults_to_1() {
        let default = GravityScale::default();

        assert_eq!(default, GravityScale::new(1.0).unwrap());
        assert_eq!(default.gravity(), STANDARD_GRAVITY);
    }

    #[test]
    fn gravity_scale_refuses_more_than_1000() {
        assert!(GravityScale::new(MAX_GRAVITY_SCALE).is_ok());
        assert!(GravityScale::new(1000.01).is_err());
        assert!(GravityScale::new(1.0e6).is_err());
    }

    #[test]
    fn round_gravity_is_standard_gravity_times_the_scale() {
        let one = GravityScale::new(1.0).unwrap();
        let thirty = GravityScale::new(30.0).unwrap();
        let none = GravityScale::new(0.0).unwrap();

        assert_eq!(STANDARD_GRAVITY, 9.81);
        assert_eq!(one.gravity(), 9.81);
        assert_eq!(thirty.gravity(), 9.81 * 30.0);
        assert_eq!(none.gravity(), 0.0);
    }

    #[test]
    fn flight_bounds_contain_their_points_grown_by_the_margin() {
        let bounds = FlightBounds::around(
            [
                Vec3::new(10.0, 5.0, 2.0),
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(4.0, 1.0, 1.0),
            ],
            3.0,
        );

        assert!(bounds.contains(Vec3::new(5.0, 2.0, 1.0)), "inside");
        assert!(bounds.contains(Vec3::new(-3.0, -3.0, -3.0)), "low corner");
        assert!(bounds.contains(Vec3::new(13.0, 8.0, 5.0)), "high corner");
        assert!(bounds.contains(Vec3::new(13.0, 0.0, 0.0)), "on a face");
        for outside in [
            Vec3::new(-3.01, 0.0, 0.0),
            Vec3::new(13.01, 0.0, 0.0),
            Vec3::new(0.0, -3.01, 0.0),
            Vec3::new(0.0, 8.01, 0.0),
            Vec3::new(0.0, 0.0, -3.01),
            Vec3::new(0.0, 0.0, 5.01),
        ] {
            assert!(!bounds.contains(outside), "{outside}");
        }
    }

    #[test]
    fn reach_is_the_box_diagonal() {
        let bounds = FlightBounds::around([Vec3::ZERO, Vec3::new(0.0, 1.0, 4.0)], 1.0);

        assert_eq!(bounds.reach(), 7.0);
    }

    #[test]
    fn a_fired_flight_has_the_muzzle_speed_along_a_non_unit_direction() {
        let ballistics = ball(940.0, 1.0);
        let origin = Vec3::new(1.0, 2.0, 3.0);

        let flight = Flight::fired(origin, Vec3::new(0.0, 30.0, 40.0), &ballistics);

        assert_eq!(flight.position, origin);
        let expected = Vec3::new(0.0, 564.0, 752.0);
        assert!(
            (flight.velocity - expected).length() < 0.01,
            "{}",
            flight.velocity
        );
    }
}
