//! A window-free driver for the fixed-timestep loop.

use std::time::Duration;

use crate::clock::GameClock;
use crate::fixed_step::{FixedStep, LoopConfig, SimSpeed};
use crate::{FrameContext, Game, TickContext};

/// Drives a [`Game`] without a window.
#[derive(Debug, Clone)]
pub struct HeadlessLoop<G> {
    step: FixedStep,
    tick_length: Duration,
    next_tick: u64,
    paused: bool,
    speed: SimSpeed,
    clock: GameClock,
    game: std::marker::PhantomData<fn(&mut G)>,
}

impl<G: Game> HeadlessLoop<G> {
    /// A loop at `config`'s tick rate, starting at tick 0.
    ///
    /// # Panics
    ///
    /// Panics if `config.tick_hz` is 0.
    pub fn new(config: LoopConfig) -> Self {
        let step = FixedStep::new(config);
        Self {
            step,
            tick_length: config.tick_length(),
            next_tick: 0,
            paused: false,
            speed: SimSpeed::default(),
            clock: config.clock,
            game: std::marker::PhantomData,
        }
    }

    /// Runs the ticks due for `frame_dt`, then one frame. Returns ticks run.
    ///
    /// While paused no tick runs; the game's command is applied through
    /// [`Game::apply_while_paused`] before the frame. Pause and speed the frame
    /// asks for take effect from the next tick.
    pub fn advance(&mut self, game: &mut G, frame_dt: Duration) -> u32 {
        let n = self.run_ticks(game, frame_dt);
        self.apply_paused_command(game);
        let mut ctx = FrameContext::headless(self.tick_length, &self.clock);
        game.frame(&mut ctx, self.step.alpha());
        let (paused, speed) = (ctx.requested_paused(), ctx.requested_speed());
        if let Some(paused) = paused {
            self.set_paused(paused);
        }
        if let Some(speed) = speed {
            self.set_speed(speed);
        }
        n
    }

    /// While paused, hands the game's command to [`Game::apply_while_paused`]
    /// at the boundary before the next tick.
    pub(crate) fn apply_paused_command(&mut self, game: &mut G) {
        if self.paused {
            let command = game.command();
            game.apply_while_paused(self.next_tick, &command);
        }
    }

    /// Runs the ticks due for `frame_dt` and no frame. Returns ticks run.
    ///
    /// A paused loop runs none and leaves the sub-tick remainder untouched.
    pub(crate) fn run_ticks(&mut self, game: &mut G, frame_dt: Duration) -> u32 {
        if self.paused {
            return 0;
        }
        let n = self.step.advance(frame_dt);
        for _ in 0..n {
            self.run_tick(game);
        }
        n
    }

    /// Pauses or resumes the simulation; the speed is untouched.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn set_speed(&mut self, speed: SimSpeed) {
        self.speed = speed;
        self.step.set_speed(speed);
    }

    pub fn speed(&self) -> SimSpeed {
        self.speed
    }

    /// Index of the next tick to run.
    pub fn next_tick(&self) -> u64 {
        self.next_tick
    }

    /// Real time until the next tick is due; one tick length while paused.
    pub(crate) fn until_next_tick(&self) -> Duration {
        if self.paused {
            return self.tick_length;
        }
        (self.tick_length / self.speed.multiplier()).mul_f32(1.0 - self.step.alpha())
    }

    /// Fraction of a tick left over after the last run.
    pub(crate) fn alpha(&self) -> f32 {
        self.step.alpha()
    }

    pub fn clock(&self) -> &GameClock {
        &self.clock
    }

    pub(crate) fn tick_length(&self) -> Duration {
        self.tick_length
    }

    /// Runs exactly `n` ticks and no frame, whether or not the loop is paused.
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
            clock: &mut self.clock,
        };
        game.tick(&mut ctx, &command);
        self.clock.advance(self.tick_length.as_secs_f32());
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
        tick_lengths: Vec<Duration>,
    }

    impl Game for Integrator {
        type Command = i64;

        fn command(&mut self) -> i64 {
            let i = self.issued as i64;
            self.issued += 1;
            (i * 7 + 3) % 11 - 5
        }

        fn tick(&mut self, ctx: &mut TickContext<'_>, command: &i64) {
            self.state = self
                .state
                .wrapping_mul(31)
                .wrapping_add(*command as u64)
                .wrapping_add(ctx.tick);
            self.history.push(self.state);
            self.tick_lengths.push(ctx.tick_length);
        }

        fn frame(&mut self, _ctx: &mut FrameContext<'_>, _alpha: f32) {}
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

        fn tick(&mut self, ctx: &mut TickContext<'_>, command: &i32) {
            self.commands.push(*command);
            self.ticks.push(ctx.tick);
            self.tick_lengths.push(ctx.tick_length);
        }

        fn frame(&mut self, _ctx: &mut FrameContext<'_>, alpha: f32) {
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

    #[derive(Default)]
    struct ContextProbe {
        renderer_absent: Option<bool>,
        audio_absent: Option<bool>,
        tick_length: Option<Duration>,
    }

    impl Game for ContextProbe {
        type Command = ();

        fn command(&mut self) {}

        fn tick(&mut self, _ctx: &mut TickContext<'_>, _command: &()) {}

        fn frame(&mut self, ctx: &mut FrameContext<'_>, _alpha: f32) {
            self.renderer_absent = Some(ctx.renderer().is_none());
            self.audio_absent = Some(ctx.audio().is_none());
            self.tick_length = Some(ctx.tick_length);
        }
    }

    #[test]
    fn headless_frame_context_has_no_renderer_and_no_audio() {
        let mut game = ContextProbe::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        sim.advance(&mut game, Duration::from_nanos(NS / 60));

        assert_eq!(game.renderer_absent, Some(true), "frame must run");
        assert_eq!(game.audio_absent, Some(true));
        assert_eq!(game.tick_length, Some(Duration::from_secs(1) / 60));
    }

    #[test]
    fn run_ticks_runs_due_ticks_without_a_frame_and_exposes_the_leftover_alpha() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        let ran = sim.run_ticks(&mut game, Duration::from_nanos(NS / 144));

        assert_eq!(ran, 0);
        assert!(game.frames.is_empty(), "run_ticks must not run a frame");
        assert!(
            (sim.alpha() - 60.0 / 144.0).abs() < 1e-4,
            "alpha {}",
            sim.alpha()
        );
    }

    #[test]
    fn alpha_is_the_fraction_of_a_tick_left_after_whole_ticks() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        let ran = sim.run_ticks(&mut game, Duration::from_nanos(NS / 60 + NS / 240));

        assert_eq!(ran, 1);
        assert!((sim.alpha() - 0.25).abs() < 1e-3, "alpha {}", sim.alpha());
    }

    #[test]
    fn tick_length_is_the_configured_tick_period() {
        let sim = HeadlessLoop::<Recorder>::new(LoopConfig::new(60));

        assert_eq!(sim.tick_length(), Duration::from_secs(1) / 60);
    }

    #[test]
    #[should_panic(expected = "tick_hz must be non-zero")]
    fn zero_tick_rate_is_rejected() {
        let _ = HeadlessLoop::<Recorder>::new(LoopConfig::new(0));
    }

    fn run_to_600_ticks(speed: SimSpeed, fps: u64) -> (u32, Integrator) {
        let mut game = Integrator::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));
        sim.set_speed(speed);
        let frame = Duration::from_nanos(NS / fps);
        let mut frames = 0;

        while game.history.len() < 600 {
            assert!(frames < 10_000, "only {} ticks ran", game.history.len());
            sim.advance(&mut game, frame);
            frames += 1;
        }

        (frames, game)
    }

    #[test]
    fn a_paused_loop_runs_no_ticks_and_still_draws_frames() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));
        sim.set_paused(true);

        let ran: u32 = (0..60)
            .map(|_| sim.advance(&mut game, Duration::from_nanos(NS / 60)))
            .sum();

        assert_eq!(ran, 0);
        assert!(game.ticks.is_empty());
        assert_eq!(game.frames.len(), 60);
        assert_eq!(sim.next_tick(), 0);
        assert_eq!(sim.clock().elapsed_seconds(), 0.0);
    }

    /// Records what the loop hands to `apply_while_paused` and every tick it runs.
    #[derive(Default)]
    struct OrderTaker {
        next_command: i32,
        applied: Vec<(u64, i32)>,
        ticks: Vec<u64>,
    }

    impl Game for OrderTaker {
        type Command = i32;

        fn command(&mut self) -> i32 {
            self.next_command += 1;
            self.next_command
        }

        fn tick(&mut self, ctx: &mut TickContext<'_>, _command: &i32) {
            self.ticks.push(ctx.tick);
        }

        fn apply_while_paused(&mut self, next_tick: u64, command: &i32) {
            self.applied.push((next_tick, *command));
        }

        fn frame(&mut self, _ctx: &mut FrameContext<'_>, _alpha: f32) {}
    }

    #[test]
    fn a_command_given_while_paused_applies_without_advancing_time() {
        let mut game = OrderTaker::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));
        sim.step(&mut game, 2);
        sim.set_paused(true);
        let elapsed = sim.clock().elapsed_seconds();

        for _ in 0..3 {
            sim.advance(&mut game, Duration::from_nanos(NS / 60));
        }

        assert_eq!(game.applied, vec![(2, 3), (2, 4), (2, 5)]);
        assert_eq!(game.ticks, vec![0, 1], "no tick may run while paused");
        assert_eq!(sim.next_tick(), 2);
        assert_eq!(sim.clock().elapsed_seconds(), elapsed);
    }

    #[test]
    fn a_running_loop_never_applies_commands_as_paused_ones() {
        let mut game = OrderTaker::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        sim.advance(&mut game, Duration::from_nanos(3 * NS / 60));

        assert!(game.applied.is_empty());
        assert_eq!(game.ticks, vec![0, 1, 2]);
    }

    #[test]
    fn speed_sets_ticks_per_second_of_real_time() {
        for (speed, expected) in [(SimSpeed::X1, 60), (SimSpeed::X2, 120), (SimSpeed::X3, 180)] {
            let mut game = Recorder::default();
            let mut sim = HeadlessLoop::new(LoopConfig::new(60));
            sim.set_speed(speed);

            let ran: u32 = (0..250)
                .map(|_| sim.advance(&mut game, Duration::from_nanos(NS / 250)))
                .sum();

            assert_eq!(ran, expected, "{speed:?}");
            assert_eq!(game.ticks.len(), expected as usize, "{speed:?}");
        }
    }

    #[test]
    fn speed_does_not_change_results() {
        let (frames_at_1x, at_1x) = run_to_600_ticks(SimSpeed::X1, 60);
        let (frames_at_3x, at_3x) = run_to_600_ticks(SimSpeed::X3, 60);

        assert_eq!(at_3x.history[599], at_1x.history[599]);
        assert_ne!(at_1x.history[599], 0);
        assert!(
            frames_at_3x * 2 < frames_at_1x,
            "3x took {frames_at_3x} frames, 1x took {frames_at_1x}"
        );
        let tick = Duration::from_secs(1) / 60;
        assert!(at_1x.tick_lengths.iter().all(|l| *l == tick));
        assert!(at_3x.tick_lengths.iter().all(|l| *l == tick));
    }

    #[test]
    fn a_stalled_frame_at_3x_runs_the_cap_and_still_draws() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));
        sim.set_speed(SimSpeed::X3);

        let ran = sim.advance(&mut game, Duration::from_secs(1));

        assert_eq!(ran, 15);
        assert_eq!(game.ticks.len(), 15);
        assert_eq!(game.frames.len(), 1);
    }

    #[test]
    fn pausing_and_resuming_keeps_the_speed() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));
        sim.set_speed(SimSpeed::X2);

        sim.set_paused(true);
        let paused_speed = sim.speed();
        sim.set_paused(false);
        let ran: u32 = (0..250)
            .map(|_| sim.advance(&mut game, Duration::from_nanos(NS / 250)))
            .sum();

        assert_eq!(paused_speed, SimSpeed::X2);
        assert_eq!(sim.speed(), SimSpeed::X2);
        assert!(!sim.paused());
        assert_eq!(ran, 120);
    }

    #[test]
    fn alpha_holds_while_paused_and_resumes_from_it() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));
        let frame = Duration::from_nanos(NS / 144);
        sim.advance(&mut game, frame);
        let before = sim.alpha();

        sim.set_paused(true);
        let ran_paused: u32 = (0..60)
            .map(|_| sim.advance(&mut game, Duration::from_nanos(NS / 60)))
            .sum();
        let held = sim.alpha();
        sim.set_paused(false);
        sim.advance(&mut game, frame);
        let resumed = sim.alpha();
        let ticks = sim.advance(&mut game, frame);

        assert!((before - 60.0 / 144.0).abs() < 1e-4, "alpha {before}");
        assert_eq!(ran_paused, 0);
        assert_eq!(held, before);
        assert!((resumed - 120.0 / 144.0).abs() < 1e-4, "alpha {resumed}");
        assert_eq!(ticks, 1);
        assert!((sim.alpha() - (180.0 / 144.0 - 1.0)).abs() < 1e-3);
    }

    /// Asks the loop to pause from its first frame.
    #[derive(Default)]
    struct PausesInFrame {
        ticks: u32,
    }

    impl Game for PausesInFrame {
        type Command = ();

        fn command(&mut self) {}

        fn tick(&mut self, _ctx: &mut TickContext<'_>, _command: &()) {
            self.ticks += 1;
        }

        fn frame(&mut self, ctx: &mut FrameContext<'_>, _alpha: f32) {
            ctx.set_paused(true);
            ctx.set_speed(SimSpeed::X3);
        }
    }

    #[test]
    fn a_pause_asked_for_in_a_frame_stops_the_next_ticks() {
        let mut game = PausesInFrame::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));

        let first = sim.advance(&mut game, Duration::from_nanos(3 * NS / 60));
        let second = sim.advance(&mut game, Duration::from_nanos(3 * NS / 60));

        assert_eq!(first, 3, "the request takes effect from the next tick");
        assert_eq!(second, 0);
        assert_eq!(game.ticks, 3);
        assert!(sim.paused());
        assert_eq!(sim.speed(), SimSpeed::X3);
    }

    fn assert_close(actual: Duration, expected: Duration) {
        assert!(
            actual.abs_diff(expected) < Duration::from_micros(1),
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn until_next_tick_shrinks_with_speed_and_is_one_tick_while_paused() {
        let tick = Duration::from_secs(1) / 60;
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::<Recorder>::new(LoopConfig::new(60));
        let mut waits = Vec::new();
        for speed in [SimSpeed::X1, SimSpeed::X2, SimSpeed::X3] {
            sim.set_speed(speed);
            waits.push(sim.until_next_tick());
        }
        sim.set_speed(SimSpeed::X2);
        sim.advance(&mut game, Duration::from_nanos(NS / 240));
        let half_done = sim.until_next_tick();
        sim.set_paused(true);
        let paused = sim.until_next_tick();

        assert_close(waits[0], tick);
        assert_close(waits[1], tick / 2);
        assert_close(waits[2], tick / 3);
        assert_close(half_done, tick / 4);
        assert_close(paused, tick);
    }

    #[test]
    fn step_runs_its_ticks_even_while_paused() {
        let mut game = Recorder::default();
        let mut sim = HeadlessLoop::new(LoopConfig::new(60));
        sim.set_paused(true);

        sim.step(&mut game, 5);

        assert_eq!(game.ticks, vec![0, 1, 2, 3, 4]);
        assert_eq!(sim.next_tick(), 5);
        assert!(sim.paused(), "step must not resume the loop");
    }
}
