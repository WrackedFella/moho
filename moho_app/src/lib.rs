//! Fixed-timestep application loop shared by every game line.

pub mod fixed_step;
pub mod headless;

use std::time::Duration;

pub use fixed_step::{FixedStep, LoopConfig};
pub use headless::HeadlessLoop;

/// Per-tick context handed to [`Game::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickContext {
    /// Index of this tick, starting at 0.
    pub tick: u64,
    pub tick_length: Duration,
}

/// Per-frame context handed to [`Game::frame`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameContext {
    pub tick_length: Duration,
}

/// A game driven by a fixed-timestep loop.
pub trait Game {
    /// Input sampled once per tick.
    type Command;

    /// Samples the command for the next tick.
    fn command(&mut self) -> Self::Command;

    fn tick(&mut self, ctx: &mut TickContext, command: &Self::Command);

    /// Presents a frame, `alpha` of the way between the last two ticks.
    fn frame(&mut self, ctx: &mut FrameContext, alpha: f32);
}
