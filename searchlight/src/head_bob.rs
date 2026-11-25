//! Head bob: the view dips with each footstep and sways with each stride. It is view-only state
//! the game steps after the sim, so it never moves the aim ray or the player's hit volumes.

use std::f32::consts::{PI, TAU};

use glam::Vec3;

use crate::lean::lean_offset;

/// Metres walked per footstep; one footstep is half a turn of the phase.
pub const STEP_LENGTH: f32 = 1.5;
/// How far the eye dips at the bottom of a footstep, in metres, at full level and walking speed.
pub const DIP: f32 = 0.04;
/// How far the eye sways sideways at the peak of a stride, in metres, at full level and walking
/// speed.
pub const SWAY: f32 = 0.025;
/// How far the eye rises at the top of a footstep's loop, in metres, at full level and walking
/// speed.
pub const RISE: f32 = 0.015;
/// The most the amplitude reaches, as a multiple of walking speed; a guard against a landing
/// spike.
pub const MAX_AMPLITUDE: f32 = 2.0;

/// The head-bob setting: a percentage from 0 to 100 on a grid of 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadBobLevel(u8);

/// A head-bob percentage that is outside 0 to 100 or off the grid of 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("head bob must be 0 to 100 in steps of 5")]
pub struct HeadBobOutOfRange;

impl HeadBobLevel {
    /// The level a fresh install starts at.
    pub const DEFAULT: Self = Self(50);
    /// The name of the saved entry and of the Settings slider.
    pub const KEY: &'static str = "head_bob";
    /// The lowest percentage.
    pub const MIN: i32 = 0;
    /// The highest percentage.
    pub const MAX: i32 = 100;
    /// The grid the percentage sits on.
    pub const STEP: i32 = 5;

    /// The level for `percent`, which must be 0 to 100 and a multiple of 5.
    pub fn new(percent: i32) -> Result<Self, HeadBobOutOfRange> {
        match u8::try_from(percent) {
            Ok(value) if percent <= Self::MAX && percent % Self::STEP == 0 => Ok(Self(value)),
            _ => Err(HeadBobOutOfRange),
        }
    }

    /// The level nearest `percent`, clamped to 0 to 100 and snapped to the grid of 5.
    pub fn nearest(percent: i32) -> Self {
        let clamped = percent.clamp(Self::MIN, Self::MAX);
        // Halves round up, so 22 snaps to 20 and 23 to 25.
        let snapped = (clamped + Self::STEP / 2) / Self::STEP * Self::STEP;
        Self(u8::try_from(snapped).expect("a clamped percentage fits in a byte"))
    }

    /// The level as a share of full bob, 0.0 to 1.0.
    pub fn fraction(self) -> f32 {
        f32::from(self.0) / 100.0
    }

    /// The level as a percentage.
    pub fn percent(self) -> i32 {
        i32::from(self.0)
    }
}

impl std::fmt::Display for HeadBobLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} %", self.0)
    }
}

/// How far into a step the view is and how strongly it bobs, as of one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bob {
    /// Radians into the stride, in `[0, 2 pi)`; a footstep is pi.
    pub phase: f32,
    /// The bob as a multiple of the walking bob: 0 standing or airborne, 1 at walking speed.
    pub amplitude: f32,
}

impl Bob {
    /// No bob, at the start of a stride.
    pub const REST: Self = Self {
        phase: 0.0,
        amplitude: 0.0,
    };

    /// The bob one tick of `dt` seconds later for a player moving `horizontal_speed` m/s along
    /// the ground, where `max_speed` m/s is a full walk. An airborne player has no bob.
    #[must_use]
    pub fn step(self, grounded: bool, horizontal_speed: f32, max_speed: f32, dt: f32) -> Self {
        let speed = if grounded { horizontal_speed } else { 0.0 };
        Self {
            phase: (self.phase + PI * speed * dt / STEP_LENGTH) % TAU,
            amplitude: (speed / max_speed).min(MAX_AMPLITUDE),
        }
    }

    /// Where the eye moves from the unbobbed eye of a player facing `yaw`, at `level`.
    #[must_use]
    pub fn offset(self, level: HeadBobLevel, yaw: f32) -> Vec3 {
        let scale = self.amplitude * level.fraction();
        let sin = self.phase.sin();
        Vec3::new(0.0, -DIP * scale * sin * sin, 0.0) + lean_offset(yaw, SWAY * scale * sin)
    }

    /// The bob `alpha` (0 to 1) of the way from `self` to `next`, the phase moving forward
    /// around the stride.
    #[must_use]
    pub fn blend(self, next: Self, alpha: f32) -> Self {
        let forward = (next.phase - self.phase).rem_euclid(TAU);
        Self {
            phase: (self.phase + alpha * forward) % TAU,
            amplitude: self.amplitude + (next.amplitude - self.amplitude) * alpha,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    use proptest::prelude::*;

    use super::*;
    use crate::lean::lean_offset;

    const DT: f32 = 1.0 / 60.0;
    const WALK: f32 = 6.0;

    fn level(percent: i32) -> HeadBobLevel {
        HeadBobLevel::new(percent).expect("a percentage on the grid")
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-5
    }

    /// Phases around a whole stride, from 0 up to just short of a turn.
    fn phases() -> impl Iterator<Item = f32> {
        (0..64).map(|i| i as f32 * TAU / 64.0)
    }

    #[test]
    fn new_accepts_exactly_the_grid() {
        for percent in -10..=110 {
            let made = HeadBobLevel::new(percent);

            if (0..=100).contains(&percent) && percent % 5 == 0 {
                assert_eq!(made.map(HeadBobLevel::percent), Ok(percent), "{percent}");
            } else {
                assert_eq!(made, Err(HeadBobOutOfRange), "{percent}");
            }
        }
        assert_eq!(HeadBobLevel::new(i32::MIN), Err(HeadBobOutOfRange));
        assert_eq!(HeadBobLevel::new(i32::MAX), Err(HeadBobOutOfRange));
    }

    #[test]
    fn nearest_snaps_to_the_closest_step_and_clamps() {
        let cases = [
            (23, 25),
            (22, 20),
            (27, 25),
            (28, 30),
            (25, 25),
            (0, 0),
            (100, 100),
            (98, 100),
            (150, 100),
            (-5, 0),
            (-1, 0),
            (i32::MIN, 0),
            (i32::MAX, 100),
        ];

        for (asked, expected) in cases {
            assert_eq!(
                HeadBobLevel::nearest(asked).percent(),
                expected,
                "{asked} should snap to {expected}"
            );
        }
    }

    proptest! {
        #[test]
        fn nearest_is_always_on_the_grid_and_in_range(percent in any::<i32>()) {
            let level = HeadBobLevel::nearest(percent);

            prop_assert!((0..=100).contains(&level.percent()));
            prop_assert_eq!(level.percent() % 5, 0);
            prop_assert_eq!(HeadBobLevel::new(level.percent()), Ok(level));
        }
    }

    #[test]
    fn the_fraction_is_the_percent_over_a_hundred_and_the_default_is_half() {
        assert_eq!(level(0).fraction(), 0.0);
        assert_eq!(level(25).fraction(), 0.25);
        assert_eq!(level(50).fraction(), 0.5);
        assert_eq!(level(100).fraction(), 1.0);
        assert_eq!(HeadBobLevel::DEFAULT, level(50));
        assert_eq!(HeadBobLevel::KEY, "head_bob");
    }

    #[test]
    fn standing_still_gives_no_offset_at_any_phase() {
        for phase in phases() {
            for yaw in [0.0, 1.0] {
                let bob = Bob {
                    phase,
                    amplitude: 1.0,
                }
                .step(true, 0.0, WALK, DT);

                assert_eq!(bob.amplitude, 0.0, "phase {phase}");
                assert!(
                    bob.offset(level(100), yaw).length() < 1e-6,
                    "phase {phase}: {:?}",
                    bob.offset(level(100), yaw)
                );
            }
        }
    }

    #[test]
    fn airborne_gives_no_offset_at_any_speed() {
        for speed in [0.0, 3.0, WALK, 2.0 * WALK, 10.0 * WALK] {
            for phase in phases() {
                let bob = Bob {
                    phase,
                    amplitude: 1.0,
                }
                .step(false, speed, WALK, DT);

                assert_eq!(bob.amplitude, 0.0, "speed {speed}, phase {phase}");
                assert!(
                    bob.offset(level(100), 0.5).length() < 1e-6,
                    "speed {speed}, phase {phase}"
                );
            }
        }
    }

    #[test]
    fn half_speed_gives_half_amplitude_and_half_step_rate() {
        let full = Bob::REST.step(true, WALK, WALK, DT);
        let half = Bob::REST.step(true, WALK / 2.0, WALK, DT);

        assert!(close(full.amplitude, 1.0), "{full:?}");
        assert!(close(half.amplitude, 0.5), "{half:?}");
        assert!(close(full.phase, PI * WALK * DT / STEP_LENGTH), "{full:?}");
        assert!(close(half.phase, full.phase / 2.0), "{half:?} vs {full:?}");
    }

    #[test]
    fn a_footstep_is_one_step_length_of_ground() {
        let mut bob = Bob::REST;

        for _ in 0..60 {
            bob = bob.step(true, STEP_LENGTH, WALK, DT);
        }

        assert!(
            (bob.phase - PI).abs() < 1e-4,
            "a second at {STEP_LENGTH} m/s: {bob:?}"
        );
    }

    #[test]
    fn amplitude_is_capped_at_one_and_a_half_walks() {
        let brisk = Bob::REST.step(true, 1.25 * WALK, WALK, DT);
        let fast = Bob::REST.step(true, 2.0 * WALK, WALK, DT);
        let faster = Bob::REST.step(true, 10.0 * WALK, WALK, DT);

        assert!(close(brisk.amplitude, 1.25), "{brisk:?}");
        assert!(close(fast.amplitude, 1.5), "{fast:?}");
        assert!(close(faster.amplitude, 1.5), "{faster:?}");
    }

    #[test]
    fn the_phase_wraps_to_the_start_of_the_stride() {
        let near_the_end = Bob {
            phase: TAU - 0.01,
            amplitude: 1.0,
        };

        let next = near_the_end.step(true, WALK, WALK, DT);

        let advance = PI * WALK * DT / STEP_LENGTH;
        assert!((0.0..TAU).contains(&next.phase), "{next:?}");
        assert!(close(next.phase, advance - 0.01), "{next:?}");
    }

    proptest! {
        #[test]
        fn a_step_keeps_the_phase_within_one_turn(
            phase in 0.0..TAU,
            speed in 0.0f32..30.0,
            grounded in any::<bool>(),
        ) {
            let next = Bob { phase, amplitude: 1.0 }.step(grounded, speed, WALK, DT);

            prop_assert!((0.0..TAU).contains(&next.phase), "{next:?}");
        }
    }

    proptest! {
        #[test]
        fn each_footstep_sways_only_to_its_own_side(
            phase in 0.01f32..(PI - 0.01),
            yaw in -TAU..TAU,
            amplitude in 0.1f32..1.5,
        ) {
            let right = lean_offset(yaw, 1.0);
            let first = Bob { phase, amplitude }.offset(level(100), yaw);
            let second = Bob { phase: phase + PI, amplitude }.offset(level(100), yaw);

            prop_assert!(first.dot(right) > 0.0, "first footstep {first:?}");
            prop_assert!(second.dot(right) < 0.0, "second footstep {second:?}");
        }

        #[test]
        fn the_eye_is_at_rest_at_the_start_of_each_footstep(
            yaw in -TAU..TAU,
            amplitude in 0.0f32..1.5,
        ) {
            for phase in [0.0, PI] {
                let offset = Bob { phase, amplitude }.offset(level(100), yaw);

                prop_assert!(offset.length() < 1e-5, "phase {phase}: {offset:?}");
            }
        }
    }

    #[test]
    fn each_footstep_loop_rises_then_falls() {
        let height = |phase| {
            Bob {
                phase,
                amplitude: 1.0,
            }
            .offset(level(100), 0.4)
            .y
        };

        assert!(height(PI / 4.0) > 0.0, "{}", height(PI / 4.0));
        assert!(height(5.0 * PI / 4.0) > 0.0, "{}", height(5.0 * PI / 4.0));
        assert!(height(3.0 * PI / 4.0) < 0.0, "{}", height(3.0 * PI / 4.0));
        assert!(height(7.0 * PI / 4.0) < 0.0, "{}", height(7.0 * PI / 4.0));
    }

    #[test]
    fn the_figure_eight_is_wider_than_tall() {
        let yaw = 0.7;
        let right = lean_offset(yaw, 1.0);
        let offsets: Vec<Vec3> = phases()
            .map(|phase| {
                Bob {
                    phase,
                    amplitude: 1.0,
                }
                .offset(level(100), yaw)
            })
            .collect();

        let range = |values: Vec<f32>| {
            let high = values.iter().copied().fold(f32::MIN, f32::max);
            let low = values.iter().copied().fold(f32::MAX, f32::min);
            high - low
        };
        let sideways = range(offsets.iter().map(|o| o.dot(right)).collect());
        let vertical = range(offsets.iter().map(|o| o.y).collect());

        assert!(vertical > 0.0, "the eye must move up and down");
        assert!(sideways > vertical, "{sideways} wide, {vertical} tall");
    }

    #[test]
    fn a_full_bob_traces_the_figure_eight_in_the_view_frame() {
        let bob = |phase| Bob {
            phase,
            amplitude: 1.0,
        };
        let expected = |yaw, phase: f32| {
            lean_offset(yaw, SWAY * phase.sin()) + Vec3::new(0.0, RISE * (2.0 * phase).sin(), 0.0)
        };

        for yaw in [0.0, 0.7, 3.0] {
            for phase in [PI / 4.0, FRAC_PI_2, 3.0 * PI / 4.0, PI, 3.0 * FRAC_PI_2] {
                let actual = bob(phase).offset(level(100), yaw);

                assert!(
                    near(actual, expected(yaw, phase)),
                    "yaw {yaw}, phase {phase}: {actual:?}"
                );
            }
            let top = bob(PI / 4.0).offset(level(100), yaw);
            assert!(close(top.y, RISE), "{top:?}");
            assert!(close(
                top.dot(lean_offset(yaw, 1.0)),
                SWAY * (PI / 4.0).sin()
            ));
        }
        assert!(close(SWAY, 0.03), "SWAY is {SWAY}");
        assert!(close(RISE, 0.015), "RISE is {RISE}");
    }

    #[test]
    fn the_amplitude_and_the_level_scale_the_offset_linearly() {
        let full = Bob {
            phase: PI / 4.0,
            amplitude: 1.0,
        }
        .offset(level(100), 0.3);
        let half_amplitude = Bob {
            phase: PI / 4.0,
            amplitude: 0.5,
        }
        .offset(level(100), 0.3);

        let quarter_level = Bob {
            phase: PI / 4.0,
            amplitude: 1.0,
        }
        .offset(level(25), 0.3);
        let half_level = Bob {
            phase: PI / 4.0,
            amplitude: 1.0,
        }
        .offset(level(50), 0.3);

        assert!(full.length() > 0.01, "{full:?}");
        assert!(near(half_amplitude, full * 0.5), "{half_amplitude:?}");
        assert!(near(half_level, full * 0.5), "{half_level:?}");
        assert!(near(quarter_level, full * 0.25), "{quarter_level:?}");
    }

    #[test]
    fn zero_level_gives_no_offset() {
        for phase in phases() {
            let bob = Bob {
                phase,
                amplitude: MAX_AMPLITUDE,
            };

            assert!(
                bob.offset(level(0), 1.2).length() < 1e-6,
                "phase {phase}: {:?}",
                bob.offset(level(0), 1.2)
            );
        }
        let moving = Bob::REST.step(true, WALK, WALK, DT);
        assert!(moving.amplitude > 0.0, "the player is walking: {moving:?}");
    }

    #[test]
    fn blending_across_the_phase_wrap_moves_forward() {
        let before = Bob {
            phase: 6.2,
            amplitude: 0.2,
        };
        let after = Bob {
            phase: 0.1,
            amplitude: 0.6,
        };

        let halfway = before.blend(after, 0.5);
        let start = before.blend(after, 0.0);
        let end = before.blend(after, 1.0);

        let forward = (0.1 + TAU - 6.2) / 2.0;
        let expected = (6.2 + forward) % TAU;
        let apart = (halfway.phase - expected).abs();
        assert!(
            apart.min(TAU - apart) < 1e-4,
            "halfway through the wrap is {expected}, not {}",
            halfway.phase
        );
        assert!((0.0..TAU).contains(&halfway.phase), "{halfway:?}");
        assert!(close(halfway.amplitude, 0.4), "{halfway:?}");
        assert!(
            close(start.phase, 6.2) && close(start.amplitude, 0.2),
            "{start:?}"
        );
        let to_end = (end.phase - 0.1).abs();
        assert!(to_end.min(TAU - to_end) < 1e-4, "{end:?}");
        assert!(close(end.amplitude, 0.6), "{end:?}");
    }

    #[test]
    fn blending_without_a_wrap_is_a_plain_blend() {
        let before = Bob {
            phase: 1.0,
            amplitude: 0.0,
        };
        let after = Bob {
            phase: 1.4,
            amplitude: 1.0,
        };

        let quarter = before.blend(after, 0.25);

        assert!(close(quarter.phase, 1.1), "{quarter:?}");
        assert!(close(quarter.amplitude, 0.25), "{quarter:?}");
    }
}
