//! Integer-accumulator fixed-timestep scheduling.

use std::time::Duration;

use crate::clock::GameClock;

/// Tick rate and catch-up limit for a loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopConfig {
    pub tick_hz: u32,
    /// Cap on ticks run for one frame; excess time is dropped.
    pub max_catch_up_ticks: u32,
    pub clock: GameClock,
}

impl LoopConfig {
    /// A config at `tick_hz` with the default catch-up cap of 5 ticks.
    pub fn new(tick_hz: u32) -> Self {
        Self {
            tick_hz,
            max_catch_up_ticks: 5,
            clock: GameClock::default(),
        }
    }

    pub fn with_clock(mut self, clock: GameClock) -> Self {
        self.clock = clock;
        self
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

/// How many simulation ticks run per tick period of real time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SimSpeed {
    #[default]
    X1,
    X2,
    X3,
}

impl SimSpeed {
    /// Ticks run per real tick period: 1, 2 or 3.
    pub fn multiplier(self) -> u32 {
        todo!("SimSpeed::multiplier")
    }
}

const UNITS_PER_TICK: u128 = 1_000_000_000;

/// Converts frame durations into whole ticks without drift.
///
/// Time is accumulated as `nanoseconds * tick_hz` in an integer, so a tick is
/// due every [`UNITS_PER_TICK`] units and no rounding error builds up.
#[derive(Debug, Clone)]
pub struct FixedStep {
    tick_hz: u32,
    max_catch_up_ticks: u32,
    accumulator: u128,
}

impl FixedStep {
    /// # Panics
    ///
    /// Panics if `config.tick_hz` is 0, which would never tick.
    pub fn new(config: LoopConfig) -> Self {
        assert!(config.tick_hz > 0, "tick_hz must be non-zero");
        Self {
            tick_hz: config.tick_hz,
            max_catch_up_ticks: config.max_catch_up_ticks,
            accumulator: 0,
        }
    }

    /// Ticks due for a frame of `frame_dt`.
    ///
    /// At most `max_catch_up_ticks` are returned; time beyond that is dropped
    /// so a stall cannot trigger a spiral of ever-longer catch-up frames.
    pub fn advance(&mut self, frame_dt: Duration) -> u32 {
        let added = frame_dt.as_nanos() * u128::from(self.tick_hz);
        self.accumulator = self.accumulator.saturating_add(added);
        let due = self.accumulator / UNITS_PER_TICK;
        self.accumulator %= UNITS_PER_TICK;
        due.min(u128::from(self.max_catch_up_ticks)) as u32
    }

    /// Runs the simulation at `speed` from the next `advance`.
    pub fn set_speed(&mut self, _speed: SimSpeed) {}

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

    #[test]
    fn catch_up_cap_scales_with_speed() {
        let mut at_3x = FixedStep::new(LoopConfig::new(60));
        at_3x.set_speed(SimSpeed::X3);
        let mut at_2x = FixedStep::new(LoopConfig::new(60));
        at_2x.set_speed(SimSpeed::X2);

        let stalled_3x = at_3x.advance(Duration::from_secs(1));
        let stalled_2x = at_2x.advance(Duration::from_secs(1));

        assert_eq!(stalled_3x, 15);
        assert_eq!(stalled_2x, 10);
        assert!(
            at_3x.alpha().abs() < 1e-6,
            "dropped time must not leave a remainder"
        );
    }

    #[test]
    fn changing_speed_keeps_the_sub_tick_remainder() {
        let mut step = FixedStep::new(LoopConfig::new(60));
        let frame = Duration::from_nanos(1_000_000_000 / 144);
        step.advance(frame);
        let before = step.alpha();

        step.set_speed(SimSpeed::X3);
        let after_change = step.alpha();
        let ticks = step.advance(frame);

        assert!((after_change - before).abs() < 1e-6, "alpha {after_change}");
        assert_eq!(ticks, 1);
        assert!(
            (step.alpha() - (60.0 / 144.0 * 4.0 - 1.0)).abs() < 1e-3,
            "alpha {}",
            step.alpha()
        );
    }

    proptest! {
        #[test]
        fn speed_multiplies_the_ticks_for_any_frame_split(
            mut cuts in prop::collection::vec(0u64..=1_000_000_000, 0..50),
            speed in prop::sample::select(vec![
                (SimSpeed::X1, 60u32),
                (SimSpeed::X2, 120),
                (SimSpeed::X3, 180),
            ]),
        ) {
            cuts.push(0);
            cuts.push(1_000_000_000);
            cuts.sort_unstable();
            let mut step = FixedStep::new(LoopConfig { max_catch_up_ticks: 1000, ..LoopConfig::new(60) });
            step.set_speed(speed.0);

            let total: u32 = cuts
                .windows(2)
                .map(|w| step.advance(Duration::from_nanos(w[1] - w[0])))
                .sum();

            prop_assert_eq!(total, speed.1);
        }

        #[test]
        fn sixty_frames_of_one_sixtieth_run_exactly_sixty_ticks(
            mut cuts in prop::collection::vec(0u64..=1_000_000_000, 0..50)
        ) {
            cuts.push(0);
            cuts.push(1_000_000_000);
            cuts.sort_unstable();
            let mut step = FixedStep::new(LoopConfig { tick_hz: 60, max_catch_up_ticks: 100, ..LoopConfig::new(60) });

            let total: u32 = cuts
                .windows(2)
                .map(|w| step.advance(Duration::from_nanos(w[1] - w[0])))
                .sum();

            prop_assert_eq!(total, 60);
        }
    }
}
