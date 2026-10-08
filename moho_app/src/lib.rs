//! Fixed-timestep application loop shared by every game line.

pub mod fixed_step;
pub mod headless;
pub mod runner;

use std::sync::Arc;
use std::time::Duration;

use moho_audio::AudioSystem;
use moho_renderer::RendererBackend;
use winit::event::{DeviceEvent, WindowEvent};
use winit::window::Window;

pub use fixed_step::{FixedStep, LoopConfig};
pub use headless::HeadlessLoop;
pub use runner::run;

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
    renderer: Option<&'a mut (dyn RendererBackend + 'static)>,
    audio: Option<&'a mut AudioSystem>,
    exit_requested: bool,
}

impl<'a> FrameContext<'a> {
    pub(crate) fn new(
        tick_length: Duration,
        renderer: Option<&'a mut (dyn RendererBackend + 'static)>,
        audio: Option<&'a mut AudioSystem>,
    ) -> Self {
        Self {
            tick_length,
            renderer,
            audio,
            exit_requested: false,
        }
    }

    pub(crate) fn headless(tick_length: Duration) -> Self {
        Self::new(tick_length, None, None)
    }

    /// The renderer, absent when running headless.
    pub fn renderer(&mut self) -> Option<&mut (dyn RendererBackend + 'static)> {
        self.renderer.as_deref_mut()
    }

    /// The audio system, absent when running headless or without audio.
    pub fn audio(&mut self) -> Option<&mut AudioSystem> {
        self.audio.as_deref_mut()
    }

    /// Asks the runner to leave the loop after this frame.
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    pub(crate) fn exit_requested(&self) -> bool {
        self.exit_requested
    }
}

/// Context handed to [`Game::init`], once the window and renderer exist.
pub struct InitContext<'a> {
    pub window: &'a Arc<Window>,
    pub renderer: &'a mut dyn RendererBackend,
    pub audio: Option<&'a mut AudioSystem>,
}

/// Context handed to [`Game::event`].
pub struct EventContext<'a> {
    renderer: Option<&'a mut (dyn RendererBackend + 'static)>,
    audio: Option<&'a mut AudioSystem>,
    exit_requested: bool,
}

impl<'a> EventContext<'a> {
    pub(crate) fn new(
        renderer: Option<&'a mut (dyn RendererBackend + 'static)>,
        audio: Option<&'a mut AudioSystem>,
    ) -> Self {
        Self {
            renderer,
            audio,
            exit_requested: false,
        }
    }

    /// The renderer, absent before the window exists.
    pub fn renderer(&mut self) -> Option<&mut (dyn RendererBackend + 'static)> {
        self.renderer.as_deref_mut()
    }

    /// The audio system, absent when disabled or when its initialisation failed.
    pub fn audio(&mut self) -> Option<&mut AudioSystem> {
        self.audio.as_deref_mut()
    }

    /// Asks the runner to leave the loop once this event is handled.
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    pub(crate) fn exit_requested(&self) -> bool {
        self.exit_requested
    }
}

/// A winit event the runner hands to the game unchanged.
#[derive(Debug)]
pub enum Event {
    Window(WindowEvent),
    Device(DeviceEvent),
}

/// Window, loop and audio settings for [`run`].
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub window_title: String,
    pub loop_config: LoopConfig,
    /// Whether to create an [`AudioSystem`]; a failure to create one is logged
    /// and the game runs without audio.
    pub init_audio: bool,
}

/// Why [`run`] stopped with an error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    #[error("failed to run the event loop: {0}")]
    EventLoop(String),
    #[error("failed to create the window: {0}")]
    Window(String),
    #[error("failed to create the renderer: {0}")]
    Renderer(String),
}

/// A game driven by a fixed-timestep loop.
pub trait Game {
    /// Input sampled once per tick.
    type Command;

    /// Samples the command for the next tick.
    fn command(&mut self) -> Self::Command;

    fn tick(&mut self, ctx: &mut TickContext, command: &Self::Command);

    /// Called once the window and renderer exist; never called headless.
    fn init(&mut self, _ctx: &mut InitContext<'_>) {}

    /// Called for every window and device event the runner does not consume;
    /// never called headless.
    fn event(&mut self, _ctx: &mut EventContext<'_>, _event: Event) {}

    /// Presents a frame, `alpha` of the way between the last two ticks.
    fn frame(&mut self, ctx: &mut FrameContext<'_>, alpha: f32);
}
