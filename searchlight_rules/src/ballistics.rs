//! A round in flight: its drag and muzzle velocity, the integrator that steps it, the gravity
//! scale and the box it may fly in.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Standard gravity in m/s², the pull on a round at gravity scale 1.
pub const STANDARD_GRAVITY: f32 = 9.81;

/// The largest gravity scale a tester may set.
pub const MAX_GRAVITY_SCALE: f32 = 1000.0;

/// Speed in m/s at or below which a flight is spent.
pub const MIN_SPEED: f32 = 100.0;

/// What a round is fired with: its speed at the muzzle and how quickly it slows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ballistics {
    /// Speed at the muzzle, in m/s.
    pub muzzle_velocity: f32,
    /// Fraction of its velocity a round loses per second, in 1/s.
    pub drag: f32,
}

/// A round's position and velocity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// Where the round is, in metres.
    pub position: Vec3,
    /// How fast and which way it moves, in m/s.
    pub velocity: Vec3,
}

impl Flight {
    /// A round leaving `origin` along `direction` at the muzzle velocity.
    pub fn fired(origin: Vec3, direction: Vec3, ballistics: &Ballistics) -> Self {
        Self {
            position: origin,
            velocity: direction.normalize_or_zero() * ballistics.muzzle_velocity,
        }
    }

    /// The flight one tick of `dt` seconds later, under drag and `gravity` in m/s².
    ///
    /// Uses only correctly rounded IEEE operations so a replay is exact (no `exp`, no `powf`).
    pub fn step(&self, ballistics: &Ballistics, gravity: f32, dt: f32) -> Self {
        let kept = (1.0 - ballistics.drag * dt).max(0.0);
        let velocity = self.velocity * kept - Vec3::Y * gravity * dt;
        Self {
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
#[error("gravity scale must be from 0 to {MAX_GRAVITY_SCALE}, got {value}")]
pub struct GravityScaleError {
    /// The value given.
    pub value: f32,
}

/// A tester's multiplier on the pull of gravity on rounds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct GravityScale(f32);

impl TryFrom<f32> for GravityScale {
    type Error = GravityScaleError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<GravityScale> for f32 {
    fn from(scale: GravityScale) -> Self {
        scale.0
    }
}

impl GravityScale {
    /// A scale of `value`.
    ///
    /// # Errors
    ///
    /// [`GravityScaleError`] when `value` is negative, not finite or above
    /// [`MAX_GRAVITY_SCALE`].
    pub fn new(value: f32) -> Result<Self, GravityScaleError> {
        if value.is_finite() && (0.0..=MAX_GRAVITY_SCALE).contains(&value) {
            Ok(Self(value))
        } else {
            Err(GravityScaleError { value })
        }
    }

    /// The pull on a round in m/s²: [`STANDARD_GRAVITY`] times the scale.
    pub fn gravity(self) -> f32 {
        STANDARD_GRAVITY * self.0
    }
}

impl Default for GravityScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// The box a round may fly in; a round that leaves it ends.
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
    pub fn contains(&self, point: Vec3) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }

    /// The length of the box's diagonal: the farthest a straight ray inside it can run.
    pub fn reach(&self) -> f32 {
        (self.max - self.min).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const DT: f32 = 1.0 / 60.0;

    fn ballistics(muzzle_velocity: f32, drag: f32) -> Ballistics {
        Ballistics {
            muzzle_velocity,
            drag,
        }
    }

    fn flight(position: Vec3, velocity: Vec3) -> Flight {
        Flight { position, velocity }
    }

    fn scale(value: f32) -> GravityScale {
        GravityScale::new(value).expect("the test scale is valid")
    }

    fn assert_near(actual: Vec3, expected: Vec3, tolerance: f32) {
        assert!(
            (actual - expected).abs().max_element() <= tolerance,
            "{actual} is not within {tolerance} of {expected}"
        );
    }

    #[test]
    fn a_step_without_gravity_or_drag_moves_at_the_muzzle_velocity() {
        let b = ballistics(940.0, 0.0);
        let origin = Vec3::new(1.0, 2.0, 3.0);
        let fired = Flight::fired(origin, Vec3::NEG_Z, &b);

        let next = fired.step(&b, 0.0, DT);

        assert_eq!(next.velocity, Vec3::new(0.0, 0.0, -940.0));
        assert_near(next.position, Vec3::new(1.0, 2.0, 3.0 - 940.0 / 60.0), 1e-4);
    }

    #[test]
    fn gravity_changes_only_the_vertical_velocity() {
        let b = ballistics(940.0, 0.0);
        let before = flight(Vec3::ZERO, Vec3::new(30.0, 40.0, -50.0));

        let next = before.step(&b, 9.81, 0.5);

        assert_eq!(next.velocity.x, 30.0);
        assert_eq!(next.velocity.z, -50.0);
        assert!(
            (next.velocity.y - (40.0 - 9.81 * 0.5)).abs() < 1e-4,
            "{:?}",
            next.velocity
        );
        assert_near(next.position, next.velocity * 0.5, 1e-4);
    }

    #[test]
    fn drag_past_one_step_stops_the_round_and_never_reverses_it() {
        let before = flight(Vec3::new(5.0, 6.0, 7.0), Vec3::new(100.0, 0.0, -50.0));

        for drag in [2.0, 10.0, 1000.0] {
            let next = before.step(&ballistics(940.0, drag), 0.0, 0.5);

            assert_eq!(next.velocity, Vec3::ZERO, "drag {drag}");
            assert_eq!(next.position, before.position, "drag {drag}");
        }
        let falling = before.step(&ballistics(940.0, 10.0), 9.81, 0.5);
        assert_eq!(falling.velocity.x, 0.0);
        assert_eq!(falling.velocity.z, 0.0);
        assert!(falling.velocity.y < 0.0, "{:?}", falling.velocity);
    }

    #[test]
    fn a_partial_drag_step_scales_velocity_linearly() {
        let before = flight(Vec3::ZERO, Vec3::new(100.0, 0.0, -50.0));

        let next = before.step(&ballistics(940.0, 1.0), 0.0, 0.5);

        assert_eq!(next.velocity, Vec3::new(50.0, 0.0, -25.0));
        assert_eq!(next.position, Vec3::new(25.0, 0.0, -12.5));
    }

    #[test]
    fn a_flight_at_the_minimum_speed_is_spent() {
        let at = |velocity| flight(Vec3::ZERO, velocity).is_spent();

        assert!(at(Vec3::new(MIN_SPEED, 0.0, 0.0)));
        assert!(at(Vec3::new(60.0, 0.0, -80.0)));
        assert!(at(Vec3::new(0.0, -99.9, 0.0)));
        assert!(at(Vec3::ZERO));
        assert!(!at(Vec3::new(100.01, 0.0, 0.0)));
        assert!(!at(Vec3::new(0.0, 0.0, -940.0)));
    }

    #[test]
    fn gravity_scale_accepts_0_and_refuses_negative_and_non_finite() {
        assert_eq!(scale(0.0).gravity(), 0.0);
        assert_eq!(scale(30.0).gravity(), STANDARD_GRAVITY * 30.0);
        for bad in [
            -0.5,
            -f32::MIN_POSITIVE,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert!(GravityScale::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_gravity_scale_read_from_data_is_checked_like_new() {
        for bad in [-5.0, 2000.0, 1000.5] {
            let read = serde_json::from_value::<GravityScale>(serde_json::json!(bad));

            assert!(read.is_err(), "{bad} was accepted: {read:?}");
        }

        let read = serde_json::from_value::<GravityScale>(serde_json::json!(30.0))
            .expect("30 is a valid scale");

        assert_eq!(read.gravity(), STANDARD_GRAVITY * 30.0);
    }

    #[test]
    fn a_gravity_scale_serialises_as_its_value_and_reads_back() {
        let thirty = scale(30.0);

        let written = serde_json::to_value(thirty).expect("a scale serialises");
        let read = serde_json::from_value::<GravityScale>(written.clone())
            .expect("what was written reads back");

        assert_eq!(written, serde_json::json!(30.0));
        assert_eq!(read, thirty);
    }

    #[test]
    fn gravity_scale_defaults_to_1() {
        assert_eq!(GravityScale::default(), scale(1.0));
        assert_eq!(GravityScale::default().gravity(), STANDARD_GRAVITY);
    }

    #[test]
    fn gravity_scale_refuses_more_than_1000() {
        assert_eq!(MAX_GRAVITY_SCALE, 1000.0);

        assert!(GravityScale::new(1000.0).is_ok());
        assert!(GravityScale::new(1000.1).is_err());
        assert!(GravityScale::new(f32::MAX).is_err());
    }

    #[test]
    fn round_gravity_is_standard_gravity_times_the_scale() {
        assert_eq!(STANDARD_GRAVITY, 9.81);

        assert_eq!(scale(1.0).gravity(), 9.81);
        assert_eq!(scale(30.0).gravity(), 9.81_f32 * 30.0);
    }

    #[test]
    fn a_fired_flight_has_the_muzzle_speed_along_a_non_unit_direction() {
        let b = ballistics(940.0, 1.0);
        let origin = Vec3::new(-4.0, 1.5, 9.0);

        let slanted = Flight::fired(origin, Vec3::new(3.0, 0.0, 4.0), &b);
        let down = Flight::fired(origin, Vec3::new(0.0, -7.0, 0.0), &b);

        assert_eq!(slanted.position, origin);
        assert!((slanted.velocity.length() - 940.0).abs() < 1e-2);
        assert_near(slanted.velocity, Vec3::new(564.0, 0.0, 752.0), 1e-2);
        assert_near(down.velocity, Vec3::new(0.0, -940.0, 0.0), 1e-2);
    }

    #[test]
    fn flight_bounds_contain_their_points_grown_by_the_margin() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 5.0, -20.0);

        let bounds = FlightBounds::around([a, b], 1.0);

        assert_eq!(bounds.min, Vec3::new(-1.0, -1.0, -21.0));
        assert_eq!(bounds.max, Vec3::new(11.0, 6.0, 1.0));
        assert!(bounds.contains(a));
        assert!(bounds.contains(b));
        assert!(
            bounds.contains(Vec3::new(11.0, 0.0, 0.0)),
            "on the max x face"
        );
        assert!(
            bounds.contains(Vec3::new(0.0, -1.0, 0.0)),
            "on the min y face"
        );
        assert!(
            bounds.contains(Vec3::new(0.0, 0.0, -21.0)),
            "on the min z face"
        );
        assert!(bounds.contains(bounds.min));
        assert!(bounds.contains(bounds.max));
        assert!(!bounds.contains(Vec3::new(11.5, 0.0, 0.0)));
        assert!(!bounds.contains(Vec3::new(-1.5, 0.0, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, 6.5, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, -1.5, 0.0)));
        assert!(!bounds.contains(Vec3::new(0.0, 0.0, 1.5)));
        assert!(!bounds.contains(Vec3::new(0.0, 0.0, -21.5)));
    }

    #[test]
    fn reach_is_the_box_diagonal() {
        let slab = FlightBounds {
            min: Vec3::ZERO,
            max: Vec3::new(3.0, 4.0, 12.0),
        };
        let cube = FlightBounds {
            min: Vec3::splat(-1.0),
            max: Vec3::splat(1.0),
        };

        assert_eq!(slab.reach(), 13.0);
        assert!((cube.reach() - 2.0 * 3.0_f32.sqrt()).abs() < 1e-5);
    }

    fn fly_for_20_ticks(direction: Vec3, drag: f32, gravity_scale: f32) -> Flight {
        let b = ballistics(940.0, drag);
        let gravity = scale(gravity_scale).gravity();
        let mut flight = Flight::fired(Vec3::ZERO, direction, &b);
        for _ in 0..20 {
            flight = flight.step(&b, gravity, DT);
        }
        flight
    }

    proptest! {
        #[test]
        fn without_gravity_speed_never_grows(
            x in -2000.0f32..2000.0,
            y in -2000.0f32..2000.0,
            z in -2000.0f32..2000.0,
            drag in 0.0f32..50.0,
            dt in 0.001f32..0.1,
        ) {
            let before = flight(Vec3::ZERO, Vec3::new(x, y, z));

            let after = before.step(&ballistics(940.0, drag), 0.0, dt);

            prop_assert!(after.velocity.length() <= before.velocity.length());
        }

        #[test]
        fn a_larger_gravity_scale_drops_a_round_further(
            low in 0.0f32..100.0,
            extra in 0.5f32..100.0,
            drag in 0.0f32..5.0,
            x in -1.0f32..1.0,
            y in -1.0f32..1.0,
        ) {
            let direction = Vec3::new(x, y, 1.0);

            let lighter = fly_for_20_ticks(direction, drag, low);
            let heavier = fly_for_20_ticks(direction, drag, low + extra);

            prop_assert!(heavier.position.y < lighter.position.y);
        }
    }
}
