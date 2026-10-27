//! Menu screens a game describes as data: labelled buttons the engine draws and drives with the
//! mouse and keyboard. Screens stack, each keeping its own focus. The stack never changes itself:
//! a choice becomes a [`MenuEvent`] the game answers by pushing or popping.

use moho_input::bindings::Input;
use moho_input::key::{Key, MouseButton};

use crate::settings::{Control, EngineSetting, EngineSettings};
use crate::text::{Anchor, ScreenText};

/// One screen: an optional title, a column of items and the action Escape chooses.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuScreen<A> {
    pub title: Option<String>,
    pub items: Vec<MenuItem<A>>,
    /// What Escape chooses on this screen; `None` makes Escape do nothing.
    pub back: Option<A>,
    /// The values the screen's settings rows start from; `None` on a screen without them.
    pub settings: Option<EngineSettings>,
}

/// One row of a screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem<A> {
    /// A labelled button that chooses `action`.
    Button { label: String, action: A },
    /// One of the engine's settings rows, changing the values staged on its screen.
    Setting(EngineSetting),
    /// Hands the game the staged settings values.
    Apply,
}

/// What the player did to the open screens, for the game to answer.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEvent<A> {
    /// The player chose an item, or pressed Escape on a screen with a back action.
    Chosen(A),
    /// The player applied the settings staged on the top screen.
    SettingsApplied(EngineSettings),
}

/// An axis-aligned rectangle in logical points from the window's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// Font size of every row and title.
const ROW_SIZE: f32 = 36.0;
/// Distance between row centres, in logical points; the bands are this tall and touch.
const ROW_PITCH: f32 = 56.0;
/// Width of a row's clickable band, centred on the window.
const ROW_WIDTH: f32 = 240.0;
const FOCUS_COLOR: [u8; 4] = [255, 220, 80, 255];
const COLOR: [u8; 4] = [220, 220, 220, 255];

/// Font size of a settings row's label and value, which share a 240-point band.
const SETTING_SIZE: f32 = 18.0;

/// A screen, the row that has the focus on it and the settings values staged on it.
#[derive(Debug)]
struct Open<A> {
    screen: MenuScreen<A>,
    focus: usize,
    /// A copy of the screen's `settings` that the rows edit until Apply.
    staged: Option<EngineSettings>,
}

impl<A: Clone> Open<A> {
    /// What choosing `row` does: a button chooses its action, a settings row flips or cycles
    /// its staged value, Apply hands over the staged values and makes them the baseline.
    fn choose(&mut self, row: usize) -> Option<MenuEvent<A>> {
        match self.screen.items.get(row)? {
            MenuItem::Button { action, .. } => Some(MenuEvent::Chosen(action.clone())),
            MenuItem::Setting(setting) => {
                if let Some(staged) = &mut self.staged {
                    cycle(staged, *setting);
                }
                None
            }
            MenuItem::Apply => {
                let staged = self.staged?;
                self.screen.settings = Some(staged);
                Some(MenuEvent::SettingsApplied(staged))
            }
        }
    }
}

impl<A> Open<A> {
    /// Moves the staged value of the settings row `row` one step towards `direction`.
    fn nudge(&mut self, row: usize, direction: i8) {
        if let (Some(staged), Some(MenuItem::Setting(setting))) =
            (&mut self.staged, self.screen.items.get(row))
        {
            match setting.control() {
                Control::Slider { step, .. } => {
                    let value = staged.slider_value(*setting) + f32::from(direction) * step;
                    staged.set_slider(*setting, value);
                }
                Control::Toggle => staged.flip(*setting),
                Control::Choice => {
                    let last = staged.choices(*setting).len().saturating_sub(1);
                    let at = staged.choice_index(*setting);
                    let moved = if direction < 0 {
                        at.saturating_sub(1)
                    } else {
                        (at + 1).min(last)
                    };
                    staged.set_choice_index(*setting, moved);
                }
            }
        }
    }

    /// Sets the staged slider on `row` from where `x` lies along the row's band.
    fn set_slider_from(&mut self, row: usize, x: f32, size: [f32; 2]) {
        if let (Some(staged), Some(MenuItem::Setting(setting)), Some(rect)) = (
            &mut self.staged,
            self.screen.items.get(row),
            layout(&self.screen, size).get(row),
        ) && let Control::Slider { min, max, .. } = setting.control()
        {
            let fraction = ((x - rect.min[0]) / (rect.max[0] - rect.min[0])).clamp(0.0, 1.0);
            staged.set_slider(*setting, min + fraction * (max - min));
        }
    }
}

/// Flips a toggle and advances a choice, wrapping; a slider does nothing.
fn cycle(staged: &mut EngineSettings, setting: EngineSetting) {
    match setting.control() {
        Control::Slider { .. } => {}
        Control::Toggle => staged.flip(setting),
        Control::Choice => {
            let count = staged.choices(setting).len();
            staged.set_choice_index(setting, (staged.choice_index(setting) + 1) % count);
        }
    }
}

/// The open screens, bottom to top, and the events not yet taken.
#[derive(Debug)]
pub struct MenuStack<A> {
    screens: Vec<Open<A>>,
    events: Vec<MenuEvent<A>>,
    /// The slider row of the top screen a held mouse button is dragging.
    dragging: Option<usize>,
}

impl<A> Default for MenuStack<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> MenuStack<A> {
    pub fn new() -> Self {
        Self {
            screens: Vec::new(),
            events: Vec::new(),
            dragging: None,
        }
    }

    /// Opens `screen` over the current one, focusing its first item and staging a copy of its
    /// settings values.
    pub fn push(&mut self, screen: MenuScreen<A>) {
        self.dragging = None;
        let staged = screen.settings;
        self.screens.push(Open {
            screen,
            focus: 0,
            staged,
        });
    }

    /// Closes the top screen, discarding the values staged on it; the one below keeps the focus
    /// it had.
    pub fn pop(&mut self) {
        self.dragging = None;
        self.screens.pop();
    }

    /// Closes every screen.
    pub fn clear(&mut self) {
        self.dragging = None;
        self.screens.clear();
    }

    /// Whether a held mouse button is dragging a slider.
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Moves the dragged slider to `point`, in logical points on a window `size` points across.
    pub fn drag(&mut self, point: [f32; 2], size: [f32; 2]) {
        if let (Some(open), Some(row)) = (self.screens.last_mut(), self.dragging) {
            open.set_slider_from(row, point[0], size);
        }
    }

    pub fn is_open(&self) -> bool {
        !self.screens.is_empty()
    }

    pub fn top(&self) -> Option<&MenuScreen<A>> {
        self.screens.last().map(|open| &open.screen)
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<MenuEvent<A>> {
        std::mem::take(&mut self.events)
    }

    /// Focuses the row under `point`, in logical points on a window `size` points across;
    /// a point over no row keeps the focus.
    pub fn hover(&mut self, point: [f32; 2], size: [f32; 2]) {
        if let Some(open) = self.screens.last_mut()
            && let Some(row) = row_at(&open.screen, point, size)
        {
            open.focus = row;
        }
    }

    /// The text of the top screen on a window `size` points across: its title above the rows,
    /// then one centre-anchored text per row with the focused one in its own colour.
    pub(crate) fn texts(&self, size: [f32; 2]) -> Vec<ScreenText> {
        let Some(open) = self.screens.last() else {
            return Vec::new();
        };
        let rows = layout(&open.screen, size);
        let text =
            |text: &str, anchor, x: f32, centre_y: f32, text_size, color: [u8; 4]| ScreenText {
                text: text.to_owned(),
                anchor,
                offset: [x, centre_y - size[1] / 2.0],
                size: text_size,
                color,
            };
        let centred = |label: &str, centre_y: f32, color| {
            text(label, Anchor::Center, 0.0, centre_y, ROW_SIZE, color)
        };
        let mut texts = Vec::new();
        if let Some(title) = &open.screen.title {
            let first_centre = rows.first().map_or(size[1] / 2.0, |r| centre_y(*r));
            texts.push(centred(title, first_centre - ROW_PITCH, COLOR));
        }
        for (row, (item, rect)) in open.screen.items.iter().zip(rows).enumerate() {
            let color = if row == open.focus {
                FOCUS_COLOR
            } else {
                COLOR
            };
            match item {
                MenuItem::Button { label, .. } => texts.push(centred(label, centre_y(rect), color)),
                MenuItem::Apply => texts.push(centred("Apply", centre_y(rect), color)),
                MenuItem::Setting(setting) => {
                    let y = centre_y(rect);
                    texts.push(text(
                        setting.label(),
                        Anchor::Left,
                        rect.min[0],
                        y,
                        SETTING_SIZE,
                        color,
                    ));
                    if let Some(staged) = &open.staged {
                        texts.push(text(
                            &staged.value_text(*setting),
                            Anchor::Right,
                            rect.max[0] - size[0],
                            y,
                            SETTING_SIZE,
                            color,
                        ));
                    }
                }
            }
        }
        texts
    }
}

impl<A: Clone> MenuStack<A> {
    /// Up and Down move the focus and wrap; Left and Right step the focused settings row; a
    /// non-repeat Enter press chooses the focused item (flipping or cycling a settings row);
    /// a non-repeat Escape press chooses the top screen's back action, if any. Releasing the
    /// left mouse button ends a slider drag; other releases and inputs do nothing.
    pub fn on_key(&mut self, input: Input, pressed: bool, repeat: bool) {
        let Some(open) = self.screens.last_mut() else {
            return;
        };
        if !pressed {
            if input == Input::Mouse(MouseButton::Left) {
                self.dragging = None;
            }
            return;
        }
        let count = open.screen.items.len();
        match input {
            Input::Key(Key::ArrowDown) if count > 0 => open.focus = (open.focus + 1) % count,
            Input::Key(Key::ArrowUp) if count > 0 => {
                open.focus = (open.focus + count - 1) % count;
            }
            Input::Key(Key::ArrowLeft) => open.nudge(open.focus, -1),
            Input::Key(Key::ArrowRight) => open.nudge(open.focus, 1),
            Input::Key(Key::Enter) if !repeat => {
                self.events.extend(open.choose(open.focus));
            }
            Input::Key(Key::Escape) if !repeat => {
                if let Some(back) = open.screen.back.clone() {
                    self.events.push(MenuEvent::Chosen(back));
                }
            }
            _ => {}
        }
    }

    /// Chooses the row under `point` (see [`MenuStack::hover`]); a point over no row chooses
    /// nothing. A press on a slider row sets it from `point` and starts a drag that
    /// [`MenuStack::drag`] follows until the left button is released.
    pub fn click(&mut self, point: [f32; 2], size: [f32; 2]) {
        if let Some(open) = self.screens.last_mut()
            && let Some(row) = row_at(&open.screen, point, size)
        {
            let on_slider = matches!(
                open.screen.items[row],
                MenuItem::Setting(setting) if matches!(setting.control(), Control::Slider { .. })
            );
            if on_slider {
                self.dragging = Some(row);
                open.set_slider_from(row, point[0], size);
            } else {
                self.events.extend(open.choose(row));
            }
        }
    }
}

fn centre_y(rect: Rect) -> f32 {
    (rect.min[1] + rect.max[1]) / 2.0
}

/// The row of `screen` strictly inside which `point` lies.
fn row_at<A>(screen: &MenuScreen<A>, point: [f32; 2], size: [f32; 2]) -> Option<usize> {
    layout(screen, size).iter().position(|r| {
        point[0] > r.min[0] && point[0] < r.max[0] && point[1] > r.min[1] && point[1] < r.max[1]
    })
}

/// The row rectangles of `screen` on a window `size` points across, top to bottom: bands 240
/// points wide, centred on the window, one 56-point pitch apart, the column centred vertically.
/// Painting and hit tests both read this.
pub fn layout<A>(screen: &MenuScreen<A>, size: [f32; 2]) -> Vec<Rect> {
    let top = size[1] / 2.0 - screen.items.len() as f32 * ROW_PITCH / 2.0;
    (0..screen.items.len())
        .map(|row| {
            let y = top + row as f32 * ROW_PITCH;
            Rect {
                min: [size[0] / 2.0 - ROW_WIDTH / 2.0, y],
                max: [size[0] / 2.0 + ROW_WIDTH / 2.0, y + ROW_PITCH],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use moho_input::key::{Key, MouseButton};

    use super::*;

    const SIZE: [f32; 2] = [1280.0, 720.0];

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Act {
        Play,
        Settings,
        Quit,
        Back,
    }

    fn button(label: &str, action: Act) -> MenuItem<Act> {
        MenuItem::Button {
            label: label.to_owned(),
            action,
        }
    }

    fn main_screen() -> MenuScreen<Act> {
        MenuScreen {
            title: None,
            items: vec![
                button("Play", Act::Play),
                button("Settings", Act::Settings),
                button("Quit", Act::Quit),
            ],
            back: None,
            settings: None,
        }
    }

    fn settings_screen() -> MenuScreen<Act> {
        MenuScreen {
            title: Some("Settings".to_owned()),
            items: vec![button("Back", Act::Back)],
            back: Some(Act::Back),
            settings: None,
        }
    }

    fn stack_of(screen: MenuScreen<Act>) -> MenuStack<Act> {
        let mut stack = MenuStack::new();
        stack.push(screen);
        stack
    }

    fn press(stack: &mut MenuStack<Act>, key: Key) {
        stack.on_key(Input::Key(key), true, false);
    }

    fn chosen(stack: &mut MenuStack<Act>) -> Vec<Act> {
        stack
            .take_events()
            .into_iter()
            .filter_map(|event| match event {
                MenuEvent::Chosen(action) => Some(action),
                MenuEvent::SettingsApplied(_) => None,
            })
            .collect()
    }

    /// The row centres of the three-item main screen on `SIZE`.
    const ROW_Y: [f32; 3] = [304.0, 360.0, 416.0];

    #[test]
    fn down_and_up_move_the_focus_and_wrap() {
        let mut stack = stack_of(main_screen());

        press(&mut stack, Key::ArrowDown);
        press(&mut stack, Key::Enter);
        let down_once = chosen(&mut stack);
        press(&mut stack, Key::ArrowDown);
        press(&mut stack, Key::ArrowDown);
        press(&mut stack, Key::Enter);
        let wrapped_down = chosen(&mut stack);
        press(&mut stack, Key::ArrowUp);
        press(&mut stack, Key::Enter);
        let wrapped_up = chosen(&mut stack);
        stack.on_key(Input::Key(Key::ArrowUp), true, true);
        press(&mut stack, Key::Enter);
        let repeated_up = chosen(&mut stack);

        assert_eq!(down_once, [Act::Settings]);
        assert_eq!(wrapped_down, [Act::Play]);
        assert_eq!(wrapped_up, [Act::Quit]);
        assert_eq!(repeated_up, [Act::Settings], "a repeat still moves");
    }

    #[test]
    fn enter_chooses_the_first_item_before_any_movement() {
        let mut stack = stack_of(main_screen());

        press(&mut stack, Key::Enter);

        assert_eq!(chosen(&mut stack), [Act::Play]);
        assert!(stack.is_open(), "the stack never closes itself");
    }

    #[test]
    fn escape_chooses_the_top_screens_back_action() {
        let mut stack = stack_of(main_screen());
        stack.push(settings_screen());

        press(&mut stack, Key::Escape);

        assert_eq!(chosen(&mut stack), [Act::Back]);
        assert_eq!(stack.top(), Some(&settings_screen()), "no hidden pop");
    }

    #[test]
    fn escape_without_a_back_action_does_nothing() {
        let mut stack = stack_of(main_screen());

        press(&mut stack, Key::Escape);

        assert_eq!(chosen(&mut stack), []);
        assert_eq!(stack.top(), Some(&main_screen()));
    }

    #[test]
    fn pop_returns_to_the_previous_screens_focus() {
        let mut stack = stack_of(main_screen());
        press(&mut stack, Key::ArrowDown);
        stack.push(settings_screen());
        press(&mut stack, Key::ArrowDown);

        stack.pop();
        press(&mut stack, Key::Enter);

        assert_eq!(stack.top(), Some(&main_screen()));
        assert_eq!(chosen(&mut stack), [Act::Settings]);
    }

    #[test]
    fn a_pushed_screen_starts_with_its_first_item_focused() {
        let mut stack = stack_of(main_screen());
        press(&mut stack, Key::ArrowUp);
        stack.push(main_screen());

        press(&mut stack, Key::Enter);

        assert_eq!(chosen(&mut stack), [Act::Play]);
    }

    #[test]
    fn a_repeated_enter_or_a_click_outside_every_row_chooses_nothing() {
        let mut stack = stack_of(main_screen());
        stack.push(settings_screen());
        press(&mut stack, Key::ArrowUp);
        stack.pop();
        press(&mut stack, Key::ArrowUp);

        stack.on_key(Input::Key(Key::Enter), true, true);
        stack.on_key(Input::Key(Key::Enter), false, false);
        stack.on_key(Input::Key(Key::Escape), true, true);
        stack.on_key(Input::Key(Key::Escape), false, false);
        stack.on_key(Input::Mouse(MouseButton::Left), true, false);
        for point in [
            [0.0, 0.0],
            [640.0, ROW_Y[2] + 56.0],
            [640.0, ROW_Y[0] - 56.0],
            [640.0 + 120.5, ROW_Y[1]],
            [640.0 - 120.5, ROW_Y[1]],
            [SIZE[0] - 1.0, ROW_Y[1]],
        ] {
            stack.click(point, SIZE);
        }

        assert_eq!(chosen(&mut stack), []);
        assert!(stack.is_open());
    }

    #[test]
    fn a_click_on_a_row_chooses_that_rows_action() {
        let mut stack = stack_of(main_screen());
        press(&mut stack, Key::ArrowDown);

        stack.click([640.0, ROW_Y[2]], SIZE);
        stack.click([640.0, ROW_Y[0]], SIZE);
        stack.click([640.0 + 119.5, ROW_Y[1]], SIZE);
        stack.click([640.0 - 119.5, ROW_Y[1]], SIZE);

        assert_eq!(
            chosen(&mut stack),
            [Act::Quit, Act::Play, Act::Settings, Act::Settings]
        );
    }

    #[test]
    fn hovering_a_row_focuses_it_and_a_miss_keeps_the_focus() {
        let mut stack = stack_of(main_screen());

        stack.hover([640.0, ROW_Y[2]], SIZE);
        press(&mut stack, Key::Enter);
        let over_quit = chosen(&mut stack);
        stack.hover([0.0, 0.0], SIZE);
        press(&mut stack, Key::Enter);
        let after_a_miss = chosen(&mut stack);
        stack.hover([640.0, ROW_Y[1]], SIZE);
        press(&mut stack, Key::Enter);
        let over_settings = chosen(&mut stack);

        assert_eq!(over_quit, [Act::Quit]);
        assert_eq!(after_a_miss, [Act::Quit]);
        assert_eq!(over_settings, [Act::Settings]);
    }

    #[test]
    fn a_click_follows_the_centre_of_a_window_of_another_size() {
        let mut stack = stack_of(main_screen());
        let size = [1000.0, 400.0];

        stack.click([500.0, 200.0], size);
        stack.click([640.0, 360.0], size);

        assert_eq!(chosen(&mut stack), [Act::Settings]);
    }

    #[test]
    fn layout_gives_240_wide_rows_one_56_point_pitch_apart_centred_on_the_window() {
        let rows = layout(&main_screen(), SIZE);

        let centres: Vec<[f32; 2]> = rows
            .iter()
            .map(|r| [(r.min[0] + r.max[0]) / 2.0, (r.min[1] + r.max[1]) / 2.0])
            .collect();
        assert_eq!(centres, [[640.0, 304.0], [640.0, 360.0], [640.0, 416.0]]);
        for row in &rows {
            assert_eq!(row.max[0] - row.min[0], 240.0);
            let height = row.max[1] - row.min[1];
            assert!((36.0..=56.0).contains(&height), "row is {height} tall");
        }
    }

    #[test]
    fn layout_centres_an_even_column_and_a_single_row() {
        let two = MenuScreen {
            title: None,
            items: vec![button("A", Act::Play), button("B", Act::Quit)],
            back: None,
            settings: None,
        };

        let pair: Vec<f32> = layout(&two, SIZE)
            .iter()
            .map(|r| (r.min[1] + r.max[1]) / 2.0)
            .collect();
        let single: Vec<f32> = layout(&settings_screen(), [1000.0, 400.0])
            .iter()
            .map(|r| (r.min[1] + r.max[1]) / 2.0)
            .collect();

        assert_eq!(pair, [332.0, 388.0]);
        assert_eq!(single, [200.0]);
    }

    const FOCUS: [u8; 4] = [255, 220, 80, 255];
    const MAIN_LABELS: [&str; 3] = ["Play", "Settings", "Quit"];

    /// Where a centre-anchored text is centred on a window `size` points across.
    fn text_centre(text: &ScreenText, size: [f32; 2]) -> [f32; 2] {
        assert_eq!(text.anchor, Anchor::Center);
        [
            size[0] / 2.0 + text.offset[0],
            size[1] / 2.0 + text.offset[1],
        ]
    }

    fn rect_centre(rect: &Rect) -> [f32; 2] {
        [
            (rect.min[0] + rect.max[0]) / 2.0,
            (rect.min[1] + rect.max[1]) / 2.0,
        ]
    }

    fn focused_labels(texts: &[ScreenText]) -> Vec<&str> {
        texts
            .iter()
            .filter(|t| t.color == FOCUS)
            .map(|t| t.text.as_str())
            .collect()
    }

    #[test]
    fn each_row_text_is_centred_on_its_layout_rect_at_any_window_size() {
        for size in [SIZE, [1000.0, 400.0]] {
            let stack = stack_of(main_screen());

            let texts = stack.texts(size);

            let rects = layout(&main_screen(), size);
            assert_eq!(texts.len(), 3);
            for ((text, rect), label) in texts.iter().zip(&rects).zip(MAIN_LABELS) {
                assert_eq!(text.text, label);
                assert_eq!(text_centre(text, size), rect_centre(rect));
            }
        }
    }

    #[test]
    fn the_focus_colour_is_on_exactly_the_focused_row_and_moves_with_down() {
        let mut stack = stack_of(main_screen());

        let first = stack.texts(SIZE);
        press(&mut stack, Key::ArrowDown);
        let second = stack.texts(SIZE);
        press(&mut stack, Key::ArrowDown);
        let third = stack.texts(SIZE);

        assert_eq!(focused_labels(&first), ["Play"]);
        assert_eq!(focused_labels(&second), ["Settings"]);
        assert_eq!(focused_labels(&third), ["Quit"]);
        assert!(
            second
                .iter()
                .filter(|t| t.color != FOCUS)
                .all(|t| t.color == COLOR)
        );
        assert_eq!(second.iter().filter(|t| t.color != FOCUS).count(), 2);
    }

    #[test]
    fn a_title_is_painted_first_one_pitch_above_the_first_row() {
        let screen = MenuScreen {
            title: Some("Pick".to_owned()),
            items: vec![button("A", Act::Play), button("B", Act::Quit)],
            back: None,
            settings: None,
        };
        let stack = stack_of(screen.clone());

        let texts = stack.texts(SIZE);

        let first_row = rect_centre(&layout(&screen, SIZE)[0]);
        assert_eq!(texts.len(), 3);
        assert_eq!(texts[0].text, "Pick");
        assert_eq!(texts[0].color, COLOR);
        assert_eq!(texts[1].text, "A");
        assert_eq!(
            text_centre(&texts[0], SIZE),
            [first_row[0], first_row[1] - 56.0]
        );
    }

    #[test]
    fn a_screen_without_a_title_paints_only_its_rows() {
        let stack = stack_of(main_screen());

        let texts = stack.texts(SIZE);

        assert_eq!(texts.len(), 3);
        assert_eq!(texts[0].text, "Play");
    }

    #[test]
    fn an_empty_stack_paints_no_text() {
        let stack: MenuStack<Act> = MenuStack::new();

        assert_eq!(stack.texts(SIZE), []);
    }

    #[test]
    fn layout_of_a_screen_with_no_items_has_no_rows() {
        let empty: MenuScreen<Act> = MenuScreen {
            title: None,
            items: Vec::new(),
            back: None,
            settings: None,
        };

        assert_eq!(layout(&empty, SIZE), []);
    }

    mod settings_rows {
        use moho_core::prefs::WindowMode;

        use super::*;

        fn base() -> EngineSettings {
            EngineSettings {
                mouse_sensitivity: 2.5,
                mouse_smoothing: false,
                invert_mouse_y: false,
                sound_effect_volume: 8.0,
                music_volume: 6.0,
                ui_volume: 7.0,
                shadow_quality: 3,
                ssao_quality: 3,
                window_mode: WindowMode::Windowed,
                resolution: (1920, 1080),
            }
        }

        /// The ten rows, Apply, then Back: twelve rows, so the whole column fits on `SIZE`.
        fn screen_of(settings: EngineSettings) -> MenuScreen<Act> {
            let mut items: Vec<MenuItem<Act>> = EngineSetting::ALL
                .into_iter()
                .map(MenuItem::Setting)
                .collect();
            items.push(MenuItem::Apply);
            items.push(button("Back", Act::Back));
            MenuScreen {
                title: Some("Settings".to_owned()),
                items,
                back: Some(Act::Back),
                settings: Some(settings),
            }
        }

        fn row_of(stack: &MenuStack<Act>, item: MenuItem<Act>) -> usize {
            let screen = stack.top().expect("a screen is open");
            screen
                .items
                .iter()
                .position(|candidate| *candidate == item)
                .expect("the screen lists the item")
        }

        fn row_centre(stack: &MenuStack<Act>, row: usize) -> [f32; 2] {
            let rect = layout(stack.top().expect("a screen is open"), SIZE)[row];
            [
                (rect.min[0] + rect.max[0]) / 2.0,
                (rect.min[1] + rect.max[1]) / 2.0,
            ]
        }

        /// Moves the focus to `setting`'s row the way a mouse would.
        fn focus(stack: &mut MenuStack<Act>, setting: EngineSetting) -> usize {
            let row = row_of(stack, MenuItem::Setting(setting));
            let centre = row_centre(stack, row);
            stack.hover(centre, SIZE);
            row
        }

        fn step(stack: &mut MenuStack<Act>, setting: EngineSetting, keys: &[Key]) {
            focus(stack, setting);
            for &key in keys {
                press(stack, key);
            }
        }

        fn click_row(stack: &mut MenuStack<Act>, setting: EngineSetting) {
            let row = row_of(stack, MenuItem::Setting(setting));
            let centre = row_centre(stack, row);
            stack.click(centre, SIZE);
        }

        /// Presses Apply and returns the one event it emits.
        fn apply(stack: &mut MenuStack<Act>) -> EngineSettings {
            let row = row_of(stack, MenuItem::Apply);
            let centre = row_centre(stack, row);
            stack.hover(centre, SIZE);
            press(stack, Key::Enter);
            let mut events = stack.take_events();
            assert_eq!(events.len(), 1, "Apply emits exactly one event: {events:?}");
            match events.remove(0) {
                MenuEvent::SettingsApplied(applied) => applied,
                other @ MenuEvent::Chosen(_) => panic!("Apply emitted {other:?}"),
            }
        }

        fn close(a: f32, b: f32) -> bool {
            (a - b).abs() < 1e-4
        }

        #[test]
        fn a_staged_change_emits_nothing_until_apply() {
            let mut stack = stack_of(screen_of(base()));

            step(
                &mut stack,
                EngineSetting::MusicVolume,
                &[Key::ArrowRight, Key::ArrowRight],
            );
            let before_apply = stack.take_events();
            let baseline = stack.top().and_then(|screen| screen.settings);
            let applied = apply(&mut stack);

            assert_eq!(before_apply, []);
            assert_eq!(baseline, Some(base()), "staging edits a copy");
            assert_eq!(
                applied,
                EngineSettings {
                    music_volume: 7.0,
                    ..base()
                }
            );
            assert_eq!(
                stack.top().and_then(|screen| screen.settings),
                Some(applied),
                "Apply makes the staged values the screen's baseline"
            );
        }

        #[test]
        fn popping_a_settings_screen_discards_its_staged_values() {
            let mut stack = stack_of(main_screen());
            stack.push(screen_of(base()));
            step(&mut stack, EngineSetting::WindowMode, &[Key::ArrowRight]);
            step(&mut stack, EngineSetting::InvertMouseY, &[Key::Enter]);
            step(&mut stack, EngineSetting::ShadowQuality, &[Key::ArrowLeft]);

            stack.pop();
            stack.push(screen_of(base()));
            let applied = apply(&mut stack);

            assert_eq!(applied, base());
            assert_eq!(stack.take_events(), []);
        }

        #[test]
        fn pressing_apply_on_an_unchanged_screen_still_emits_its_values() {
            let mut stack = stack_of(screen_of(base()));

            let applied = apply(&mut stack);

            assert_eq!(applied, base());
        }

        #[test]
        fn left_and_right_step_a_slider_and_clamp_at_its_ends() {
            let ends = EngineSettings {
                sound_effect_volume: 9.5,
                ui_volume: 0.5,
                ..base()
            };
            let mut stack = stack_of(screen_of(ends));
            step(
                &mut stack,
                EngineSetting::SoundEffectVolume,
                &[Key::ArrowRight, Key::ArrowRight],
            );
            step(
                &mut stack,
                EngineSetting::UiVolume,
                &[Key::ArrowLeft, Key::ArrowLeft],
            );
            step(
                &mut stack,
                EngineSetting::MusicVolume,
                &[Key::ArrowRight, Key::ArrowRight, Key::ArrowLeft],
            );
            step(
                &mut stack,
                EngineSetting::MouseSensitivity,
                &[Key::ArrowRight],
            );

            let applied = apply(&mut stack);

            assert_eq!(applied.sound_effect_volume, 10.0);
            assert_eq!(applied.ui_volume, 0.0);
            assert_eq!(applied.music_volume, 6.5);
            assert!(close(applied.mouse_sensitivity, 2.6), "{applied:?}");
        }

        #[test]
        fn a_slider_at_its_range_ends_stays_there() {
            let low = EngineSettings {
                mouse_sensitivity: 0.1,
                ..base()
            };
            let high = EngineSettings {
                mouse_sensitivity: 10.0,
                ..base()
            };
            let mut at_low = stack_of(screen_of(low));
            let mut at_high = stack_of(screen_of(high));

            step(
                &mut at_low,
                EngineSetting::MouseSensitivity,
                &[Key::ArrowLeft],
            );
            step(
                &mut at_high,
                EngineSetting::MouseSensitivity,
                &[Key::ArrowRight],
            );

            assert!(close(apply(&mut at_low).mouse_sensitivity, 0.1));
            assert!(close(apply(&mut at_high).mouse_sensitivity, 10.0));
        }

        #[test]
        fn left_and_right_step_a_choice_and_clamp_at_its_ends() {
            let ends = EngineSettings {
                shadow_quality: 3,
                ssao_quality: 0,
                ..base()
            };
            let mut stack = stack_of(screen_of(ends));
            step(
                &mut stack,
                EngineSetting::ShadowQuality,
                &[Key::ArrowRight, Key::ArrowRight],
            );
            step(
                &mut stack,
                EngineSetting::SsaoQuality,
                &[Key::ArrowLeft, Key::ArrowRight, Key::ArrowRight],
            );
            step(&mut stack, EngineSetting::Resolution, &[Key::ArrowRight]);

            let applied = apply(&mut stack);

            assert_eq!(applied.shadow_quality, 4);
            assert_eq!(applied.ssao_quality, 2);
            assert_eq!(applied.resolution, (2560, 1440));
        }

        #[test]
        fn left_and_right_flip_a_toggle() {
            let mut stack = stack_of(screen_of(base()));
            step(
                &mut stack,
                EngineSetting::MouseSmoothing,
                &[Key::ArrowRight],
            );
            step(
                &mut stack,
                EngineSetting::InvertMouseY,
                &[Key::ArrowLeft, Key::ArrowLeft, Key::ArrowLeft],
            );

            let applied = apply(&mut stack);

            assert!(applied.mouse_smoothing);
            assert!(applied.invert_mouse_y);
        }

        #[test]
        fn enter_flips_a_toggle_and_cycles_a_choice() {
            let ends = EngineSettings {
                shadow_quality: 4,
                ssao_quality: 1,
                resolution: (3840, 2160),
                ..base()
            };
            let mut stack = stack_of(screen_of(ends));
            step(&mut stack, EngineSetting::MouseSmoothing, &[Key::Enter]);
            step(
                &mut stack,
                EngineSetting::InvertMouseY,
                &[Key::Enter, Key::Enter],
            );
            step(&mut stack, EngineSetting::ShadowQuality, &[Key::Enter]);
            step(&mut stack, EngineSetting::SsaoQuality, &[Key::Enter]);
            step(&mut stack, EngineSetting::WindowMode, &[Key::Enter]);
            step(&mut stack, EngineSetting::Resolution, &[Key::Enter]);
            let after_one_press = stack.take_events();

            let first = apply(&mut stack);
            step(&mut stack, EngineSetting::WindowMode, &[Key::Enter]);
            step(&mut stack, EngineSetting::Resolution, &[Key::Enter]);
            let second = apply(&mut stack);

            assert_eq!(after_one_press, [], "Enter on a row chooses nothing");
            assert!(first.mouse_smoothing);
            assert!(!first.invert_mouse_y);
            assert_eq!(first.shadow_quality, 0, "Ultra wraps to Off");
            assert_eq!(first.ssao_quality, 2);
            assert_eq!(first.window_mode, WindowMode::Borderless);
            assert_eq!(first.resolution, (1280, 720), "the last preset wraps");
            assert_eq!(second.window_mode, WindowMode::Windowed);
            assert_eq!(second.resolution, (1920, 1080));
        }

        #[test]
        fn a_saved_fullscreen_cycles_back_to_windowed() {
            let saved = EngineSettings {
                window_mode: WindowMode::Fullscreen,
                ..base()
            };
            let mut stack = stack_of(screen_of(saved));

            step(&mut stack, EngineSetting::WindowMode, &[Key::Enter]);

            assert_eq!(apply(&mut stack).window_mode, WindowMode::Windowed);
        }

        #[test]
        fn moving_the_focus_off_a_row_does_not_change_its_value() {
            let mut stack = stack_of(screen_of(base()));

            press(&mut stack, Key::ArrowDown);
            press(&mut stack, Key::ArrowUp);
            press(&mut stack, Key::ArrowDown);
            press(&mut stack, Key::ArrowDown);

            assert_eq!(apply(&mut stack), base());
        }

        #[test]
        fn a_click_flips_a_toggle_and_advances_a_choice() {
            let mut stack = stack_of(screen_of(base()));

            click_row(&mut stack, EngineSetting::MouseSmoothing);
            click_row(&mut stack, EngineSetting::ShadowQuality);
            click_row(&mut stack, EngineSetting::WindowMode);
            let clicked = stack.take_events();
            let applied = apply(&mut stack);

            assert_eq!(clicked, []);
            assert!(applied.mouse_smoothing);
            assert_eq!(applied.shadow_quality, 4);
            assert_eq!(applied.window_mode, WindowMode::Borderless);
        }

        /// The x a press must land on to sit `fraction` of the way along a row's track, taking
        /// the track as the row's whole 240-point band.
        fn track_x(stack: &MenuStack<Act>, setting: EngineSetting, fraction: f32) -> f32 {
            let row = row_of(stack, MenuItem::Setting(setting));
            let rect = layout(stack.top().expect("a screen is open"), SIZE)[row];
            rect.min[0] + fraction * (rect.max[0] - rect.min[0])
        }

        fn press_track(stack: &mut MenuStack<Act>, setting: EngineSetting, fraction: f32) {
            let row = row_of(stack, MenuItem::Setting(setting));
            let y = row_centre(stack, row)[1];
            let x = track_x(stack, setting, fraction);
            stack.click([x, y], SIZE);
        }

        #[test]
        fn a_press_on_a_slider_track_sets_the_value_from_x_snapped_to_the_step() {
            let mut stack = stack_of(screen_of(base()));

            press_track(&mut stack, EngineSetting::SoundEffectVolume, 0.3);
            press_track(&mut stack, EngineSetting::MusicVolume, 0.33);
            press_track(&mut stack, EngineSetting::UiVolume, 0.99);
            let events = stack.take_events();
            let applied = apply(&mut stack);

            assert_eq!(events, [], "a press on a slider chooses nothing");
            assert_eq!(applied.sound_effect_volume, 3.0);
            assert_eq!(applied.music_volume, 3.5, "3.3 snaps to the nearest 0.5");
            assert_eq!(applied.ui_volume, 10.0);
        }

        #[test]
        fn a_settings_screen_draws_each_rows_label_and_value_and_the_apply_row() {
            let stack = stack_of(screen_of(base()));

            let texts: Vec<String> = stack.texts(SIZE).into_iter().map(|t| t.text).collect();

            for setting in EngineSetting::ALL {
                assert!(
                    texts.iter().any(|text| text == setting.label()),
                    "no label for {setting:?} in {texts:?}"
                );
                let value = base().value_text(setting);
                assert!(
                    texts.contains(&value),
                    "no value {value:?} for {setting:?} in {texts:?}"
                );
            }
            assert!(texts.iter().any(|text| text == "Apply"), "{texts:?}");
            assert!(texts.iter().any(|text| text == "Back"), "{texts:?}");
        }
    }
}
