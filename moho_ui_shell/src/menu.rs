//! Menu screens a game describes as data: labelled buttons the engine draws and drives with the
//! mouse and keyboard. Screens stack, each keeping its own focus. The stack never changes itself:
//! a choice becomes a [`MenuEvent`] the game answers by pushing or popping.

use moho_input::bindings::Input;
use moho_input::key::Key;

use crate::text::{Anchor, ScreenText};

/// One screen: an optional title, a column of items and the action Escape chooses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuScreen<A> {
    pub title: Option<String>,
    pub items: Vec<MenuItem<A>>,
    /// What Escape chooses on this screen; `None` makes Escape do nothing.
    pub back: Option<A>,
}

/// One row of a screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem<A> {
    /// A labelled button that chooses `action`.
    Button { label: String, action: A },
}

/// What the player did to the open screens, for the game to answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEvent<A> {
    /// The player chose an item, or pressed Escape on a screen with a back action.
    Chosen(A),
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

/// A screen and the row that has the focus on it.
#[derive(Debug)]
struct Open<A> {
    screen: MenuScreen<A>,
    focus: usize,
}

/// The open screens, bottom to top, and the events not yet taken.
#[derive(Debug)]
pub struct MenuStack<A> {
    screens: Vec<Open<A>>,
    events: Vec<MenuEvent<A>>,
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
        }
    }

    /// Opens `screen` over the current one, focusing its first item.
    pub fn push(&mut self, screen: MenuScreen<A>) {
        self.screens.push(Open { screen, focus: 0 });
    }

    /// Closes the top screen; the one below keeps the focus it had.
    pub fn pop(&mut self) {
        self.screens.pop();
    }

    /// Closes every screen.
    pub fn clear(&mut self) {
        self.screens.clear();
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
        let text = |text: &str, centre_y: f32, color: [u8; 4]| ScreenText {
            text: text.to_owned(),
            anchor: Anchor::Center,
            offset: [0.0, centre_y - size[1] / 2.0],
            size: ROW_SIZE,
            color,
        };
        let mut texts = Vec::new();
        if let Some(title) = &open.screen.title {
            let first_centre = rows.first().map_or(size[1] / 2.0, |r| centre_y(*r));
            texts.push(text(title, first_centre - ROW_PITCH, COLOR));
        }
        for (row, (item, rect)) in open.screen.items.iter().zip(rows).enumerate() {
            let MenuItem::Button { label, .. } = item;
            let color = if row == open.focus {
                FOCUS_COLOR
            } else {
                COLOR
            };
            texts.push(text(label, centre_y(rect), color));
        }
        texts
    }
}

impl<A: Clone> MenuStack<A> {
    /// Up and Down move the focus and wrap; a non-repeat Enter press chooses the focused item;
    /// a non-repeat Escape press chooses the top screen's back action, if any. Releases and
    /// other inputs do nothing.
    pub fn on_key(&mut self, input: Input, pressed: bool, repeat: bool) {
        let Some(open) = self.screens.last_mut() else {
            return;
        };
        if !pressed {
            return;
        }
        let count = open.screen.items.len();
        match input {
            Input::Key(Key::ArrowDown) if count > 0 => open.focus = (open.focus + 1) % count,
            Input::Key(Key::ArrowUp) if count > 0 => {
                open.focus = (open.focus + count - 1) % count;
            }
            Input::Key(Key::Enter) if !repeat => {
                if let Some(action) = open.screen.items.get(open.focus).map(action_of) {
                    self.events.push(MenuEvent::Chosen(action));
                }
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
    /// nothing.
    pub fn click(&mut self, point: [f32; 2], size: [f32; 2]) {
        if let Some(open) = self.screens.last()
            && let Some(row) = row_at(&open.screen, point, size)
        {
            let action = action_of(&open.screen.items[row]);
            self.events.push(MenuEvent::Chosen(action));
        }
    }
}

fn action_of<A: Clone>(item: &MenuItem<A>) -> A {
    match item {
        MenuItem::Button { action, .. } => action.clone(),
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
        }
    }

    fn settings_screen() -> MenuScreen<Act> {
        MenuScreen {
            title: Some("Settings".to_owned()),
            items: vec![button("Back", Act::Back)],
            back: Some(Act::Back),
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
            .map(|event| match event {
                MenuEvent::Chosen(action) => action,
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

    #[test]
    fn layout_of_a_screen_with_no_items_has_no_rows() {
        let empty: MenuScreen<Act> = MenuScreen {
            title: None,
            items: Vec::new(),
            back: None,
        };

        assert_eq!(layout(&empty, SIZE), []);
    }
}
