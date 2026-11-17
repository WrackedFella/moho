//! A round in flight: muzzle velocity, drag and gravity stepped as segments.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Standard gravity at a scale of 1, in m/s².
pub const STANDARD_GRAVITY: f32 = 9.81;

/// The speed at or below which a round is spent, in m/s.
pub const MIN_SPEED: f32 = 100.0;

/// The largest gravity scale a tester may ask for.
pub const MAX_GRAVITY_SCALE: f32 = 1000.0;

/// How a round moves through the air.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ballistics {
    /// Speed when it leaves the muzzle, in m/s.
    pub muzzle_velocity: f32,
    /// Fraction of its velocity a round loses each second, in 1/s.
    pub drag: f32,
}

/// Where a round is and how fast it moves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// Position, in metres.
    pub position: Vec3,
    /// Velocity, in m/s.
    pub velocity: Vec3,
}

impl Flight {
    /// A round leaving `origin` along `direction` (any non-zero length) at the muzzle velocity.
    #[must_use]
    pub fn fired(origin: Vec3, direction: Vec3, ballistics: &Ballistics) -> Self {
        Self {
            position: origin,
            velocity: direction.normalize() * ballistics.muzzle_velocity,
        }
    }

    /// The flight `dt` seconds later under `gravity` (m/s², positive down).
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

/// A multiplier on standard gravity for rounds only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GravityScale(f32);

/// Why a gravity scale was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[non_exhaustive]
#[error("a gravity scale is between 0 and {MAX_GRAVITY_SCALE}, got {scale}")]
pub struct GravityScaleError {
    /// The value given.
    pub scale: f32,
}

impl GravityScale {
    /// A scale of `scale`.
    ///
    /// # Errors
    ///
    /// [`GravityScaleError`] when `scale` is negative, not finite or above
    /// [`MAX_GRAVITY_SCALE`].
    pub fn new(scale: f32) -> Result<Self, GravityScaleError> {
        if scale.is_finite() && (0.0..=MAX_GRAVITY_SCALE).contains(&scale) {
            Ok(Self(scale))
        } else {
            Err(GravityScaleError { scale })
        }
    }

    /// Gravity for a round, in m/s²: [`STANDARD_GRAVITY`] times the scale.
    #[must_use]
    pub fn gravity(self) -> f32 {
        STANDARD_GRAVITY * self.0
    }
}

impl Default for GravityScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// The box a round may fly in; one that leaves it is gone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightBounds {
    /// The corner with the smallest coordinates.
    pub min: Vec3,
    /// The corner with the largest coordinates.
    pub max: Vec3,
}

impl FlightBounds {
    /// The box around `points`, grown by `margin` on every side.
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

    /// Whether `point` is inside the box or on a face of it.
    #[must_use]
    pub fn contains(&self, point: Vec3) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }

    /// The length of the box's diagonal, in metres.
    #[must_use]
    pub fn reach(&self) -> f32 {
        (self.max - self.min).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const DT: f32 = 1.0 / 60.0;

    fn ball(muzzle_velocity: f32, drag: f32) -> Ballistics {
        Ballistics {
            muzzle_velocity,
            drag,
        }
    }

    fn scale(value: f32) -> GravityScale {
        GravityScale::new(value).expect("a valid scale")
    }

    fn close(actual: f32, expected: f32, tolerance: f32) -> bool {
        (actual - expected).abs() <= tolerance
    }

    #[test]
    fn a_step_without_gravity_or_drag_moves_at_the_muzzle_velocity() {
        let ballistics = ball(940.0, 0.0);
        let origin = Vec3::new(1.0, 2.0, 3.0);
        let flight = Flight::fired(origin, Vec3::Z, &ballistics);

        let next = flight.step(&ballistics, 0.0, DT);

        assert_eq!(next.velocity, Vec3::new(0.0, 0.0, 940.0));
        assert!(close(next.position.x, 1.0, 1e-6), "{next:?}");
        assert!(close(next.position.y, 2.0, 1e-6), "{next:?}");
        assert!(close(next.position.z, 3.0 + 940.0 * DT, 1e-3), "{next:?}");
    }

    #[test]
    fn gravity_changes_only_the_vertical_velocity() {
        let ballistics = ball(500.0, 0.0);
        let flight = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(300.0, 0.0, 400.0),
        };

        let next = flight.step(&ballistics, 10.0, 0.1);

        assert_eq!(next.velocity.x, 300.0);
        assert_eq!(next.velocity.z, 400.0);
        assert!(close(next.velocity.y, -1.0, 1e-6), "{next:?}");
    }

    #[test]
    fn a_step_applies_drag_to_the_old_velocity_before_gravity_is_added() {
        let ballistics = ball(500.0, 1.0);
        let flight = Flight {
            position: Vec3::new(0.0, 10.0, 0.0),
            velocity: Vec3::new(0.0, 100.0, 200.0),
        };

        let next = flight.step(&ballistics, 10.0, 0.1);

        assert!(close(next.velocity.y, 100.0 * 0.9 - 1.0, 1e-4), "{next:?}");
        assert!(close(next.velocity.z, 200.0 * 0.9, 1e-4), "{next:?}");
        assert!(close(next.position.y, 10.0 + 8.9, 1e-4), "{next:?}");
        assert!(close(next.position.z, 18.0, 1e-4), "{next:?}");
    }

    #[test]
    fn drag_past_one_step_stops_the_round_and_never_reverses_it() {
        let ballistics = ball(940.0, 100.0);
        let flight = Flight {
            position: Vec3::new(5.0, 6.0, 7.0),
            velocity: Vec3::new(0.0, 0.0, 940.0),
        };

        let overshoot = flight.step(&ballistics, 0.0, 0.02);
        let exactly_one = flight.step(&ballistics, 0.0, 0.01);

        assert_eq!(overshoot.velocity, Vec3::ZERO);
        assert_eq!(overshoot.position, flight.position);
        assert_eq!(exactly_one.velocity, Vec3::ZERO);
    }

    #[test]
    fn a_flight_at_the_minimum_speed_is_spent() {
        let at = |velocity: Vec3| {
            Flight {
                position: Vec3::ZERO,
                velocity,
            }
            .is_spent()
        };

        assert!(at(Vec3::new(0.0, 0.0, MIN_SPEED)));
        assert!(at(Vec3::new(60.0, 0.0, 80.0)));
        assert!(at(Vec3::ZERO));
        assert!(!at(Vec3::new(0.0, 0.0, 100.5)));
        assert!(!at(Vec3::new(0.0, -101.0, 0.0)));
    }

    #[test]
    fn gravity_scale_accepts_0_and_refuses_negative_and_non_finite() {
        assert_eq!(scale(0.0).gravity(), 0.0);
        assert!(GravityScale::new(-0.5).is_err());
        assert!(GravityScale::new(f32::NAN).is_err());
        assert!(GravityScale::new(f32::INFINITY).is_err());
        assert!(GravityScale::new(f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn gravity_scale_refuses_more_than_1000() {
        assert_eq!(MAX_GRAVITY_SCALE, 1000.0);
        assert!(GravityScale::new(MAX_GRAVITY_SCALE).is_ok());
        assert!(GravityScale::new(1000.5).is_err());
        assert!(GravityScale::new(f32::MAX).is_err());
    }

    #[test]
    fn gravity_scale_defaults_to_1() {
        assert_eq!(GravityScale::default(), scale(1.0));
        assert_eq!(GravityScale::default().gravity(), STANDARD_GRAVITY);
    }

    #[test]
    fn round_gravity_is_standard_gravity_times_the_scale() {
        assert_eq!(STANDARD_GRAVITY, 9.81);
        assert_eq!(scale(1.0).gravity(), 9.81);
        assert_eq!(scale(30.0).gravity(), 9.81 * 30.0);
    }

    #[test]
    fn flight_bounds_contain_their_points_grown_by_the_margin() {
        let points = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 5.0, -20.0)];

        let bounds = FlightBounds::around(points, 3.0);

        assert_eq!(bounds.min, Vec3::new(-3.0, -3.0, -23.0));
        assert_eq!(bounds.max, Vec3::new(13.0, 8.0, 3.0));
        for point in points {
            assert!(bounds.contains(point), "{point}");
        }
        assert!(bounds.contains(bounds.min), "a point on the min corner");
        assert!(bounds.contains(bounds.max), "a point on the max corner");
        assert!(bounds.contains(Vec3::new(13.0, 0.0, 0.0)), "on the +x face");
        assert!(bounds.contains(Vec3::new(0.0, -3.0, 0.0)), "on the -y face");
        for outside in [
            Vec3::new(13.01, 0.0, 0.0),
            Vec3::new(-3.01, 0.0, 0.0),
            Vec3::new(0.0, 8.01, 0.0),
            Vec3::new(0.0, -3.01, 0.0),
            Vec3::new(0.0, 0.0, 3.01),
            Vec3::new(0.0, 0.0, -23.01),
        ] {
            assert!(!bounds.contains(outside), "{outside}");
        }
    }

    #[test]
    fn reach_is_the_box_diagonal() {
        let flush = FlightBounds::around([Vec3::ZERO, Vec3::new(3.0, 4.0, 12.0)], 0.0);
        let grown = FlightBounds::around([Vec3::ZERO, Vec3::new(3.0, 4.0, 12.0)], 1.0);

        assert!(close(flush.reach(), 13.0, 1e-4), "{}", flush.reach());
        assert!(
            close(grown.reach(), 257.0_f32.sqrt(), 1e-4),
            "{}",
            grown.reach()
        );
    }

    #[test]
    fn a_fired_flight_has_the_muzzle_speed_along_a_non_unit_direction() {
        let origin = Vec3::new(1.0, 1.6, -4.0);

        let flight = Flight::fired(origin, Vec3::new(0.0, 3.0, 4.0), &ball(500.0, 1.0));

        assert_eq!(flight.position, origin);
        assert!(close(flight.velocity.x, 0.0, 1e-4), "{flight:?}");
        assert!(close(flight.velocity.y, 300.0, 1e-2), "{flight:?}");
        assert!(close(flight.velocity.z, 400.0, 1e-2), "{flight:?}");
    }

    proptest! {
        #[test]
        fn without_gravity_speed_never_grows(
            muzzle in 100.0f32..2000.0,
            drag in 0.0f32..50.0,
            dt in 0.001f32..0.1,
            direction in (-1.0f32..1.0, -1.0f32..1.0, 0.1f32..1.0),
            steps in 1usize..40,
        ) {
            let ballistics = ball(muzzle, drag);
            let mut flight = Flight::fired(
                Vec3::ZERO,
                Vec3::new(direction.0, direction.1, direction.2),
                &ballistics,
            );

            for _ in 0..steps {
                let next = flight.step(&ballistics, 0.0, dt);
                prop_assert!(
                    next.velocity.length() <= flight.velocity.length() * (1.0 + 1e-5),
                    "{:?} -> {:?}", flight, next
                );
                flight = next;
            }
        }

        #[test]
        fn a_larger_gravity_scale_drops_a_round_further(
            low in 0.0f32..500.0,
            gap in 1.0f32..400.0,
            steps in 1usize..30,
            muzzle in 300.0f32..1000.0,
        ) {
            let ballistics = ball(muzzle, 1.0);
            let fly = |scale: f32| {
                let gravity = GravityScale::new(scale).unwrap().gravity();
                let mut flight = Flight::fired(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, &ballistics);
                for _ in 0..steps {
                    flight = flight.step(&ballistics, gravity, DT);
                }
                flight.position.y
            };

            let shallow = fly(low);
            let steep = fly(low + gap);

            prop_assert!(steep < shallow, "{steep} !< {shallow}");
        }
    }
}
