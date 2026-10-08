//! A window-free driver for the fixed-timestep loop.

use std::time::Duration;

use crate::fixed_step::{FixedStep, LoopConfig};
use crate::{FrameContext, Game, TickContext};

/// Drives a [`Game`] without a window.
#[derive(Debug, Clone)]
pub struct HeadlessLoop<G> {
    step: FixedStep,
    tick_length: Duration,
    next_tick: u64,
    game: std::marker::PhantomData<fn(&mut G)>,
}

impl<G: Game> HeadlessLoop<G> {
    /// A loop at `config`'s tick rate, starting at tick 0.
    ///
    /// # Panics
    ///
    /// Panics if `config.tick_hz` is 0.
    pub fn new(config: LoopConfig) -> Self {
        assert!(config.tick_hz > 0, "tick_hz must be non-zero");
        Self {
            step: FixedStep::new(config),
            tick_length: config.tick_length(),
            next_tick: 0,
            game: std::marker::PhantomData,
        }
    }

    /// Runs the ticks due for `frame_dt`, then one frame. Returns ticks run.
    pub fn advance(&mut self, game: &mut G, frame_dt: Duration) -> u32 {
        let n = self.step.advance(frame_dt);
        for _ in 0..n {
            self.run_tick(game);
        }
        let mut ctx = FrameContext {
            tick_length: self.tick_length,
        };
        game.frame(&mut ctx, self.step.alpha());
        n
    }

    /// Runs exactly `n` ticks and no frame.
    pub fn step(&mut self, game: &mut G, n: u32) {
        for _ in 0..n {
            self.run_tick(game);
        }
    }

    fn run_tick(&mut self, game: &mut G) {
        let command = game.command();
        let mut ctx = TickContext {
            tick: self.next_tick,
            tick_length: self.tick_length,
        };
        game.tick(&mut ctx, &command);
        self.next_tick += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: u64 = 1_000_000_000;

    /// Folds a scripted command sequence into order-sensitive state and
    /// records the state after every tick.
    #[derive(Default)]
    struct Integrator {
        issued: u64,
        state: u64,
        history: Vec<u64>,
    }

    impl Game for Integrator {
        type Command = i64;

        fn command(&mut self) -> i64 {
            let i = self.issued as i64;
            self.issued += 1;
            (i * 7 + 3) % 11 - 5
        }

        fn tick(&mut self, ctx: &mut TickContext, command: &i64) {
            self.state = self
                .state
                .wrapping_mul(31)
                .wrapping_add(*command as u64)
                .wrapping_add(ctx.tick);
            self.history.push(self.state);
        }

        fn frame(&mut self, _ctx: &mut FrameContext, _alpha: f32) {}
    }

    fn state_after_600_ticks(fps: u64) -> u64 {
        let mut game = Integrator::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));
        let frame = Duration::from_nanos(NS / fps);

        for _ in 0..10_000 {
            if game.history.len() >= 600 {
                break;
            }
            sim.advance(&mut game, frame);
        }

        assert!(
            game.history.len() >= 600,
            "only {} ticks ran",
            game.history.len()
        );
        game.history[599]
    }

    #[test]
    fn frame_rate_does_not_change_simulation_results() {
        let at_60 = state_after_600_ticks(60);

        for fps in [30, 144] {
            assert_eq!(
                state_after_600_ticks(fps),
                at_60,
                "{fps} fps diverged from 60 fps"
            );
        }
        assert_ne!(at_60, 0);
    }

    #[derive(Default)]
    struct Recorder {
        next_command: i32,
        commands: Vec<i32>,
        ticks: Vec<u64>,
        tick_lengths: Vec<Duration>,
        frames: Vec<f32>,
    }

    impl Game for Recorder {
        type Command = i32;

        fn command(&mut self) -> i32 {
            self.next_command += 1;
            self.next_command
        }

        fn tick(&mut self, ctx: &mut TickContext, command: &i32) {
            self.commands.push(*command);
            self.ticks.push(ctx.tick);
            self.tick_lengths.push(ctx.tick_length);
        }

        fn frame(&mut self, _ctx: &mut FrameContext, alpha: f32) {
            self.frames.push(alpha);
        }
    }

    #[test]
    fn step_runs_tick_n_times_in_order() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        sim.step(&mut game, 10);

        assert_eq!(game.ticks, (0..10).collect::<Vec<u64>>());
        assert!(
            game.tick_lengths
                .iter()
                .all(|l| *l == Duration::from_secs(1) / 60)
        );
        assert!(game.frames.is_empty(), "step must not run frames");
    }

    #[test]
    fn step_continues_tick_indices_across_calls() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        sim.step(&mut game, 3);
        sim.step(&mut game, 2);

        assert_eq!(game.ticks, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn each_tick_receives_its_own_command() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        let ran = sim.advance(&mut game, Duration::from_nanos(3 * NS / 60));

        assert_eq!(ran, 3);
        assert_eq!(game.commands, vec![1, 2, 3]);
        assert_eq!(
            game.next_command, 3,
            "command() must be called once per tick"
        );
    }

    #[test]
    fn advance_runs_one_frame_after_ticks_with_alpha() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        sim.advance(&mut game, Duration::from_nanos(NS / 144));

        assert!(game.ticks.is_empty());
        assert_eq!(game.frames.len(), 1);
        assert!(
            (game.frames[0] - 60.0 / 144.0).abs() < 1e-4,
            "alpha {}",
            game.frames[0]
        );
    }

    #[test]
    #[should_panic(expected = "tick_hz must be non-zero")]
    fn zero_tick_rate_is_rejected() {
        let _ = HeadlessLoop::<Recorder>::new(LoopConfig::new(0));
    }
}
