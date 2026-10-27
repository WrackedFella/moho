//! The engine's own egui host: one egui pass per frame, drawing the console and
//! the screen text and images a game queues.

use std::sync::Arc;

use moho_input::bindings::Input;
use moho_input::key::{Key, MouseButton};
use moho_render_api::{Rgba8Image, UiFrame, UiFrameSource};
use winit::event::{KeyEvent, WindowEvent};
use winit::window::Window;

use crate::console::{Console, Routing};
use crate::image::{ScreenImage, ScreenImageError, ScreenImageId, ScreenImages};
use crate::menu::MenuStack;
use crate::paint_map::to_ui_frame;
use crate::text::{self, ScreenText, ScreenTextError};

/// Owns the egui context, the console it draws and the menu screens a game with actions `A`
/// pushes.
pub struct UiShell<G, A> {
    context: egui::Context,
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    console: Console<G>,
    menu: MenuStack<A>,
    /// The cursor's last position, in logical points from the window's top left.
    cursor: [f32; 2],
    texts: Vec<ScreenText>,
    images: ScreenImages,
}

impl<G, A> std::fmt::Debug for UiShell<G, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiShell")
            .field("console_open", &self.console.is_open())
            .finish_non_exhaustive()
    }
}

impl<G, A> UiShell<G, A> {
    pub fn new(console: Console<G>) -> Self {
        Self {
            context: egui::Context::default(),
            window: None,
            state: None,
            console,
            menu: MenuStack::new(),
            cursor: [0.0, 0.0],
            texts: Vec::new(),
            images: ScreenImages::default(),
        }
    }

    pub fn menu_mut(&mut self) -> &mut MenuStack<A> {
        &mut self.menu
    }

    /// The cursor moved to `physical` pixels from the window's top left, on a window scaled by
    /// `scale`; hovers the menu in logical points unless the console is open over it.
    /// `on_window_event` delegates here with the attached window's scale factor, or 1 when none
    /// is attached.
    pub fn cursor_moved(&mut self, physical: [f64; 2], scale: f64) {
        self.cursor = [(physical[0] / scale) as f32, (physical[1] / scale) as f32];
        if !self.console.is_open() {
            let size = self.window_points();
            self.menu.hover(self.cursor, size);
        }
    }

    /// The window's size in logical points; 1280 by 720 when no window is attached.
    fn window_points(&self) -> [f32; 2] {
        match &self.window {
            Some(window) => {
                let size = window.inner_size().to_logical::<f32>(window.scale_factor());
                [size.width, size.height]
            }
            None => [1280.0, 720.0],
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

    /// Feeds an input event to egui when `routing`, from a prior `route` of that event, is
    /// `Console`. A toggle or a game input is not fed.
    pub fn feed(&mut self, event: &WindowEvent, routing: Routing) {
        if routing == Routing::Console {
            self.feed_egui(event);
        }
    }

    /// Whether the console or a menu screen takes input, so the game drops device events.
    pub fn captures_input(&self) -> bool {
        self.console.is_open() || self.menu.is_open()
    }

    pub fn console_mut(&mut self) -> &mut Console<G> {
        &mut self.console
    }

    /// Replaces the text the next `ui_frame` paints; that frame clears it. Call once per
    /// `Game::frame` with everything to show, so a frame the renderer skips leaves no stale text.
    ///
    /// # Errors
    ///
    /// [`ScreenTextError::BadSize`] when a text's size is not finite and positive; the shell then
    /// holds no text.
    pub fn set_texts(&mut self, texts: Vec<ScreenText>) -> Result<(), ScreenTextError> {
        self.texts.clear();
        text::validate(&texts)?;
        self.texts = texts;
        Ok(())
    }

    /// Uploads `image` to the UI once under `id`, replacing and freeing any image already
    /// there.
    pub fn add_image(&mut self, id: ScreenImageId, image: &Rgba8Image) {
        self.images.add(&self.context, id, image);
    }

    /// Frees the texture of the image under `id`, which the next frame no longer paints; an
    /// unknown id is ignored.
    pub fn remove_image(&mut self, id: ScreenImageId) {
        self.images.remove(id);
    }

    /// Replaces the images the next `ui_frame` paints; that frame clears them. Call once per
    /// `Game::frame` with everything to show, like [`set_texts`](Self::set_texts).
    ///
    /// # Errors
    ///
    /// [`ScreenImageError::UnknownImage`] when an image was never added or was removed, and
    /// [`ScreenImageError::BadSize`] when a side of its size is not finite and positive; the shell
    /// then holds no images.
    pub fn set_images(&mut self, images: Vec<ScreenImage>) -> Result<(), ScreenImageError> {
        self.images.set(images)
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

/// Routing chooses actions, so it clones them out of the screen.
impl<G, A: Clone> UiShell<G, A> {
    /// Routes one key or button through the console, then through the open menu screen, which
    /// takes every input, releases included. `on_window_event` delegates here, so this is the
    /// platform-free part of that routing.
    pub fn route(&mut self, input: Input, pressed: bool, repeat: bool) -> Routing {
        let routing = self.console.on_key(input, pressed, repeat);
        if routing != Routing::Game || !self.menu.is_open() {
            return routing;
        }
        if input == Input::Mouse(MouseButton::Left) && pressed && !repeat {
            let size = self.window_points();
            self.menu.click(self.cursor, size);
        } else {
            self.menu.on_key(input, pressed, repeat);
        }
        Routing::Menu
    }

    /// Routes a window event. Keys and mouse buttons go through the console and reach egui only
    /// when routed `Console` (so the toggle key is never typed); every other event is fed to
    /// egui and returns `Game`, so the game still sees resizes and focus changes. A cursor move
    /// also hovers the menu (see `cursor_moved`).
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
        if let WindowEvent::CursorMoved { position, .. } = event {
            let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
            self.cursor_moved([position.x, position.y], scale);
        }
        self.feed_egui(event);
        Routing::Game
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
pub fn input_of(event: &WindowEvent) -> Option<(Input, bool, bool)> {
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
            Input::Key(Key::from_winit(*physical_key)?),
            state.is_pressed(),
            *repeat,
        )),
        WindowEvent::MouseInput { state, button, .. } => Some((
            Input::Mouse(MouseButton::from_winit(*button)?),
            state.is_pressed(),
            false,
        )),
        _ => None,
    }
}

impl<G, A> UiFrameSource for UiShell<G, A> {
    fn ui_frame(&mut self, size_in_pixels: [u32; 2]) -> Option<UiFrame> {
        // Taken even while closed, so events fed to egui do not pile up unread.
        let input = self.take_input(size_in_pixels);
        if !self.console.is_open()
            && self.texts.is_empty()
            && !self.images.has_work()
            && !self.menu.is_open()
        {
            return None;
        }
        let context = self.context.clone();
        let output = context.run_ui(input, |ui| {
            // Game images, then game text, then the menu, so the console paints over all.
            self.images.paint(ui.ctx());
            text::paint(ui.ctx(), &self.texts);
            let screen = ui.ctx().content_rect().size();
            text::paint(ui.ctx(), &self.menu.texts([screen.x, screen.y]));
            if self.console.is_open() {
                self.console.show(ui.ctx());
            }
        });
        self.texts.clear();
        self.images.end_frame();
        // Platform output is dropped: handing it to egui-winit would show the cursor the game
        // grabbed. Copy, IME and cursor icons in the console go with it.
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
    use winit::dpi::PhysicalPosition;
    use winit::event::DeviceId;

    use super::*;
    use crate::console::tests::Cmd;
    use crate::menu::{MenuEvent, MenuItem, MenuScreen};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Choice {
        Play,
        Settings,
        Quit,
    }

    fn shell() -> UiShell<Cmd, Choice> {
        let (console, _) = Console::new(&BTreeMap::new());
        UiShell::new(console)
    }

    fn main_screen() -> MenuScreen<Choice> {
        let button = |label: &str, action| MenuItem::Button {
            label: label.to_owned(),
            action,
        };
        MenuScreen {
            title: None,
            items: vec![
                button("Play", Choice::Play),
                button("Settings", Choice::Settings),
                button("Quit", Choice::Quit),
            ],
            back: None,
        }
    }

    fn menu_shell() -> UiShell<Cmd, Choice> {
        let mut shell = shell();
        shell.menu_mut().push(main_screen());
        shell
    }

    /// The windowless shell's window is 1280 by 720 points; the main screen's rows centre on x
    /// 640 and on these y values.
    const ROW_Y: [f64; 3] = [304.0, 360.0, 416.0];

    fn cursor_at(shell: &mut UiShell<Cmd, Choice>, at: [f64; 2]) {
        let routing = shell.on_window_event(&WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(at[0], at[1]),
        });
        assert_eq!(routing, Routing::Game, "a cursor move is not an input");
    }

    fn chosen(shell: &mut UiShell<Cmd, Choice>) -> Vec<Choice> {
        shell
            .menu_mut()
            .take_events()
            .into_iter()
            .map(|event| match event {
                MenuEvent::Chosen(choice) => choice,
            })
            .collect()
    }

    #[test]
    fn hovering_a_row_focuses_it_and_a_click_chooses_it() {
        let mut shell = menu_shell();
        let left = Input::Mouse(MouseButton::Left);

        cursor_at(&mut shell, [640.0, ROW_Y[2]]);
        let enter = shell.route(Input::Key(Key::Enter), true, false);
        let focused = chosen(&mut shell);
        let press = shell.route(left, true, false);
        let release = shell.route(left, false, false);
        let clicked = chosen(&mut shell);
        cursor_at(&mut shell, [640.0, ROW_Y[1]]);
        shell.route(left, true, false);
        let clicked_settings = chosen(&mut shell);
        cursor_at(&mut shell, [5.0, 5.0]);
        shell.route(left, true, false);
        let missed = chosen(&mut shell);

        assert_eq!(
            (enter, press, release),
            (Routing::Menu, Routing::Menu, Routing::Menu)
        );
        assert_eq!(focused, [Choice::Quit], "hover moved the focus");
        assert_eq!(clicked, [Choice::Quit], "only the press chooses");
        assert_eq!(clicked_settings, [Choice::Settings]);
        assert_eq!(missed, []);
    }

    #[test]
    fn an_open_menu_takes_every_key_and_button_including_releases() {
        let mut shell = menu_shell();
        let inputs = [
            Input::Key(Key::W),
            Input::Key(Key::Space),
            Input::Key(Key::Tab),
            Input::Key(Key::ArrowDown),
            Input::Mouse(MouseButton::Left),
            Input::Mouse(MouseButton::Right),
        ];

        for input in inputs {
            for (pressed, repeat) in [(true, false), (true, true), (false, false)] {
                let routing = shell.route(input, pressed, repeat);

                assert_eq!(
                    routing,
                    Routing::Menu,
                    "{input:?} pressed {pressed} repeat {repeat}"
                );
            }
        }
    }

    #[test]
    fn captures_input_is_true_while_a_menu_is_open() {
        let mut shell = shell();
        assert!(!shell.captures_input());

        shell.menu_mut().push(main_screen());
        let open = shell.captures_input();
        shell.menu_mut().clear();

        assert!(open);
        assert!(!shell.captures_input());
    }

    #[test]
    fn the_console_key_opens_the_console_over_a_menu() {
        let mut shell = menu_shell();
        let backtick = Input::Key(Key::Backtick);

        let opening = shell.route(backtick, true, false);
        let typed = shell.route(Input::Key(Key::Enter), true, false);
        let closing = shell.route(backtick, true, false);
        let after = shell.route(Input::Key(Key::Enter), true, false);

        assert_eq!(opening, Routing::Opened);
        assert_eq!(typed, Routing::Console);
        assert_eq!(closing, Routing::Closed);
        assert_eq!(after, Routing::Menu);
        assert_eq!(
            chosen(&mut shell),
            [Choice::Play],
            "the console kept its Enter"
        );
        assert!(shell.menu_mut().is_open());
    }

    #[test]
    fn a_cursor_moving_under_an_open_console_keeps_the_menus_focus() {
        let mut shell = menu_shell();
        shell.route(Input::Key(Key::Backtick), true, false);

        cursor_at(&mut shell, [640.0, ROW_Y[2]]);
        shell.route(Input::Key(Key::Backtick), true, false);
        shell.route(Input::Key(Key::Enter), true, false);

        assert_eq!(chosen(&mut shell), [Choice::Play]);
    }

    #[test]
    fn a_cursor_in_physical_pixels_hits_rows_laid_out_in_points() {
        let mut shell = menu_shell();

        shell.cursor_moved([1280.0, 832.0], 2.0);
        shell.route(Input::Key(Key::Enter), true, false);
        let scaled = chosen(&mut shell);
        shell.cursor_moved([1280.0, 720.0], 2.0);
        shell.route(Input::Key(Key::Enter), true, false);
        let other_row = chosen(&mut shell);
        cursor_at(&mut shell, [640.0, ROW_Y[0]]);
        shell.route(Input::Key(Key::Enter), true, false);
        let unscaled = chosen(&mut shell);

        assert_eq!(scaled, [Choice::Quit]);
        assert_eq!(other_row, [Choice::Settings]);
        assert_eq!(unscaled, [Choice::Play], "a windowless shell has scale 1");
    }

    #[test]
    fn an_open_menu_draws_a_frame_and_an_empty_stack_draws_none() {
        let mut shell = shell();
        let empty = shell.ui_frame([1280, 720]);

        shell.menu_mut().push(main_screen());
        let open = shell.ui_frame([1280, 720]);
        shell.menu_mut().clear();
        let closed = shell.ui_frame([1280, 720]);

        assert_eq!(empty, None);
        let open = open.expect("an open menu draws a frame");
        assert!(!open.meshes.is_empty());
        assert!(open.meshes.iter().all(|m| !m.vertices.is_empty()));
        assert_eq!(closed, None);
    }

    #[test]
    fn moving_the_focus_changes_the_frame_that_is_drawn() {
        let mut shell = menu_shell();
        let first = shell.ui_frame([1280, 720]).expect("an open menu draws");

        shell.route(Input::Key(Key::ArrowDown), true, false);
        let second = shell.ui_frame([1280, 720]).expect("an open menu draws");

        assert_ne!(first.meshes, second.meshes, "the highlight did not move");
    }

    #[test]
    fn the_toggle_key_is_not_fed_to_egui() {
        let mut shell = shell();
        let backtick = Input::Key(Key::Backtick);

        let opening = shell.route(backtick, true, false);
        let typed = shell.route(Input::Key(Key::W), true, false);
        let closing = shell.route(backtick, true, false);

        assert_eq!(opening, Routing::Opened);
        assert_eq!(typed, Routing::Console);
        assert_eq!(closing, Routing::Closed);
    }

    #[test]
    fn a_focus_loss_with_the_console_open_goes_to_the_game() {
        let mut shell = shell();
        shell.route(Input::Key(Key::Backtick), true, false);

        let routing = shell.on_window_event(&WindowEvent::Focused(false));

        assert_eq!(routing, Routing::Game);
        assert!(shell.captures_input());
    }

    #[test]
    fn an_open_console_captures_input_and_a_closed_one_does_not() {
        let mut shell = shell();
        assert!(!shell.captures_input());

        shell.route(Input::Key(Key::Backtick), true, false);

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

    #[test]
    fn the_open_console_fills_the_bottom_300_points_of_the_screen_width() {
        let mut shell = shell();
        shell.console_mut().open();

        let frame = shell.ui_frame([800, 600]).expect("an open console draws");

        let positions: Vec<[f32; 2]> = frame
            .meshes
            .iter()
            .flat_map(|m| m.vertices.iter().map(|v| v.pos))
            .collect();
        let min_x = positions.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        let max_x = positions.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        let min_y = positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let max_y = positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert_eq!(frame.pixels_per_point, 1.0);
        assert!(min_x.abs() <= 1.0, "left edge at {min_x}");
        assert!((max_x - 800.0).abs() <= 1.0, "right edge at {max_x}");
        assert!((min_y - 300.0).abs() <= 1.0, "top edge at {min_y}");
        assert!((max_y - 600.0).abs() <= 1.0, "bottom edge at {max_y}");
    }

    #[test]
    fn a_long_log_scrolls_inside_a_234_point_window_above_the_input_line() {
        let mut shell = shell();
        shell.console_mut().open();
        for i in 0..60 {
            shell.console_mut().submit(&format!("line {i}"));
        }

        let frame = shell.ui_frame([800, 600]).expect("an open console draws");

        let log = frame
            .meshes
            .iter()
            .max_by_key(|m| m.vertices.len())
            .expect("the log is drawn");
        let height = log.clip_rect.max[1] - log.clip_rect.min[1];
        assert!((height - 234.0).abs() <= 1.0, "log window is {height} tall");
        assert!(log.clip_rect.min[1] >= 300.0, "log starts above the panel");
        assert!(log.clip_rect.max[1] <= 600.0, "log ends below the panel");
    }

    fn ammo() -> ScreenText {
        ScreenText {
            text: "30 / 30".to_owned(),
            anchor: crate::text::Anchor::BottomRight,
            offset: [-10.0, -10.0],
            size: 24.0,
            color: [255, 255, 255, 255],
        }
    }

    #[test]
    fn set_text_yields_meshes_on_the_font_texture() {
        let mut shell = shell();
        shell.set_texts(vec![ammo()]).expect("valid texts");

        let frame = shell.ui_frame([800, 600]);

        let frame = frame.expect("set text draws a frame with the console closed");
        assert!(!frame.meshes.is_empty());
        assert!(!frame.textures_set.is_empty());
        assert!(frame.meshes.iter().all(|m| !m.vertices.is_empty()));
        assert!(
            frame
                .meshes
                .iter()
                .all(|m| frame.textures_set.iter().any(|s| s.id == m.texture))
        );
    }

    #[test]
    fn set_text_is_cleared_by_the_frame_that_paints_it() {
        let mut shell = shell();
        shell.set_texts(vec![ammo()]).expect("valid texts");
        assert!(shell.ui_frame([800, 600]).is_some());

        let next = shell.ui_frame([800, 600]);

        assert_eq!(next, None);
    }

    #[test]
    fn a_skipped_frame_leaves_only_the_latest_text() {
        let mut fresh = shell();
        let mut shell = shell();
        let latest = ScreenText {
            text: "29 / 30".to_owned(),
            ..ammo()
        };
        shell
            .set_texts(vec![ScreenText {
                text: "30 / 30".to_owned(),
                ..ammo()
            }])
            .expect("valid texts");
        shell.set_texts(vec![latest.clone()]).expect("valid texts");
        fresh.set_texts(vec![latest]).expect("valid texts");

        let frame = shell.ui_frame([800, 600]).expect("the latest text draws");

        let expected = fresh.ui_frame([800, 600]).expect("the fresh shell draws");
        assert!(!frame.meshes.is_empty());
        assert_eq!(frame.meshes, expected.meshes);
    }

    fn text_of(text: &str, anchor: crate::text::Anchor) -> ScreenText {
        ScreenText {
            text: text.to_owned(),
            anchor,
            offset: [0.0, 0.0],
            size: 24.0,
            color: [255, 255, 255, 255],
        }
    }

    #[test]
    fn every_text_set_this_frame_is_drawn() {
        use crate::text::Anchor;
        let mut shell = shell();
        shell
            .set_texts(vec![
                text_of("alpha", Anchor::TopLeft),
                text_of("bravo", Anchor::Top),
                text_of("charlie", Anchor::BottomRight),
            ])
            .expect("valid texts");

        let frame = shell.ui_frame([800, 600]);

        let frame = frame.expect("set texts draw a frame");
        assert!(!frame.meshes.is_empty());
        assert!(frame.meshes.iter().all(|m| !m.vertices.is_empty()));
    }

    #[test]
    fn set_text_clears_when_the_console_is_open_too() {
        let mut shell = shell();
        shell.set_texts(vec![ammo()]).expect("valid texts");
        shell.console_mut().open();
        assert!(shell.ui_frame([800, 600]).is_some());
        shell.console_mut().close();

        let next = shell.ui_frame([800, 600]);

        assert_eq!(next, None);
    }

    #[test]
    fn a_refused_list_replaces_the_text_with_none() {
        for size in [0.0, -5.0, f32::NAN] {
            let mut shell = shell();
            shell.set_texts(vec![ammo()]).expect("a valid list is set");

            let refused = shell.set_texts(vec![ammo(), ScreenText { size, ..ammo() }]);

            assert!(refused.is_err(), "size {size} was accepted");
            assert_eq!(shell.ui_frame([800, 600]), None, "size {size}");
        }
    }

    #[test]
    fn empty_text_draws_no_meshes_and_does_not_panic_the_frame() {
        let mut shell = shell();
        shell
            .set_texts(vec![ScreenText {
                text: String::new(),
                ..ammo()
            }])
            .expect("valid texts");

        let frame = shell.ui_frame([800, 600]);

        let frame = frame.expect("a set text still runs the frame");
        assert!(frame.meshes.is_empty());
    }

    #[test]
    fn the_console_shell_debug_names_the_shell_and_whether_the_console_is_open() {
        let mut shell = shell();
        let closed = format!("{shell:?}");

        shell.console_mut().open();
        let open = format!("{shell:?}");

        assert!(closed.contains("UiShell"), "{closed}");
        assert!(closed.contains("console_open: false"), "{closed}");
        assert!(open.contains("console_open: true"), "{open}");
    }
}
