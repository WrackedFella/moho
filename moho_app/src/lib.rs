//! Fixed-timestep application loop shared by every game line.

pub mod fixed_step;
pub mod headless;
pub mod runner;

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
pub struct FrameContext<'a> {
    pub tick_length: Duration,
    renderer: Option<&'a mut dyn moho_renderer::RendererBackend>,
    audio: Option<&'a mut moho_audio::AudioSystem>,
    exit_requested: bool,
}

impl<'a> FrameContext<'a> {
    pub(crate) fn headless(tick_length: Duration) -> Self {
        Self {
            tick_length,
            renderer: None,
            audio: None,
            exit_requested: false,
        }
    }

    /// The renderer, absent when running headless.
    pub fn renderer(&mut self) -> Option<&mut dyn moho_renderer::RendererBackend> {
        let _ = (&self.renderer, self.exit_requested);
        todo!("FrameContext::renderer")
    }

    /// The audio system, absent when running headless or without audio.
    pub fn audio(&mut self) -> Option<&mut moho_audio::AudioSystem> {
        let _ = &self.audio;
        todo!("FrameContext::audio")
    }
}

/// A game driven by a fixed-timestep loop.
pub trait Game {
    /// Input sampled once per tick.
    type Command;

    /// Samples the command for the next tick.
    fn command(&mut self) -> Self::Command;

    fn tick(&mut self, ctx: &mut TickContext, command: &Self::Command);

    /// Presents a frame, `alpha` of the way between the last two ticks.
    fn frame(&mut self, ctx: &mut FrameContext<'_>, alpha: f32);
}
