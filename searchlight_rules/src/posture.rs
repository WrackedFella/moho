//! Posture rules: how far the player leans around a corner and how fast the lean blends.

use serde::{Deserialize, Serialize};

/// The side a lean is held toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeanSide {
    Left,
    Right,
}

/// How a full lean looks and how quickly it comes and goes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeanTuning {
    /// Sideways shift of a full lean, in metres.
    pub shift: f32,
    /// Roll of a full lean, in radians.
    pub roll: f32,
    /// Seconds from upright to a full lean; not above zero snaps.
    pub blend_seconds: f32,
    /// Fraction of walking speed while a lean is held.
    pub speed_factor: f32,
}

/// How much of a full lean each side has room for, each in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeanReach {
    pub left: f32,
    pub right: f32,
}

impl LeanReach {
    /// Open space on both sides.
    pub const FULL: Self = Self {
        left: 1.0,
        right: 1.0,
    };
}

/// How far the player leans, in units of a full lean: negative left, positive right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lean {
    amount: f32,
}

impl Lean {
    /// The signed amount in `[-1, 1]`.
    pub fn amount(self) -> f32 {
        self.amount
    }

    /// This lean after `dt` seconds of wanting `want`, held within `reach`.
    #[must_use]
    pub fn step(
        self,
        want: Option<LeanSide>,
        reach: LeanReach,
        dt: f32,
        tuning: &LeanTuning,
    ) -> Lean {
        let target = match want {
            None => 0.0,
            Some(LeanSide::Left) => -1.0,
            Some(LeanSide::Right) => 1.0,
        };
        let remaining = target - self.amount;
        let max_step = if tuning.blend_seconds > 0.0 {
            // Never negative or NaN, which would make the clamp below panic.
            (dt / tuning.blend_seconds).max(0.0)
        } else {
            f32::INFINITY
        };
        let amount = self.amount + remaining.clamp(-max_step, max_step);
        // A reach outside [0, 1] or NaN is read as the nearest bound, so the clamp cannot panic.
        let room = |side: f32| {
            if side.is_nan() {
                0.0
            } else {
                side.clamp(0.0, 1.0)
            }
        };
        Lean {
            amount: amount.clamp(-room(reach.left), room(reach.right)),
        }
    }

    /// The sideways shift in metres, positive right.
    pub fn shift(self, tuning: &LeanTuning) -> f32 {
        self.amount * tuning.shift
    }

    /// The roll in radians, positive right.
    pub fn roll(self, tuning: &LeanTuning) -> f32 {
        self.amount * tuning.roll
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn tuning() -> LeanTuning {
        LeanTuning {
            shift: 0.35,
            roll: 12.0_f32.to_radians(),
            blend_seconds: 0.25,
            speed_factor: 0.5,
        }
    }

    #[test]
    fn a_reach_outside_zero_to_one_or_nan_is_read_as_the_nearest_bound() {
        let settled = |reach| Lean::default().step(Some(LeanSide::Left), reach, 1.0, &tuning());

        let negative = settled(LeanReach {
            left: -0.5,
            right: 0.0,
        });
        let nan = settled(LeanReach {
            left: f32::NAN,
            right: f32::NAN,
        });
        let wide = settled(LeanReach {
            left: 2.0,
            right: 2.0,
        });

        assert_eq!(negative.amount(), 0.0);
        assert_eq!(nan.amount(), 0.0);
        assert_eq!(wide.amount(), -1.0);
    }

    const DT: f32 = 1.0 / 60.0;

    /// A lean that has settled fully toward `side`.
    fn settled(side: LeanSide) -> Lean {
        Lean::default().step(Some(side), LeanReach::FULL, 10.0, &tuning())
    }

    fn any_tuning() -> impl Strategy<Value = LeanTuning> {
        (0.05f32..1.0).prop_map(|blend_seconds| LeanTuning {
            blend_seconds,
            ..tuning()
        })
    }

    fn any_side() -> impl Strategy<Value = Option<LeanSide>> {
        prop_oneof![
            Just(None),
            Just(Some(LeanSide::Left)),
            Just(Some(LeanSide::Right)),
        ]
    }

    fn any_reach() -> impl Strategy<Value = LeanReach> {
        (0.0f32..=1.0, 0.0f32..=1.0).prop_map(|(left, right)| LeanReach { left, right })
    }

    #[test]
    fn one_step_moves_part_way_toward_the_side() {
        let right = Lean::default().step(Some(LeanSide::Right), LeanReach::FULL, DT, &tuning());
        let left = Lean::default().step(Some(LeanSide::Left), LeanReach::FULL, DT, &tuning());

        assert!((right.amount() - DT / 0.25).abs() < 1e-5, "{right:?}");
        assert!((left.amount() + DT / 0.25).abs() < 1e-5, "{left:?}");
    }

    #[test]
    fn holding_a_side_settles_at_a_full_lean_and_no_further() {
        let mut lean = Lean::default();

        for _ in 0..120 {
            lean = lean.step(Some(LeanSide::Left), LeanReach::FULL, DT, &tuning());
        }

        assert_eq!(lean.amount(), -1.0);
    }

    #[test]
    fn releasing_blends_back_to_exactly_upright() {
        let mut lean = settled(LeanSide::Right);

        lean = lean.step(None, LeanReach::FULL, DT, &tuning());
        let after_one = lean.amount();
        for _ in 0..120 {
            lean = lean.step(None, LeanReach::FULL, DT, &tuning());
        }

        assert!((after_one - (1.0 - DT / 0.25)).abs() < 1e-5, "{after_one}");
        assert_eq!(lean.amount(), 0.0);
    }

    #[test]
    fn a_non_positive_blend_time_snaps_to_the_side() {
        for blend_seconds in [0.0, -1.0] {
            let snap = LeanTuning {
                blend_seconds,
                ..tuning()
            };

            let lean = Lean::default().step(Some(LeanSide::Right), LeanReach::FULL, DT, &snap);
            let back = lean.step(None, LeanReach::FULL, DT, &snap);

            assert_eq!(lean.amount(), 1.0, "blend {blend_seconds}");
            assert_eq!(back.amount(), 0.0, "blend {blend_seconds}");
        }
    }

    #[test]
    fn a_limited_reach_holds_the_lean_at_that_fraction() {
        let reach = LeanReach {
            left: 0.4,
            right: 0.6,
        };
        let mut left = Lean::default();
        let mut right = Lean::default();

        for _ in 0..120 {
            left = left.step(Some(LeanSide::Left), reach, DT, &tuning());
            right = right.step(Some(LeanSide::Right), reach, DT, &tuning());
        }

        assert_eq!(left.amount(), -0.4);
        assert_eq!(right.amount(), 0.6);
    }

    #[test]
    fn a_reach_that_closes_pulls_a_full_lean_in_at_once() {
        let full = settled(LeanSide::Left);
        let closed = LeanReach {
            left: 0.25,
            right: 1.0,
        };

        let lean = full.step(Some(LeanSide::Left), closed, DT, &tuning());

        assert_eq!(lean.amount(), -0.25);
    }

    #[test]
    fn shift_and_roll_are_linear_in_the_amount_and_positive_to_the_right() {
        let tuning = tuning();
        let half = Lean::default().step(Some(LeanSide::Right), LeanReach::FULL, 0.125, &tuning);

        let cases = [
            (settled(LeanSide::Right), 1.0),
            (settled(LeanSide::Left), -1.0),
            (half, 0.5),
            (Lean::default(), 0.0),
        ];
        for (lean, fraction) in cases {
            assert!(
                (lean.shift(&tuning) - 0.35 * fraction).abs() < 1e-5,
                "{lean:?}: shift {}",
                lean.shift(&tuning)
            );
            assert!(
                (lean.roll(&tuning) - 12.0_f32.to_radians() * fraction).abs() < 1e-5,
                "{lean:?}: roll {}",
                lean.roll(&tuning)
            );
        }
    }

    proptest! {
        #[test]
        fn the_amount_stays_within_one_and_within_reach(
            tuning in any_tuning(),
            wants in proptest::collection::vec((any_side(), any_reach(), 0.001f32..0.5), 1..40),
        ) {
            let mut lean = Lean::default();

            for (want, reach, dt) in wants {
                lean = lean.step(want, reach, dt, &tuning);

                prop_assert!((-1.0..=1.0).contains(&lean.amount()), "{lean:?}");
                prop_assert!(lean.amount() >= -reach.left, "{lean:?} {reach:?}");
                prop_assert!(lean.amount() <= reach.right, "{lean:?} {reach:?}");
            }
        }

        #[test]
        fn open_space_moves_toward_the_wanted_side_by_exactly_the_blend_step_or_the_remainder(
            tuning in any_tuning(),
            start in -1.0f32..=1.0,
            want in any_side(),
            dt in 0.001f32..0.5,
        ) {
            let from = Lean::default().step(
                Some(if start < 0.0 { LeanSide::Left } else { LeanSide::Right }),
                LeanReach::FULL,
                start.abs() * tuning.blend_seconds,
                &tuning,
            );
            let target = match want {
                None => 0.0,
                Some(LeanSide::Left) => -1.0,
                Some(LeanSide::Right) => 1.0,
            };

            let next = from.step(want, LeanReach::FULL, dt, &tuning);

            let remaining = target - from.amount();
            let moved = (dt / tuning.blend_seconds).min(remaining.abs());
            let expected = from.amount() + remaining.signum() * moved;
            prop_assert!(
                (next.amount() - expected).abs() < 1e-5,
                "{from:?} -> {next:?} toward {target}, expected {expected}"
            );
        }

        #[test]
        fn switching_sides_passes_through_upright(
            tuning in any_tuning(),
            dt in 0.001f32..0.2,
            from_left in any::<bool>(),
        ) {
            let (start, to) = if from_left {
                (LeanSide::Left, LeanSide::Right)
            } else {
                (LeanSide::Right, LeanSide::Left)
            };
            let mut lean = Lean::default().step(Some(start), LeanReach::FULL, 10.0, &tuning);
            let mut amounts = vec![lean.amount()];

            for _ in 0..((2.0 * tuning.blend_seconds / dt).ceil() as usize + 2) {
                lean = lean.step(Some(to), LeanReach::FULL, dt, &tuning);
                amounts.push(lean.amount());
            }

            let step = dt / tuning.blend_seconds;
            prop_assert!(
                amounts.iter().any(|amount| amount.abs() <= step / 2.0 + 1e-5),
                "never near upright: {amounts:?}"
            );
            let ends_at = if from_left { 1.0 } else { -1.0 };
            prop_assert_eq!(*amounts.last().expect("a start amount"), ends_at);
        }

        #[test]
        fn a_zero_reach_holds_that_side_at_upright(
            tuning in any_tuning(),
            dt in 0.001f32..0.5,
            right_reach in 0.0f32..=1.0,
            steps in 1usize..30,
        ) {
            let walled = LeanReach { left: 0.0, right: right_reach };
            let mut lean = Lean::default();

            for _ in 0..steps {
                lean = lean.step(Some(LeanSide::Left), walled, dt, &tuning);
            }

            prop_assert_eq!(lean.amount(), 0.0);
        }
    }
}
