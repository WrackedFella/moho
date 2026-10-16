//! The engine's own egui host: one egui pass per frame, drawing the console.

use std::sync::Arc;

use moho_input::bindings::Binding;
use moho_input::key::{Key, MouseButton};
use moho_render_api::{UiFrame, UiFrameSource};
use winit::event::{KeyEvent, WindowEvent};
use winit::window::Window;

use crate::console::{Console, Routing};
use crate::paint_map::to_ui_frame;

/// Owns the egui context and the console it draws.
pub struct UiShell<G> {
    context: egui::Context,
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    console: Console<G>,
}

impl<G> std::fmt::Debug for UiShell<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiShell")
            .field("console_open", &self.console.is_open())
            .finish_non_exhaustive()
    }
}

impl<G> UiShell<G> {
    pub fn new(console: Console<G>) -> Self {
        Self {
            context: egui::Context::default(),
            window: None,
            state: None,
            console,
        }
    }

    /// Creates the egui window state; call once from `Game::init`.
    pub fn attach(&mut self, window: Arc<Window>) {
        self.state = Some(egui_winit::State::new(
            self.context.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            None,
            None,
            None,
        ));
        self.window = Some(window);
    }

    /// Routes one key or button through the console. `on_window_event` delegates here, so this
    /// is the platform-free part of that routing.
    pub fn route(&mut self, input: Binding, pressed: bool, repeat: bool) -> Routing {
        self.console.on_key(input, pressed, repeat)
    }

    /// Keyboard and mouse-button events go through the console; a toggle is not fed to egui,
    /// `Console` routing is, and non-input events are fed to egui and return `Game`.
    pub fn on_window_event(&mut self, event: &WindowEvent) -> Routing {
        if let Some((input, pressed, repeat)) = input_of(event) {
            let routing = self.route(input, pressed, repeat);
            self.feed(event, routing);
            return routing;
        }
        if is_input_event(event) {
            // An input the engine does not name (a dead key, say) can still be typed into the
            // console.
            let routing = if self.console.is_open() {
                Routing::Console
            } else {
                Routing::Game
            };
            self.feed(event, routing);
            return routing;
        }
        self.feed_egui(event);
        Routing::Game
    }

    /// Feeds an input event to egui when `routing`, from a prior `route` of that event, is
    /// `Console`. A toggle or a game input is not fed.
    pub fn feed(&mut self, event: &WindowEvent, routing: Routing) {
        if routing == Routing::Console {
            self.feed_egui(event);
        }
    }

    /// Whether the console takes input, so the game drops device events.
    pub fn captures_input(&self) -> bool {
        self.console.is_open()
    }

    pub fn console_mut(&mut self) -> &mut Console<G> {
        &mut self.console
    }

    fn feed_egui(&mut self, event: &WindowEvent) {
        if let (Some(state), Some(window)) = (&mut self.state, &self.window) {
            // Whether egui consumed the event is moot: the console already decided its routing.
            let response = state.on_window_event(window, event);
            if response.repaint {
                self.context.request_repaint();
            }
        }
    }

    fn take_input(&mut self, size_in_pixels: [u32; 2]) -> egui::RawInput {
        match (&mut self.state, &self.window) {
            (Some(state), Some(window)) => state.take_egui_input(window),
            _ => egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size_in_pixels[0] as f32, size_in_pixels[1] as f32),
                )),
                ..egui::RawInput::default()
            },
        }
    }
}

fn is_input_event(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. }
    )
}

/// The key or mouse button a window event presses or releases, as `(input, pressed, repeat)`;
/// `None` for any other event and for inputs the engine does not name.
pub fn input_of(event: &WindowEvent) -> Option<(Binding, bool, bool)> {
    match event {
        WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key,
                    state,
                    repeat,
                    ..
                },
            ..
        } => Some((
            Binding::Key(Key::from_winit(*physical_key)?),
            state.is_pressed(),
            *repeat,
        )),
        WindowEvent::MouseInput { state, button, .. } => Some((
            Binding::Mouse(MouseButton::from_winit(*button)?),
            state.is_pressed(),
            false,
        )),
        _ => None,
    }
}

impl<G> UiFrameSource for UiShell<G> {
    fn ui_frame(&mut self, size_in_pixels: [u32; 2]) -> Option<UiFrame> {
        // Taken even while closed, so events fed to egui do not pile up unread.
        let input = self.take_input(size_in_pixels);
        if !self.console.is_open() {
            return None;
        }
        let context = self.context.clone();
        let output = context.run_ui(input, |ui| self.console.show(&ui.ctx().clone()));
        let primitives = context.tessellate(output.shapes, output.pixels_per_point);
        Some(to_ui_frame(
            &primitives,
            output.textures_delta,
            output.pixels_per_point,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use moho_input::key::Key;

    use super::*;
    use crate::console::tests::Cmd;

    fn shell() -> UiShell<Cmd> {
        let (console, _) = Console::new(&BTreeMap::new());
        UiShell::new(console)
    }

    #[test]
    fn the_toggle_key_is_not_fed_to_egui() {
        let mut shell = shell();
        let backtick = Binding::Key(Key::Backtick);

        let opening = shell.route(backtick, true, false);
        let typed = shell.route(Binding::Key(Key::W), true, false);
        let closing = shell.route(backtick, true, false);

        assert_eq!(opening, Routing::Opened);
        assert_eq!(typed, Routing::Console);
        assert_eq!(closing, Routing::Closed);
    }

    #[test]
    fn a_focus_loss_with_the_console_open_goes_to_the_game() {
        let mut shell = shell();
        shell.route(Binding::Key(Key::Backtick), true, false);

        let routing = shell.on_window_event(&WindowEvent::Focused(false));

        assert_eq!(routing, Routing::Game);
        assert!(shell.captures_input());
    }

    #[test]
    fn an_open_console_captures_input_and_a_closed_one_does_not() {
        let mut shell = shell();
        assert!(!shell.captures_input());

        shell.route(Binding::Key(Key::Backtick), true, false);

        assert!(shell.captures_input());
    }

    #[test]
    fn a_closed_console_draws_no_frame() {
        let mut shell = shell();

        assert_eq!(shell.ui_frame([800, 600]), None);
    }

    #[test]
    fn an_open_console_frame_has_meshes_and_the_font_texture() {
        let mut shell = shell();
        shell.console_mut().open();

        let frame = shell.ui_frame([800, 600]);

        let frame = frame.expect("an open console draws a frame");
        assert!(!frame.meshes.is_empty());
        assert!(!frame.textures_set.is_empty());
        assert!(frame.meshes.iter().all(|m| !m.vertices.is_empty()));
    }
}
