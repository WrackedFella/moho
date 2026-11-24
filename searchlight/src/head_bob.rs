//! Head bob: the view dips with each footstep and sways with each stride. It is view-only state
//! the game steps after the sim, so it never moves the aim ray or the player's hit volumes.

use glam::Vec3;

/// Metres walked per footstep; one footstep is half a turn of the phase.
pub const STEP_LENGTH: f32 = 1.5;
/// How far the eye dips at the bottom of a footstep, in metres, at full level and walking speed.
pub const DIP: f32 = 0.04;
/// How far the eye sways sideways at the peak of a stride, in metres, at full level and walking
/// speed.
pub const SWAY: f32 = 0.025;
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

    /// The level for `percent`, which must be 0 to 100 and a multiple of 5.
    pub fn new(_percent: i32) -> Result<Self, HeadBobOutOfRange> {
        todo!("HeadBobLevel::new")
    }

    /// The level nearest `percent`, clamped to 0 to 100 and snapped to the grid of 5.
    pub fn nearest(_percent: i32) -> Self {
        todo!("HeadBobLevel::nearest")
    }

    /// The level as a share of full bob, 0.0 to 1.0.
    pub fn fraction(self) -> f32 {
        todo!("HeadBobLevel::fraction")
    }

    /// The level as a percentage.
    pub fn percent(self) -> i32 {
        todo!("HeadBobLevel::percent")
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
    pub fn step(self, _grounded: bool, _horizontal_speed: f32, _max_speed: f32, _dt: f32) -> Self {
        todo!("Bob::step")
    }

    /// Where the eye moves from the unbobbed eye of a player facing `yaw`, at `level`.
    #[must_use]
    pub fn offset(self, _level: HeadBobLevel, _yaw: f32) -> Vec3 {
        todo!("Bob::offset")
    }

    /// The bob `alpha` (0 to 1) of the way from `self` to `next`, the phase moving forward
    /// around the stride.
    #[must_use]
    pub fn blend(self, _next: Self, _alpha: f32) -> Self {
        todo!("Bob::blend")
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
    fn amplitude_is_capped_at_twice_a_full_walk() {
        let fast = Bob::REST.step(true, 2.0 * WALK, WALK, DT);
        let faster = Bob::REST.step(true, 10.0 * WALK, WALK, DT);

        assert!(close(fast.amplitude, MAX_AMPLITUDE), "{fast:?}");
        assert!(close(faster.amplitude, MAX_AMPLITUDE), "{faster:?}");
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

    #[test]
    fn offset_is_zero_at_phase_zero_and_never_above_the_eye() {
        for yaw in [0.0, 0.7, 3.0] {
            let start = Bob {
                phase: 0.0,
                amplitude: 1.0,
            };

            assert!(
                start.offset(level(100), yaw).length() < 1e-6,
                "yaw {yaw}: {:?}",
                start.offset(level(100), yaw)
            );
        }
        for phase in phases() {
            let offset = Bob {
                phase,
                amplitude: 2.0,
            }
            .offset(level(100), 0.4);

            assert!(
                offset.y <= 1e-6,
                "phase {phase}: the eye rose to {offset:?}"
            );
        }
    }

    #[test]
    fn a_full_bob_dips_once_per_step_and_sways_once_per_stride_in_the_view_frame() {
        let bob = |phase| Bob {
            phase,
            amplitude: 1.0,
        };

        for yaw in [0.0, 0.7, 3.0] {
            let quarter = bob(FRAC_PI_2).offset(level(100), yaw);
            let half = bob(PI).offset(level(100), yaw);
            let three_quarters = bob(3.0 * FRAC_PI_2).offset(level(100), yaw);

            assert!(
                near(quarter, lean_offset(yaw, SWAY) + Vec3::new(0.0, -DIP, 0.0)),
                "yaw {yaw}: {quarter:?}"
            );
            assert!(near(half, Vec3::ZERO), "yaw {yaw}: {half:?}");
            assert!(
                near(
                    three_quarters,
                    lean_offset(yaw, -SWAY) + Vec3::new(0.0, -DIP, 0.0)
                ),
                "yaw {yaw}: {three_quarters:?}"
            );
        }
    }

    #[test]
    fn the_amplitude_and_the_level_scale_the_offset_linearly() {
        let full = Bob {
            phase: FRAC_PI_2,
            amplitude: 1.0,
        }
        .offset(level(100), 0.3);
        let half_amplitude = Bob {
            phase: FRAC_PI_2,
            amplitude: 0.5,
        }
        .offset(level(100), 0.3);

        let quarter_level = Bob {
            phase: FRAC_PI_2,
            amplitude: 1.0,
        }
        .offset(level(25), 0.3);
        let half_level = Bob {
            phase: FRAC_PI_2,
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
