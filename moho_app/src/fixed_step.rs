//! Integer-accumulator fixed-timestep scheduling.

use std::time::Duration;

/// Tick rate and catch-up limit for a loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopConfig {
    pub tick_hz: u32,
    /// Cap on ticks run for one frame; excess time is dropped.
    pub max_catch_up_ticks: u32,
}

impl LoopConfig {
    /// A config at `tick_hz` with the default catch-up cap of 5 ticks.
    pub fn new(tick_hz: u32) -> Self {
        Self {
            tick_hz,
            max_catch_up_ticks: 5,
        }
    }

    /// Simulated time one tick covers.
    ///
    /// # Panics
    ///
    /// Panics if `tick_hz` is 0.
    pub fn tick_length(&self) -> Duration {
        Duration::from_secs(1) / self.tick_hz
    }
}

const UNITS_PER_TICK: u128 = 1_000_000_000;

/// Converts frame durations into whole ticks without drift.
///
/// Time is accumulated as `nanoseconds * tick_hz` in an integer, so a tick is
/// due every [`UNITS_PER_TICK`] units and no rounding error builds up.
#[derive(Debug, Clone)]
pub struct FixedStep {
    config: LoopConfig,
    accumulator: u128,
}

impl FixedStep {
    pub fn new(config: LoopConfig) -> Self {
        Self {
            config,
            accumulator: 0,
        }
    }

    /// Ticks due for a frame of `frame_dt`.
    ///
    /// At most `max_catch_up_ticks` are returned; time beyond that is dropped
    /// so a stall cannot trigger a spiral of ever-longer catch-up frames.
    pub fn advance(&mut self, frame_dt: Duration) -> u32 {
        let added = frame_dt.as_nanos() * u128::from(self.config.tick_hz);
        self.accumulator = self.accumulator.saturating_add(added);
        let due = self.accumulator / UNITS_PER_TICK;
        self.accumulator %= UNITS_PER_TICK;
        due.min(u128::from(self.config.max_catch_up_ticks)) as u32
    }

    /// Fraction of a tick left over after the last `advance`.
    pub fn alpha(&self) -> f32 {
        self.accumulator as f32 / UNITS_PER_TICK as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn catch_up_after_stall_runs_at_most_the_cap() {
        let mut step = FixedStep::new(LoopConfig::new(60));

        let stalled = step.advance(Duration::from_secs(1));
        let alpha_after_stall = step.alpha();
        let following = step.advance(Duration::from_millis(17));

        assert_eq!(stalled, 5);
        assert!(
            alpha_after_stall.abs() < 1e-6,
            "dropped time must not leave a remainder"
        );
        assert_eq!(following, 1);
        assert!((step.alpha() - 0.02).abs() < 1e-4, "alpha {}", step.alpha());
    }

    #[test]
    fn frame_shorter_than_tick_runs_none_and_reports_alpha() {
        let mut step = FixedStep::new(LoopConfig::new(60));

        let ticks = step.advance(Duration::from_nanos(1_000_000_000 / 144));

        assert_eq!(ticks, 0);
        assert!(
            (step.alpha() - 60.0 / 144.0).abs() < 1e-4,
            "alpha {}",
            step.alpha()
        );
    }

    #[test]
    fn remainder_carries_over_to_the_next_frame() {
        let mut step = FixedStep::new(LoopConfig::new(60));
        let frame = Duration::from_nanos(1_000_000_000 / 144);

        let first = step.advance(frame);
        let second = step.advance(frame);
        let third = step.advance(frame);

        assert_eq!([first, second, third], [0, 0, 1]);
        assert!((step.alpha() - (3.0 * 60.0 / 144.0 - 1.0)).abs() < 1e-4);
    }

    #[test]
    fn capped_frame_keeps_the_sub_tick_remainder() {
        let mut step = FixedStep::new(LoopConfig::new(60));

        let ticks = step.advance(Duration::from_nanos(13 * 1_000_000_000 / 120));

        assert_eq!(ticks, 5);
        assert!((step.alpha() - 0.5).abs() < 1e-4, "alpha {}", step.alpha());
    }

    #[test]
    fn longest_frame_runs_the_cap_without_overflow() {
        let mut step = FixedStep::new(LoopConfig::new(60));

        assert_eq!(step.advance(Duration::MAX), 5);
        assert_eq!(
            step.advance(Duration::from_nanos(1_000_000_000 / 60 + 1)),
            1
        );
    }

    proptest! {
        #[test]
        fn sixty_frames_of_one_sixtieth_run_exactly_sixty_ticks(
            mut cuts in prop::collection::vec(0u64..=1_000_000_000, 0..50)
        ) {
            cuts.push(0);
            cuts.push(1_000_000_000);
            cuts.sort_unstable();
            let mut step = FixedStep::new(LoopConfig { tick_hz: 60, max_catch_up_ticks: 100 });

            let total: u32 = cuts
                .windows(2)
                .map(|w| step.advance(Duration::from_nanos(w[1] - w[0])))
                .sum();

            prop_assert_eq!(total, 60);
        }
    }
}
