//! Accuracy rules: how far the player's shots stray from the crosshair.
//!
//! All angles are in radians and a spread is the half-angle of the cone the shots leave in.

use serde::{Deserialize, Serialize};

/// How wide the cone is while moving and how quickly it closes once the player stops.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccuracyTuning {
    /// Spread at walking speed, in radians.
    pub walk_spread: f32,
    /// Seconds for the movement spread to close from walking to nothing.
    pub settle_seconds: f32,
}

/// The player's spread state, stepped once per tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Accuracy {
    movement: f32,
}

impl Accuracy {
    /// This state after `dt` seconds at `speed_ratio` times walking speed.
    #[must_use]
    pub fn step(
        self,
        _speed_ratio: f32,
        _grounded: bool,
        _dt: f32,
        _tuning: &AccuracyTuning,
    ) -> Accuracy {
        self
    }

    /// The cone's half-angle for the next shot, in radians.
    pub fn spread(self, _tuning: &AccuracyTuning) -> f32 {
        self.movement * 0.0
    }
}

/// The sideways and upward offsets `(a, b)` of a shot, as tangents off the view direction, from
/// two unit draws `u` and `v` in `[0, 1)`: uniform in a disc of radius `tan(spread)`.
pub fn disc_offset(_u: f32, _v: f32, spread: f32) -> (f32, f32) {
    (2.0 * spread.tan(), 0.0)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn tuning() -> AccuracyTuning {
        AccuracyTuning {
            walk_spread: 0.03,
            settle_seconds: 0.35,
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn spread_is_walk_spread_times_the_speed_ratio() {
        let tuning = tuning();
        let table = [0.0, 0.25, 0.5, 1.0, 1.25];

        assert_eq!(
            Accuracy::default().spread(&tuning),
            0.0,
            "a settled player has no spread"
        );
        for ratio in table {
            let spread = Accuracy::default()
                .step(ratio, true, 1.0 / 60.0, &tuning)
                .spread(&tuning);

            assert_close(spread, tuning.walk_spread * ratio);
        }
    }

    #[test]
    fn movement_spread_falls_to_zero_in_settle_seconds() {
        let tuning = tuning();
        let dt = tuning.settle_seconds / 10.0;
        let mut accuracy = Accuracy::default().step(1.0, true, dt, &tuning);

        let mut spreads = Vec::new();
        for _ in 0..10 {
            accuracy = accuracy.step(0.0, true, dt, &tuning);
            spreads.push(accuracy.spread(&tuning));
        }

        for (index, spread) in spreads.iter().enumerate() {
            let remaining = (9 - index) as f32 / 10.0;
            assert_close(*spread, tuning.walk_spread * remaining);
        }
        assert_eq!(spreads[9], 0.0, "exact after the settle time");
    }

    #[test]
    fn a_step_longer_than_the_settle_time_stops_at_zero_not_below() {
        let tuning = tuning();
        let walking = Accuracy::default().step(1.0, true, 1.0 / 60.0, &tuning);

        let stopped = walking.step(0.0, true, tuning.settle_seconds * 5.0, &tuning);

        assert_eq!(stopped.spread(&tuning), 0.0);
    }

    #[test]
    fn a_slower_walk_falls_at_the_same_rate_as_a_full_one() {
        let tuning = tuning();
        let dt = 0.05;
        let walking = Accuracy::default().step(1.0, true, dt, &tuning);

        let slowed = walking.step(0.5, true, dt, &tuning);

        let fall = tuning.walk_spread / tuning.settle_seconds * dt;
        assert_close(slowed.spread(&tuning), tuning.walk_spread - fall);
    }

    #[test]
    fn speed_ratio_caps_at_one_and_a_half_and_airborne_reads_at_least_walking() {
        let tuning = tuning();
        let dt = 1.0 / 60.0;
        let spread = |ratio, grounded| {
            Accuracy::default()
                .step(ratio, grounded, dt, &tuning)
                .spread(&tuning)
        };

        assert_close(spread(3.0, true), tuning.walk_spread * 1.5);
        assert_close(spread(1.5, true), tuning.walk_spread * 1.5);
        assert_close(spread(0.0, false), tuning.walk_spread);
        assert_close(spread(0.4, false), tuning.walk_spread);
        assert_close(spread(1.25, false), tuning.walk_spread * 1.25);
        assert_close(spread(3.0, false), tuning.walk_spread * 1.5);
    }

    #[test]
    fn the_draw_maps_radius_by_the_square_root_and_angle_by_a_full_turn() {
        let spread = 0.1_f32;
        let radius = spread.tan();

        let (a0, b0) = disc_offset(0.25, 0.0, spread);
        let (a1, b1) = disc_offset(0.25, 0.25, spread);
        let (a2, b2) = disc_offset(0.25, 0.5, spread);

        assert_close(a0, radius * 0.5);
        assert_close(b0, 0.0);
        assert_close(a1, 0.0);
        assert_close(b1, radius * 0.5);
        assert_close(a2, -radius * 0.5);
        assert_close(b2, 0.0);
    }

    proptest! {
        #[test]
        fn every_draw_lies_inside_the_cone(
            u in 0.0f32..1.0,
            v in 0.0f32..1.0,
            spread in 0.0f32..0.2,
        ) {
            let (a, b) = disc_offset(u, v, spread);

            let limit = spread.tan().powi(2);
            prop_assert!(a * a + b * b <= limit * (1.0 + 1e-5), "({a}, {b}) outside {spread}");
        }

        #[test]
        fn a_spread_of_zero_gives_exactly_the_crosshair(u in 0.0f32..1.0, v in 0.0f32..1.0) {
            prop_assert_eq!(disc_offset(u, v, 0.0), (0.0, 0.0));
        }
    }
}
