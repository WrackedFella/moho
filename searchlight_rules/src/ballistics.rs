//! Round flight: a round leaves the muzzle at a speed, slows by drag and falls under gravity,
//! one fixed step at a time.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The pull of gravity at a gravity scale of 1, in m/s².
pub const STANDARD_GRAVITY: f32 = 9.81;

/// The largest gravity scale a tester may set.
pub const MAX_GRAVITY_SCALE: f32 = 1000.0;

/// A flight at or below this speed, in m/s, is spent.
pub const MIN_SPEED: f32 = 100.0;

/// What a round's flight depends on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ballistics {
    /// Speed at the muzzle, in m/s.
    pub muzzle_velocity: f32,
    /// How fast the round loses speed, in 1/s.
    pub drag: f32,
}

/// Where a round is and how it moves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// Position, in metres.
    pub position: Vec3,
    /// Velocity, in m/s.
    pub velocity: Vec3,
}

impl Flight {
    /// A round leaving `origin` along `direction` (need not be unit length) at the muzzle
    /// velocity.
    pub fn fired(origin: Vec3, direction: Vec3, ballistics: &Ballistics) -> Self {
        Self {
            position: origin,
            velocity: direction.normalize() * ballistics.muzzle_velocity,
        }
    }

    /// One step of `dt` seconds under `gravity` (m/s², positive down): drag slows the round
    /// (never past a stop), gravity then pulls its vertical velocity down, and it moves by the
    /// new velocity. Only correctly rounded IEEE operations, so replays are exact.
    #[must_use]
    pub fn step(&self, ballistics: &Ballistics, gravity: f32, dt: f32) -> Flight {
        let keep = (1.0 - ballistics.drag * dt).max(0.0);
        let mut velocity = self.velocity * keep;
        velocity.y -= gravity * dt;
        Flight {
            position: self.position + velocity * dt,
            velocity,
        }
    }

    /// Whether the round has slowed to [`MIN_SPEED`] or below.
    pub fn is_spent(&self) -> bool {
        self.velocity.length() <= MIN_SPEED
    }
}

/// Why a gravity scale was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[non_exhaustive]
pub enum GravityScaleError {
    /// Negative, not finite, or above [`MAX_GRAVITY_SCALE`].
    #[error("gravity scale must be from 0 to {MAX_GRAVITY_SCALE}, got {0}")]
    OutOfRange(f32),
}

/// How many times standard gravity pulls on rounds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32")]
pub struct GravityScale(f32);

impl GravityScale {
    /// A gravity scale of `scale`.
    ///
    /// # Errors
    ///
    /// [`GravityScaleError::OutOfRange`] when `scale` is negative, not finite or above
    /// [`MAX_GRAVITY_SCALE`].
    pub fn new(scale: f32) -> Result<Self, GravityScaleError> {
        if scale.is_finite() && (0.0..=MAX_GRAVITY_SCALE).contains(&scale) {
            Ok(Self(scale))
        } else {
            Err(GravityScaleError::OutOfRange(scale))
        }
    }

    /// The downward acceleration this scale gives rounds, in m/s².
    pub fn gravity(self) -> f32 {
        STANDARD_GRAVITY * self.0
    }
}

impl TryFrom<f32> for GravityScale {
    type Error = GravityScaleError;

    fn try_from(scale: f32) -> Result<Self, Self::Error> {
        Self::new(scale)
    }
}

impl Default for GravityScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// The box rounds may fly in; a round that leaves it is gone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightBounds {
    /// The corner with the smallest coordinates.
    pub min: Vec3,
    /// The corner with the largest coordinates.
    pub max: Vec3,
}

impl FlightBounds {
    /// The box around `points`, grown by `margin` metres on every side.
    pub fn around(points: impl IntoIterator<Item = Vec3>, margin: f32) -> Self {
        let (min, max) = points
            .into_iter()
            .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), p| {
                (min.min(p), max.max(p))
            });
        Self {
            min: min - Vec3::splat(margin),
            max: max + Vec3::splat(margin),
        }
    }

    /// Whether `point` is inside the box or on a face.
    pub fn contains(&self, point: Vec3) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }

    /// The length of the box's diagonal, in metres.
    pub fn reach(&self) -> f32 {
        (self.max - self.min).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const BALL: Ballistics = Ballistics {
        muzzle_velocity: 940.0,
        drag: 1.0,
    };

    fn no_drag() -> Ballistics {
        Ballistics {
            muzzle_velocity: 940.0,
            drag: 0.0,
        }
    }

    fn flight(position: Vec3, velocity: Vec3) -> Flight {
        Flight { position, velocity }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_step_without_gravity_or_drag_moves_at_the_muzzle_velocity() {
        let fired = Flight::fired(Vec3::new(1.0, 2.0, 3.0), Vec3::Z, &no_drag());

        let next = fired.step(&no_drag(), 0.0, 0.25);

        assert_eq!(next.velocity, Vec3::new(0.0, 0.0, 940.0));
        assert_eq!(next.position, Vec3::new(1.0, 2.0, 3.0 + 940.0 * 0.25));
    }

    #[test]
    fn a_step_slows_by_one_minus_drag_dt_then_moves_by_the_slowed_velocity() {
        let before = flight(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, 0.0, 200.0));
        let ballistics = Ballistics {
            muzzle_velocity: 200.0,
            drag: 1.0,
        };

        let next = before.step(&ballistics, 0.0, 0.25);

        assert_eq!(next.velocity, Vec3::new(0.0, 0.0, 150.0));
        assert_eq!(next.position, Vec3::new(0.0, 5.0, 37.5));
    }

    #[test]
    fn a_step_with_drag_and_gravity_slows_first_then_drops_by_gravity() {
        let before = flight(Vec3::ZERO, Vec3::new(0.0, 10.0, 200.0));
        let ballistics = Ballistics {
            muzzle_velocity: 200.0,
            drag: 1.0,
        };

        let next = before.step(&ballistics, 8.0, 0.25);

        assert_eq!(next.velocity, Vec3::new(0.0, 5.5, 150.0));
        assert_eq!(next.position, Vec3::new(0.0, 1.375, 37.5));
    }

    #[test]
    fn gravity_changes_only_the_vertical_velocity() {
        let before = flight(Vec3::ZERO, Vec3::new(3.0, 4.0, 5.0));

        let next = before.step(&no_drag(), 9.81, 0.25);

        assert_eq!(next.velocity.x, 3.0);
        assert_eq!(next.velocity.z, 5.0);
        assert!(close(next.velocity.y, 4.0 - 9.81 * 0.25), "{next:?}");
        assert_eq!(next.position.x, 0.75);
        assert_eq!(next.position.z, 1.25);
        assert!(close(next.position.y, next.velocity.y * 0.25), "{next:?}");
    }

    #[test]
    fn drag_past_one_step_stops_the_round_and_never_reverses_it() {
        let ballistics = Ballistics {
            muzzle_velocity: 500.0,
            drag: 100.0,
        };
        let before = flight(Vec3::new(1.0, 2.0, 3.0), Vec3::new(0.0, 0.0, 500.0));

        let overshoot = before.step(&ballistics, 0.0, 0.1);
        let exactly_one = before.step(
            &Ballistics {
                drag: 4.0,
                ..ballistics
            },
            0.0,
            0.25,
        );

        assert_eq!(overshoot.velocity, Vec3::ZERO);
        assert_eq!(overshoot.position, before.position);
        assert_eq!(exactly_one.velocity, Vec3::ZERO);
    }

    proptest! {
        #[test]
        fn without_gravity_speed_never_grows(
            speed in 100.0f32..2000.0,
            drag in 0.0f32..200.0,
            dt in 0.001f32..0.1,
            direction in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
            steps in 1usize..40,
        ) {
            let direction = Vec3::new(direction.0, direction.1, direction.2);
            prop_assume!(direction.length() > 0.1);
            let ballistics = Ballistics { muzzle_velocity: speed, drag };
            let mut flight = Flight::fired(Vec3::ZERO, direction, &ballistics);

            for _ in 0..steps {
                let before = flight.velocity.length();
                flight = flight.step(&ballistics, 0.0, dt);

                prop_assert!(
                    flight.velocity.length() <= before * (1.0 + 1e-6),
                    "{} grew to {}", before, flight.velocity.length()
                );
            }
        }

        #[test]
        fn a_larger_gravity_scale_drops_a_round_further(
            low in 0.0f32..500.0,
            extra in 0.5f32..400.0,
            dt in 0.008f32..0.034,
            steps in 1usize..30,
        ) {
            let high = low + extra;
            let drops = |scale: f32| {
                let gravity = GravityScale::new(scale).expect("a valid scale").gravity();
                let mut flight = Flight::fired(Vec3::new(0.0, 1.5, 0.0), Vec3::Z, &BALL);
                for _ in 0..steps {
                    flight = flight.step(&BALL, gravity, dt);
                }
                flight.position.y
            };

            prop_assert!(drops(high) < drops(low));
        }
    }

    #[test]
    fn a_flight_at_the_minimum_speed_is_spent() {
        let at = flight(Vec3::ZERO, Vec3::new(60.0, 80.0, 0.0));
        let above = flight(Vec3::ZERO, Vec3::new(70.0, 75.0, 0.0));
        let below = flight(Vec3::ZERO, Vec3::new(0.0, 99.99, 0.0));
        let stopped = flight(Vec3::ZERO, Vec3::ZERO);

        assert!(at.is_spent());
        assert!(!above.is_spent());
        assert!(below.is_spent());
        assert!(stopped.is_spent());
    }

    #[test]
    fn gravity_scale_accepts_0_and_refuses_negative_and_non_finite() {
        assert!(GravityScale::new(0.0).is_ok());
        assert!(GravityScale::new(30.0).is_ok());
        for bad in [-0.001, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(GravityScale::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn gravity_scale_refuses_more_than_1000() {
        assert!(GravityScale::new(MAX_GRAVITY_SCALE).is_ok());
        assert!(GravityScale::new(1000.5).is_err());
        assert!(GravityScale::new(f32::MAX).is_err());
    }

    #[test]
    fn gravity_scale_defaults_to_1() {
        let default = GravityScale::default();

        assert_eq!(default, GravityScale::new(1.0).expect("1 is a valid scale"));
        assert_eq!(default.gravity(), STANDARD_GRAVITY);
    }

    #[test]
    fn a_gravity_scale_read_from_data_goes_through_the_same_checks() {
        let valid: Result<GravityScale, _> = serde_json::from_str("30.0");
        let negative: Result<GravityScale, _> = serde_json::from_str("-1.0");
        let too_large: Result<GravityScale, _> = serde_json::from_str("1001.0");

        assert_eq!(
            valid.expect("30 is valid"),
            GravityScale::new(30.0).unwrap()
        );
        assert!(negative.is_err());
        assert!(too_large.is_err());
    }

    #[test]
    fn round_gravity_is_standard_gravity_times_the_scale() {
        let one = GravityScale::new(1.0).expect("valid").gravity();
        let thirty = GravityScale::new(30.0).expect("valid").gravity();
        let zero = GravityScale::new(0.0).expect("valid").gravity();

        assert_eq!(STANDARD_GRAVITY, 9.81);
        assert_eq!(one, 9.81);
        assert_eq!(thirty, 9.81 * 30.0);
        assert_eq!(zero, 0.0);
    }

    #[test]
    fn flight_bounds_contain_their_points_grown_by_the_margin() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 5.0, -20.0);

        let bounds = FlightBounds::around([a, b], 100.0);

        assert!(bounds.contains(a));
        assert!(bounds.contains(b));
        assert!(
            bounds.contains(Vec3::new(110.0, 105.0, 100.0)),
            "max corner"
        );
        assert!(
            bounds.contains(Vec3::new(-100.0, -100.0, -120.0)),
            "min corner"
        );
        assert!(
            bounds.contains(Vec3::new(110.0, 0.0, 0.0)),
            "a point on a face"
        );
        for outside in [
            Vec3::new(110.01, 0.0, 0.0),
            Vec3::new(-100.01, 0.0, 0.0),
            Vec3::new(0.0, 105.01, 0.0),
            Vec3::new(0.0, -100.01, 0.0),
            Vec3::new(0.0, 0.0, 100.01),
            Vec3::new(0.0, 0.0, -120.01),
        ] {
            assert!(!bounds.contains(outside), "{outside:?}");
        }
    }

    #[test]
    fn reach_is_the_box_diagonal() {
        let bounds = FlightBounds::around([Vec3::ZERO, Vec3::new(3.0, 4.0, 12.0)], 0.0);

        assert_eq!(bounds.reach(), 13.0);
    }

    #[test]
    fn reach_is_the_diagonal_when_the_box_is_centred_on_the_origin() {
        let bounds = FlightBounds::around(
            [Vec3::new(-3.0, -4.0, -12.0), Vec3::new(3.0, 4.0, 12.0)],
            0.0,
        );

        assert_eq!(bounds.reach(), 26.0);
    }

    #[test]
    fn reach_is_the_diagonal_when_the_box_lies_in_negative_coordinates() {
        let bounds = FlightBounds::around(
            [Vec3::new(-6.0, -8.0, -24.0), Vec3::new(-3.0, -4.0, -12.0)],
            0.0,
        );

        assert_eq!(bounds.reach(), 13.0);
    }

    #[test]
    fn reach_grows_by_the_margin_on_every_side() {
        let bounds = FlightBounds::around(
            [Vec3::new(-2.0, -2.0, -2.0), Vec3::new(-2.0, -2.0, -2.0)],
            1.0,
        );

        assert_eq!(bounds.reach(), 12.0_f32.sqrt());
    }

    #[test]
    fn a_fired_flight_has_the_muzzle_speed_along_a_non_unit_direction() {
        let origin = Vec3::new(1.0, 2.0, 3.0);

        let fired = Flight::fired(origin, Vec3::new(0.0, 30.0, 40.0), &BALL);

        assert_eq!(fired.position, origin);
        assert!(close(fired.velocity.x, 0.0), "{fired:?}");
        assert!((fired.velocity.y - 564.0).abs() < 1e-2, "{fired:?}");
        assert!((fired.velocity.z - 752.0).abs() < 1e-2, "{fired:?}");
        assert!((fired.velocity.length() - 940.0).abs() < 1e-2, "{fired:?}");
    }
}
