//! The winit `ApplicationHandler` that drives a [`crate::Game`].

use std::sync::Arc;
use std::time::Instant;

use moho_audio::AudioSystem;
use moho_renderer::RendererBackend;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::{
    AppConfig, AppError, Event, EventContext, FrameContext, Game, HeadlessLoop, InitContext,
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
    let mut runner = Runner {
        sim: HeadlessLoop::new(config.loop_config.clone()),
        game,
        config,
        window: None,
        renderer: None,
        audio: None,
        last_wake: Instant::now(),
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
    sim: HeadlessLoop<G>,
    window: Option<Arc<Window>>,
    renderer: Option<Box<dyn RendererBackend>>,
    audio: Option<AudioSystem>,
    last_wake: Instant,
    error: Option<AppError>,
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
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        let Some(window) = &self.window else {
            return;
        };

        let now = Instant::now();
        let elapsed = now - self.last_wake;
        self.last_wake = now;

        if self.sim.run_ticks(&mut self.game, elapsed) > 0 {
            window.request_redraw();
        }

        let until_next_tick = self.sim.tick_length().mul_f32(1.0 - self.sim.alpha());
        event_loop.set_control_flow(ControlFlow::WaitUntil(now + until_next_tick));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let route = route(&event);
        match route {
            Route::Frame => {
                let mut ctx = FrameContext::new(
                    self.sim.tick_length(),
                    self.sim.clock(),
                    self.renderer.as_deref_mut(),
                    self.audio.as_mut(),
                );
                self.game.frame(&mut ctx, self.sim.alpha());
                if ctx.exit_requested() {
                    event_loop.exit();
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
        if ctx.exit_requested() || route == Route::Close {
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
        if ctx.exit_requested() {
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
