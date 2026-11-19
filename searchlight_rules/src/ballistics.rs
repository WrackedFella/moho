//! Rounds in flight: a stepped path under gravity and drag, platform-free.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Metres per second squared at a gravity scale of 1.
pub const STANDARD_GRAVITY: f32 = 9.81;

/// The largest gravity scale a tester may set.
pub const MAX_GRAVITY_SCALE: f32 = 1000.0;

/// A flight at or below this speed, in metres per second, is spent.
pub const MIN_SPEED: f32 = 100.0;

/// What a round's flight depends on, read from its definition.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ballistics {
    /// Speed at the muzzle, in metres per second.
    pub muzzle_velocity: f32,
    /// Speed lost per second, as a fraction of the speed, in 1/s.
    pub drag: f32,
}

/// A round in the air: where it is and how it moves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// Where the round is, in metres.
    pub position: Vec3,
    /// How the round moves, in metres per second.
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

    /// The flight `dt` seconds later under `gravity` (metres per second squared, downward).
    ///
    /// Drag never reverses a round: the speed kept in one step is floored at 0. Only correctly
    /// rounded IEEE operations are used, so a replay is exact on one platform.
    #[must_use]
    pub fn step(&self, ballistics: &Ballistics, gravity: f32, dt: f32) -> Flight {
        let kept = (1.0 - ballistics.drag * dt).max(0.0);
        let velocity = self.velocity * kept - Vec3::Y * gravity * dt;
        Flight {
            position: self.position + velocity * dt,
            velocity,
        }
    }

    /// Whether the round is at or below [`MIN_SPEED`].
    #[must_use]
    pub fn is_spent(&self) -> bool {
        self.velocity.length_squared() <= MIN_SPEED * MIN_SPEED
    }
}

/// Why a gravity scale was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[non_exhaustive]
#[error("gravity scale {value} must be from 0 to 1000")]
pub struct GravityScaleError {
    /// The value given.
    pub value: f32,
}

/// How strongly gravity pulls rounds, as a multiple of [`STANDARD_GRAVITY`]. Moves rounds only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GravityScale(f32);

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
            Err(GravityScaleError { value: scale })
        }
    }

    /// The downward acceleration this scale gives rounds.
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

/// The box rounds may fly in; one that leaves it is gone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightBounds {
    /// The corner with the smallest coordinates.
    pub min: Vec3,
    /// The corner with the largest coordinates.
    pub max: Vec3,
}

impl FlightBounds {
    /// The box around `points`, grown by `margin` metres on every side.
    #[must_use]
    pub fn around(points: impl IntoIterator<Item = Vec3>, margin: f32) -> Self {
        let mut points = points.into_iter();
        let first = points.next().unwrap_or(Vec3::ZERO);
        let (min, max) = points.fold((first, first), |(min, max), p| (min.min(p), max.max(p)));
        Self {
            min: min - Vec3::splat(margin),
            max: max + Vec3::splat(margin),
        }
    }

    /// Whether `point` is inside the box, faces included.
    #[must_use]
    pub fn contains(&self, point: Vec3) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }

    /// The length of the box diagonal: the farthest a ray inside it can run.
    #[must_use]
    pub fn reach(&self) -> f32 {
        self.min.distance(self.max)
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
        GravityScale::new(value).expect("the test scale is valid")
    }

    fn near(actual: f32, expected: f32, tolerance: f32) -> bool {
        (actual - expected).abs() <= tolerance
    }

    #[test]
    fn a_fired_flight_has_the_muzzle_speed_along_a_non_unit_direction() {
        let origin = Vec3::new(1.0, 2.0, 3.0);

        let flight = Flight::fired(origin, Vec3::new(30.0, 0.0, 40.0), &ball(940.0, 1.0));

        assert_eq!(flight.position, origin);
        assert!(
            near(flight.velocity.length(), 940.0, 1e-2),
            "{:?}",
            flight.velocity
        );
        assert!(
            near(flight.velocity.x, 564.0, 1e-2),
            "{:?}",
            flight.velocity
        );
        assert!(near(flight.velocity.y, 0.0, 1e-3), "{:?}", flight.velocity);
        assert!(
            near(flight.velocity.z, 752.0, 1e-2),
            "{:?}",
            flight.velocity
        );
    }

    #[test]
    fn a_step_without_gravity_or_drag_moves_at_the_muzzle_velocity() {
        let origin = Vec3::new(1.0, 2.0, 3.0);
        let ballistics = ball(940.0, 0.0);
        let flight = Flight::fired(origin, Vec3::Z, &ballistics);

        let next = flight.step(&ballistics, 0.0, DT);

        assert_eq!(next.velocity, Vec3::new(0.0, 0.0, 940.0));
        assert!(near(next.position.x, 1.0, 1e-6), "{:?}", next.position);
        assert!(near(next.position.y, 2.0, 1e-6), "{:?}", next.position);
        assert!(
            near(next.position.z, 3.0 + 940.0 * DT, 1e-3),
            "{:?}",
            next.position
        );
    }

    #[test]
    fn gravity_changes_only_the_vertical_velocity() {
        let flight = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(300.0, 4.0, -500.0),
        };

        let next = flight.step(&ball(940.0, 0.0), 9.81, DT);

        assert_eq!(next.velocity.x, 300.0);
        assert_eq!(next.velocity.z, -500.0);
        assert!(
            near(next.velocity.y, 4.0 - 9.81 * DT, 1e-5),
            "{:?}",
            next.velocity
        );
    }

    #[test]
    fn a_step_moves_by_the_slowed_and_dropped_velocity() {
        let ballistics = ball(940.0, 1.0);
        let flight = Flight {
            position: Vec3::new(0.0, 10.0, 0.0),
            velocity: Vec3::new(0.0, 0.0, 600.0),
        };

        let next = flight.step(&ballistics, 9.81, DT);

        let slowed = 600.0 * (1.0 - DT);
        assert!(near(next.velocity.z, slowed, 1e-3), "{:?}", next.velocity);
        assert!(
            near(next.velocity.y, -9.81 * DT, 1e-5),
            "{:?}",
            next.velocity
        );
        assert!(
            near(next.position.z, slowed * DT, 1e-3),
            "{:?}",
            next.position
        );
        assert!(
            near(next.position.y, 10.0 - 9.81 * DT * DT, 1e-5),
            "{:?}",
            next.position
        );
    }

    #[test]
    fn drag_past_one_step_stops_the_round_and_never_reverses_it() {
        let flight = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(500.0, 0.0, 200.0),
        };

        for drag in [61.0, 600.0, 1.0e6] {
            let next = flight.step(&ball(940.0, drag), 0.0, DT);

            assert_eq!(next.velocity, Vec3::ZERO, "drag {drag}");
            assert_eq!(next.position, Vec3::ZERO, "drag {drag}");
        }
    }

    #[test]
    fn a_flight_at_the_minimum_speed_is_spent() {
        let at = |speed: f32| Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(0.0, 0.0, speed),
        };

        assert!(at(MIN_SPEED).is_spent());
        assert!(at(MIN_SPEED - 0.01).is_spent());
        assert!(at(0.0).is_spent());
        assert!(!at(MIN_SPEED + 0.01).is_spent());
        assert!(!at(940.0).is_spent());
        assert_eq!(MIN_SPEED, 100.0);
    }

    #[test]
    fn a_flight_is_spent_by_its_speed_not_its_direction() {
        let down = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(0.0, -MIN_SPEED, 0.0),
        };
        let fast_down = Flight {
            position: Vec3::ZERO,
            velocity: Vec3::new(0.0, -500.0, 0.0),
        };

        assert!(down.is_spent());
        assert!(!fast_down.is_spent());
    }

    #[test]
    fn gravity_scale_accepts_0_and_refuses_negative_and_non_finite() {
        let zero = GravityScale::new(0.0).expect("0 turns the drop off");
        assert_eq!(zero.gravity(), 0.0);
        assert!(GravityScale::new(30.0).is_ok());

        for bad in [
            -0.5,
            -1.0,
            f32::MIN,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let result = GravityScale::new(bad);

            assert!(result.is_err(), "{bad} was accepted: {result:?}");
        }
    }

    #[test]
    fn gravity_scale_refuses_more_than_1000() {
        assert_eq!(MAX_GRAVITY_SCALE, 1000.0);

        let top = GravityScale::new(1000.0);
        let over = GravityScale::new(1000.0_f32.next_up());

        assert!(top.is_ok(), "1000 is the top: {top:?}");
        assert!(over.is_err(), "{over:?}");
        assert!(GravityScale::new(1.0e6).is_err());
    }

    #[test]
    fn gravity_scale_defaults_to_1() {
        let default = GravityScale::default();

        assert_eq!(default, scale(1.0));
        assert_eq!(default.gravity(), STANDARD_GRAVITY);
    }

    #[test]
    fn round_gravity_is_standard_gravity_times_the_scale() {
        assert_eq!(STANDARD_GRAVITY, 9.81);

        assert_eq!(scale(1.0).gravity(), STANDARD_GRAVITY);
        assert_eq!(scale(30.0).gravity(), STANDARD_GRAVITY * 30.0);
        assert_eq!(scale(0.0).gravity(), 0.0);
    }

    #[test]
    fn flight_bounds_contain_their_points_grown_by_the_margin() {
        let bounds = FlightBounds::around(
            [Vec3::new(30.0, 15.0, 22.0), Vec3::new(20.0, 25.0, 18.0)],
            3.0,
        );

        assert!(bounds.contains(Vec3::new(30.0, 15.0, 22.0)));
        assert!(bounds.contains(Vec3::new(20.0, 25.0, 18.0)));
        assert!(bounds.contains(Vec3::new(25.0, 20.0, 20.0)));
        assert!(bounds.contains(Vec3::new(17.0, 12.0, 15.0)), "min corner");
        assert!(bounds.contains(Vec3::new(33.0, 28.0, 25.0)), "max corner");
        assert!(
            bounds.contains(Vec3::new(33.0, 20.0, 20.0)),
            "a point on a face"
        );
        assert!(!bounds.contains(Vec3::ZERO), "the origin is outside");
        for outside in [
            Vec3::new(16.99, 20.0, 20.0),
            Vec3::new(33.01, 20.0, 20.0),
            Vec3::new(25.0, 11.99, 20.0),
            Vec3::new(25.0, 28.01, 20.0),
            Vec3::new(25.0, 20.0, 14.99),
            Vec3::new(25.0, 20.0, 25.01),
        ] {
            assert!(!bounds.contains(outside), "{outside:?}");
        }
    }

    #[test]
    fn flight_bounds_around_one_point_with_no_margin_contain_only_it() {
        let point = Vec3::new(1.0, 2.0, 3.0);

        let bounds = FlightBounds::around([point], 0.0);

        assert!(bounds.contains(point));
        assert!(!bounds.contains(point + Vec3::X * 0.01));
        assert!(!bounds.contains(Vec3::ZERO));
        assert_eq!((bounds.min, bounds.max), (point, point));
    }

    #[test]
    fn reach_is_the_box_diagonal() {
        let flat = FlightBounds::around([Vec3::new(1.0, 1.0, 1.0), Vec3::new(4.0, 5.0, 13.0)], 0.0);
        let grown =
            FlightBounds::around([Vec3::new(1.0, 1.0, 1.0), Vec3::new(4.0, 5.0, 13.0)], 2.0);

        assert!(near(flat.reach(), 13.0, 1e-5), "{}", flat.reach());
        assert!(
            near(grown.reach(), 369.0_f32.sqrt(), 1e-4),
            "{}",
            grown.reach()
        );
    }

    proptest! {
        #[test]
        fn without_gravity_speed_never_grows(
            vx in -1000.0_f32..1000.0,
            vy in -1000.0_f32..1000.0,
            vz in -1000.0_f32..1000.0,
            drag in 0.0_f32..200.0,
            dt in 0.001_f32..0.1,
        ) {
            let ballistics = ball(940.0, drag);
            let mut flight = Flight {
                position: Vec3::ZERO,
                velocity: Vec3::new(vx, vy, vz),
            };

            for _ in 0..10 {
                let next = flight.step(&ballistics, 0.0, dt);

                let kept = (1.0 - drag * dt).max(0.0);
                prop_assert!(
                    next.velocity.length() <= flight.velocity.length() * kept * (1.0 + 1e-5),
                    "{:?} -> {:?}",
                    flight.velocity,
                    next.velocity
                );
                flight = next;
            }
        }

        #[test]
        fn a_larger_gravity_scale_drops_a_round_further(
            low in 0.0_f32..100.0,
            extra in 0.1_f32..100.0,
            drag in 0.0_f32..3.0,
        ) {
            let ballistics = ball(940.0, drag);
            let start = Flight::fired(Vec3::new(0.0, 1.63, 0.0), Vec3::Z, &ballistics);
            let fly = |scale_value: f32| {
                let gravity = scale(scale_value).gravity();
                (0..30).fold(start, |flight, _| flight.step(&ballistics, gravity, DT))
            };

            let gentle = fly(low);
            let steep = fly(low + extra);

            prop_assert!(
                steep.position.y < gentle.position.y,
                "scale {} fell to {}, scale {} to {}",
                low + extra,
                steep.position.y,
                low,
                gentle.position.y
            );
        }
    }
}
