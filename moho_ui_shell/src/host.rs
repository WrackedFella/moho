//! The engine's own egui host: one egui pass per frame, drawing the console.

use std::sync::Arc;

use moho_input::bindings::Binding;
use moho_render_api::{UiFrame, UiFrameSource};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::console::{Console, Routing};

/// Owns the egui context and the console it draws.
pub struct UiShell<G> {
    context: egui::Context,
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
            state: None,
            console,
        }
    }

    /// Creates the egui window state; call once from `Game::init`.
    pub fn attach(&mut self, _window: Arc<Window>) {
        let _ = (&self.context, &self.state);
    }

    /// Routes one key or button through the console. `on_window_event` delegates here, so this
    /// is the platform-free part of that routing.
    pub fn route(&mut self, _input: Binding, _pressed: bool, _repeat: bool) -> Routing {
        Routing::Game
    }

    /// Keyboard and mouse-button events go through the console; a toggle is not fed to egui,
    /// `Console` routing is, and non-input events are fed to egui and return `Game`.
    pub fn on_window_event(&mut self, _event: &WindowEvent) -> Routing {
        Routing::Game
    }

    /// Whether the console takes input, so the game drops device events.
    pub fn captures_input(&self) -> bool {
        self.console.is_open()
    }

    pub fn console_mut(&mut self) -> &mut Console<G> {
        &mut self.console
    }
}

impl<G> UiFrameSource for UiShell<G> {
    fn ui_frame(&mut self, _size_in_pixels: [u32; 2]) -> Option<UiFrame> {
        Some(UiFrame::default())
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
