//! The winit `ApplicationHandler` that drives a [`crate::Game`].

use std::sync::Arc;
use std::time::Instant;

use moho_audio::AudioSystem;
use moho_renderer::RendererBackend;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use crate::{
    AppConfig, AppError, Event, EventContext, FrameContext, Game, HeadlessLoop, InitContext,
    Observed, ToggleKeys, WindowMode,
};

/// What the runner does with a window event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    /// Run a frame; the game does not see the event.
    Frame,
    /// Resize the renderer, then forward the event.
    Resize { width: u32, height: u32 },
    /// Forward the event, then exit the loop.
    Close,
    /// Hand the event to the game unchanged.
    Forward,
}

/// Decides how the runner handles `event`, without needing a display.
pub(crate) fn route(event: &WindowEvent) -> Route {
    match event {
        WindowEvent::RedrawRequested => Route::Frame,
        WindowEvent::Resized(size) => Route::Resize {
            width: size.width,
            height: size.height,
        },
        WindowEvent::CloseRequested => Route::Close,
        _ => Route::Forward,
    }
}

/// Runs `game` in a window until it asks to exit or the window closes.
///
/// # Errors
///
/// Returns [`AppError`] if the event loop, window or renderer cannot be created.
pub fn run<G: Game + 'static>(game: G, config: AppConfig) -> Result<(), AppError> {
    let event_loop = EventLoop::new().map_err(|e| AppError::EventLoop(e.to_string()))?;
    let toggles = config.toggle_keys.clone().map(ToggleKeys::new);
    let mut runner = Runner {
        sim: HeadlessLoop::new(config.loop_config),
        game,
        config,
        toggles,
        mode: WindowMode::Windowed,
        window: None,
        renderer: None,
        audio: None,
        last_wake: Instant::now(),
        last_frame: Instant::now(),
        error: None,
    };

    event_loop
        .run_app(&mut runner)
        .map_err(|e| AppError::EventLoop(e.to_string()))?;

    runner.error.map_or(Ok(()), Err)
}

struct Runner<G: Game> {
    game: G,
    config: AppConfig,
    toggles: Option<ToggleKeys>,
    mode: WindowMode,
    sim: HeadlessLoop<G>,
    window: Option<Arc<Window>>,
    renderer: Option<Box<dyn RendererBackend>>,
    audio: Option<AudioSystem>,
    last_wake: Instant,
    last_frame: Instant,
    error: Option<AppError>,
}

impl<G: Game> Runner<G> {
    /// Feeds key and focus events to the toggle detector; without one everything passes.
    fn observe_toggle(&mut self, event: &WindowEvent) -> Observed {
        let Some(toggles) = &mut self.toggles else {
            return Observed::Pass;
        };
        match event {
            WindowEvent::KeyboardInput { event, .. } => match event.physical_key {
                PhysicalKey::Code(code) => {
                    toggles.observe(code, event.state.is_pressed(), event.repeat)
                }
                PhysicalKey::Unidentified(_) => Observed::Pass,
            },
            WindowEvent::Focused(false) => {
                toggles.focus_lost();
                Observed::Pass
            }
            _ => Observed::Pass,
        }
    }

    fn flip_window_mode(&mut self, event_loop: &ActiveEventLoop) {
        self.mode = self.mode.toggled();
        if let Some(window) = &self.window {
            window.set_fullscreen(match self.mode {
                WindowMode::Windowed => None,
                WindowMode::Borderless => Some(Fullscreen::Borderless(None)),
            });
        }
        let mut ctx = EventContext::new(
            self.sim.clock(),
            self.renderer.as_deref_mut(),
            self.audio.as_mut(),
        );
        self.game.window_mode_changed(&mut ctx, self.mode);
        let requests = ctx.loop_requests();
        let exit = ctx.exit_requested();
        self.sim.apply_requests(requests);
        if exit {
            event_loop.exit();
        }
    }
}

impl<G: Game> ApplicationHandler for Runner<G> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default().with_title(self.config.window_title.clone());
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                self.error = Some(AppError::Window(e.to_string()));
                event_loop.exit();
                return;
            }
        };

        // SAFETY: The renderer requires a &'static Window because Box<dyn RendererBackend>
        // is implicitly 'static. We clone the Arc<Window> before leaking it, so the Arc
        // refcount keeps the Window alive independently of `window`. This is a deliberate
        // one-time leak for the main window, which lives for the entire program lifetime.
        // If winit ever allows window recreation (e.g. fullscreen toggle) this should be
        // replaced by giving RendererBackend a lifetime parameter (TD-09 / TD-06).
        let window_ref: &'static Window = Box::leak(Box::new(window.clone()));
        let mut renderer = match moho_renderer::create_renderer(Some(window_ref)) {
            Ok(renderer) => renderer,
            Err(e) => {
                self.error = Some(AppError::Renderer(e.to_string()));
                event_loop.exit();
                return;
            }
        };

        // The audio system is optional: without a device the game runs silent.
        if self.config.init_audio {
            match AudioSystem::new() {
                Ok(audio) => {
                    tracing::info!("Audio system initialized successfully");
                    self.audio = Some(audio);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to initialize audio system, continuing without audio");
                }
            }
        }

        self.game.init(&mut InitContext {
            window: &window,
            renderer: &mut *renderer,
            audio: self.audio.as_mut(),
        });

        window.request_redraw();
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.last_wake = Instant::now();
        self.last_frame = self.last_wake;
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        if self.window.is_none() {
            return;
        }

        let now = Instant::now();
        let elapsed = now - self.last_wake;
        self.last_wake = now;

        self.sim.run_ticks(&mut self.game, elapsed);

        event_loop.set_control_flow(ControlFlow::WaitUntil(now + self.sim.until_next_tick()));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match self.observe_toggle(&event) {
            Observed::Pass => {}
            Observed::Swallow => return,
            Observed::Flip => {
                self.flip_window_mode(event_loop);
                return;
            }
        }

        let route = route(&event);
        match route {
            Route::Frame => {
                self.sim.apply_paused_command(&mut self.game);
                let now = Instant::now();
                let frame_dt = self.sim.capped_frame_dt(now - self.last_frame);
                self.last_frame = now;
                let mut ctx = FrameContext::new(
                    self.sim.tick_length(),
                    self.sim.clock(),
                    self.renderer.as_deref_mut(),
                    self.audio.as_mut(),
                );
                ctx.frame_dt = frame_dt;
                self.game.frame(&mut ctx, self.sim.alpha());
                let requests = ctx.loop_requests();
                let exit = ctx.exit_requested();
                self.sim.apply_requests(requests);
                if exit {
                    event_loop.exit();
                } else if let Some(window) = &self.window {
                    window.request_redraw();
                }
                return;
            }
            Route::Resize { width, height } => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(width, height);
                }
            }
            Route::Close | Route::Forward => {}
        }

        let mut ctx = EventContext::new(
            self.sim.clock(),
            self.renderer.as_deref_mut(),
            self.audio.as_mut(),
        );
        self.game.event(&mut ctx, Event::Window(event));
        let requests = ctx.loop_requests();
        let exit = ctx.exit_requested();
        self.sim.apply_requests(requests);
        if exit || route == Route::Close {
            event_loop.exit();
        }
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let mut ctx = EventContext::new(
            self.sim.clock(),
            self.renderer.as_deref_mut(),
            self.audio.as_mut(),
        );
        self.game.event(&mut ctx, Event::Device(event));
        let requests = ctx.loop_requests();
        let exit = ctx.exit_requested();
        self.sim.apply_requests(requests);
        if exit {
            event_loop.exit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::{PhysicalPosition, PhysicalSize};

    #[test]
    fn resized_routes_to_resize_with_the_new_size() {
        let event = WindowEvent::Resized(PhysicalSize::new(1280, 720));

        assert_eq!(
            route(&event),
            Route::Resize {
                width: 1280,
                height: 720
            }
        );
    }

    #[test]
    fn resized_to_a_different_size_carries_that_size() {
        let event = WindowEvent::Resized(PhysicalSize::new(3, 9));

        assert_eq!(
            route(&event),
            Route::Resize {
                width: 3,
                height: 9
            }
        );
    }

    #[test]
    fn close_requested_routes_to_close() {
        assert_eq!(route(&WindowEvent::CloseRequested), Route::Close);
    }

    #[test]
    fn redraw_requested_routes_to_frame() {
        assert_eq!(route(&WindowEvent::RedrawRequested), Route::Frame);
    }

    #[test]
    fn other_window_events_route_to_forward() {
        let others = [
            WindowEvent::Focused(true),
            WindowEvent::Occluded(false),
            WindowEvent::Moved(PhysicalPosition::new(4, 5)),
        ];

        for event in &others {
            assert_eq!(route(event), Route::Forward, "{event:?}");
        }
    }
}
