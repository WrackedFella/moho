//! Fixed-timestep application loop shared by every game line.

pub mod clock;
pub mod fixed_step;
pub mod headless;
pub mod runner;

use std::sync::Arc;
use std::time::Duration;

use moho_audio::AudioSystem;
use moho_renderer::RendererBackend;
use winit::event::{DeviceEvent, WindowEvent};
use winit::window::Window;

pub use clock::GameClock;
pub use fixed_step::{FixedStep, LoopConfig};
pub use headless::HeadlessLoop;
pub use runner::run;

/// Per-tick context handed to [`Game::tick`].
#[derive(Debug)]
pub struct TickContext<'a> {
    /// Index of this tick, starting at 0.
    pub tick: u64,
    pub tick_length: Duration,
    /// Advanced by the loop after the game's tick.
    pub clock: &'a mut GameClock,
}

/// Per-frame context handed to [`Game::frame`].
pub struct FrameContext<'a> {
    pub tick_length: Duration,
    clock: &'a GameClock,
    renderer: Option<&'a mut (dyn RendererBackend + 'static)>,
    audio: Option<&'a mut AudioSystem>,
    exit_requested: bool,
}

impl<'a> FrameContext<'a> {
    /// A context for driving [`Game::frame`] directly; a game's tests use it
    /// with a fake renderer.
    pub fn new(
        tick_length: Duration,
        clock: &'a GameClock,
        renderer: Option<&'a mut (dyn RendererBackend + 'static)>,
        audio: Option<&'a mut AudioSystem>,
    ) -> Self {
        Self {
            tick_length,
            clock,
            renderer,
            audio,
            exit_requested: false,
        }
    }

    pub(crate) fn headless(tick_length: Duration, clock: &'a GameClock) -> Self {
        Self::new(tick_length, clock, None, None)
    }

    pub fn clock(&self) -> &'a GameClock {
        self.clock
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

    /// Whether the game asked to leave the loop.
    pub fn exit_requested(&self) -> bool {
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
    clock: &'a GameClock,
    renderer: Option<&'a mut (dyn RendererBackend + 'static)>,
    audio: Option<&'a mut AudioSystem>,
    exit_requested: bool,
}

impl<'a> EventContext<'a> {
    /// A context for driving [`Game::event`] directly; a game's tests use it
    /// with a fake renderer.
    pub fn new(
        clock: &'a GameClock,
        renderer: Option<&'a mut (dyn RendererBackend + 'static)>,
        audio: Option<&'a mut AudioSystem>,
    ) -> Self {
        Self {
            clock,
            renderer,
            audio,
            exit_requested: false,
        }
    }

    pub fn clock(&self) -> &'a GameClock {
        self.clock
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

    /// Whether the game asked to leave the loop.
    pub fn exit_requested(&self) -> bool {
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

    fn tick(&mut self, ctx: &mut TickContext<'_>, command: &Self::Command);

    /// Called once the window and renderer exist; never called headless.
    fn init(&mut self, _ctx: &mut InitContext<'_>) {}

    /// Called for every window and device event the runner does not consume;
    /// never called headless.
    fn event(&mut self, _ctx: &mut EventContext<'_>, _event: Event) {}

    /// Presents a frame, `alpha` of the way between the last two ticks.
    fn frame(&mut self, ctx: &mut FrameContext<'_>, alpha: f32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use moho_renderer::{FrameError, InstanceGpu, LightingGpu, MaterialGpu};

    /// Records the point lights it is given; everything else is a no-op.
    #[derive(Default)]
    struct FakeRenderer {
        lights: Vec<f32>,
    }

    impl RendererBackend for FakeRenderer {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            0
        }
        fn register_indexed_mesh(
            &mut self,
            _vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            _ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            _block_light_rgb: &[[f32; 3]],
            _sky_exposed: &[f32],
            _indices: &[u32],
        ) -> u32 {
            0
        }
        fn unregister_mesh(&mut self, _mesh: u32) {}
        fn begin_frame(
            &mut self,
            _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        ) -> Result<(), FrameError> {
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[InstanceGpu]) {}
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, _materials: &[MaterialGpu]) {}
        fn update_lighting(&mut self, _lighting: LightingGpu) {}
        fn add_point_light(
            &mut self,
            _position: glam::Vec3,
            _color: glam::Vec3,
            intensity: f32,
            _range: f32,
        ) -> u32 {
            self.lights.push(intensity);
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _position: glam::Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, _quality: u8) {}
        fn set_ssao_quality(&mut self, _quality: u8) {}
        fn set_frame_callback_arc(
            &mut self,
            _cb: Option<std::sync::Arc<std::sync::Mutex<dyn moho_renderer::FrameCallback>>>,
        ) {
        }
    }

    const TICK: Duration = Duration::from_millis(16);

    fn light(ctx_renderer: Option<&mut (dyn RendererBackend + 'static)>, intensity: f32) {
        ctx_renderer.expect("renderer present").add_point_light(
            glam::Vec3::ZERO,
            glam::Vec3::ONE,
            intensity,
            1.0,
        );
    }

    #[test]
    fn frame_context_hands_out_the_renderer_it_was_built_with() {
        let clock = GameClock::default();
        let mut fake = FakeRenderer::default();
        let mut ctx = FrameContext::new(TICK, &clock, Some(&mut fake), None);

        light(ctx.renderer(), 3.0);

        assert_eq!(ctx.tick_length, TICK);
        assert_eq!(fake.lights, vec![3.0]);
    }

    #[test]
    fn frame_context_without_renderer_or_audio_has_neither() {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(TICK, &clock, None, None);

        assert!(ctx.renderer().is_none());
        assert!(ctx.audio().is_none());
    }

    #[test]
    fn frame_context_exit_is_not_requested_until_asked() {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(TICK, &clock, None, None);
        assert!(!ctx.exit_requested());

        ctx.request_exit();

        assert!(ctx.exit_requested());
    }

    #[test]
    fn event_context_hands_out_the_renderer_it_was_built_with() {
        let clock = GameClock::default();
        let mut fake = FakeRenderer::default();
        let mut ctx = EventContext::new(&clock, Some(&mut fake), None);

        light(ctx.renderer(), 5.0);

        assert_eq!(fake.lights, vec![5.0]);
    }

    #[test]
    fn event_context_without_renderer_or_audio_has_neither() {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        assert!(ctx.renderer().is_none());
        assert!(ctx.audio().is_none());
    }

    #[test]
    fn event_context_exit_is_not_requested_until_asked() {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        assert!(!ctx.exit_requested());

        ctx.request_exit();

        assert!(ctx.exit_requested());
    }

    #[test]
    fn frame_context_exposes_the_clock_it_was_built_with() {
        let mut clock = GameClock::default();
        clock.reset_to(15.25);

        let ctx = FrameContext::new(TICK, &clock, None, None);

        assert_eq!(ctx.clock().time_of_day(), 15.25);
    }

    #[test]
    fn event_context_exposes_the_clock_it_was_built_with() {
        let mut clock = GameClock::default();
        clock.reset_to(15.25);

        let ctx = EventContext::new(&clock, None, None);

        assert_eq!(ctx.clock().time_of_day(), 15.25);
    }
}
