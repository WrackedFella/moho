//! Menu screens a game describes as data: labelled buttons the engine draws and drives with the
//! mouse and keyboard. Screens stack, each keeping its own focus. The stack never changes itself:
//! a choice becomes a [`MenuEvent`] the game answers by pushing or popping.

use std::cell::Cell;

use moho_input::bindings::{Binding, Input};
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
    /// A rebindable action: its label, two binding slots and the game's refusal note, if any.
    Control {
        label: String,
        action: A,
        slots: [Option<Binding>; 2],
        note: Option<String>,
    },
}

/// A column of a [`MenuItem::Control`] row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Primary,
    Secondary,
}

/// What the player did to the open screens, for the game to answer.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEvent<A> {
    /// The player chose an item, or pressed Escape on a screen with a back action.
    Chosen(A),
    /// The player applied the settings staged on the top screen.
    SettingsApplied(EngineSettings),
    /// The player pressed `input` while capturing `slot` of `action`.
    Captured { action: A, slot: Slot, input: Input },
    /// The player cleared `slot` of `action`.
    Cleared { action: A, slot: Slot },
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

/// Height of a [`MenuItem::Control`] row.
const CONTROL_HEIGHT: f32 = 32.0;
/// Width of a [`MenuItem::Control`] row: a label column, then two equal slot cells.
const CONTROL_WIDTH: f32 = 640.0;
const LABEL_WIDTH: f32 = 240.0;
const CELL_WIDTH: f32 = (CONTROL_WIDTH - LABEL_WIDTH) / 2.0;
/// Free space kept above and below the rows of a column that scrolls; the title sits in the top.
const BAND: f32 = ROW_PITCH;
const WARNING_COLOR: [u8; 4] = [255, 120, 80, 255];
const PROTECTED_NOTE: &str = "That input can't be bound";
/// Window size assumed until a screen has been laid out once.
const DEFAULT_SIZE: [f32; 2] = [1280.0, 720.0];

/// A screen, the row that has the focus on it and the settings values staged on it.
#[derive(Debug)]
struct Open<A> {
    screen: MenuScreen<A>,
    focus: usize,
    /// A copy of the screen's `settings` that the rows edit until Apply.
    staged: Option<EngineSettings>,
    /// The slot of a `Control` row the focus is on.
    slot: Slot,
    /// Whether the next press is a binding for the focused slot.
    capturing: bool,
    /// The shell's refusal of a captured input, shown on the focused row.
    shell_note: Option<String>,
    /// Whether moving the focus or starting a capture has hidden the game's notes.
    notes_hidden: bool,
    /// The first visible row. Cells so that laying out, which sees the window size, can settle it.
    scroll: Cell<usize>,
    /// Set by a focus change; the next layout scrolls the focused row into view.
    reveal: Cell<bool>,
    /// The window size of the last layout, for the wheel, which knows none.
    size: Cell<[f32; 2]>,
    /// The fraction of a row the wheel has scrolled and not yet shown.
    remainder: f32,
}

impl<A> Open<A> {
    fn new(screen: MenuScreen<A>) -> Self {
        let staged = screen.settings;
        Self {
            screen,
            focus: 0,
            staged,
            slot: Slot::Primary,
            capturing: false,
            shell_note: None,
            notes_hidden: false,
            scroll: Cell::new(0),
            reveal: Cell::new(false),
            size: Cell::new(DEFAULT_SIZE),
            remainder: 0.0,
        }
    }

    /// Moves the focus, hiding the notes when it moved.
    fn set_focus(&mut self, row: usize, slot: Slot) {
        if (row, slot) != (self.focus, self.slot) {
            self.focus = row;
            self.slot = slot;
            self.shell_note = None;
            self.notes_hidden = true;
            self.reveal.set(true);
        }
    }

    fn focused_is_control(&self) -> bool {
        matches!(
            self.screen.items.get(self.focus),
            Some(MenuItem::Control { .. })
        )
    }

    /// Starts capturing the focused slot when the focus is on a `Control` row.
    fn start_capture(&mut self) {
        if self.focused_is_control() {
            self.capturing = true;
            self.shell_note = None;
            self.notes_hidden = true;
        }
    }

    /// Settles the scroll for a window `size` points across: clamps it so no row is wasted past
    /// the end, then brings the focused row into view if the focus moved.
    fn settle(&self, size: [f32; 2]) {
        self.size.set(size);
        let items = &self.screen.items;
        let viewport = viewport(size);
        let mut scroll = self.scroll.get().min(max_scroll(items, viewport));
        if self.reveal.take() {
            if self.focus < scroll {
                scroll = self.focus;
            } else {
                while scroll < self.focus
                    && scroll + fitting_from(items, scroll, viewport) <= self.focus
                {
                    scroll += 1;
                }
            }
        }
        self.scroll.set(scroll);
    }

    /// The row rectangles at the settled scroll, rows above the view included.
    fn rects(&self, size: [f32; 2]) -> Vec<Rect> {
        self.settle(size);
        rects(&self.screen.items, self.scroll.get(), size)
    }

    /// The rows fully in view, as a range of row indices.
    fn visible(&self, size: [f32; 2]) -> std::ops::Range<usize> {
        let scroll = self.scroll.get();
        scroll..scroll + fitting_from(&self.screen.items, scroll, viewport(size))
    }

    /// The visible row strictly under `point`, with its rectangle.
    fn row_at(&self, point: [f32; 2], size: [f32; 2]) -> Option<(usize, Rect)> {
        let rects = self.rects(size);
        self.visible(size)
            .map(|row| (row, rects[row]))
            .find(|(_, r)| {
                point[0] > r.min[0]
                    && point[0] < r.max[0]
                    && point[1] > r.min[1]
                    && point[1] < r.max[1]
            })
    }

    /// The slot of a `Control` row `rect` whose cell holds `x`; `None` over its label or on
    /// another kind of row.
    fn slot_at(&self, row: usize, rect: Rect, x: f32) -> Option<Slot> {
        if !matches!(self.screen.items.get(row), Some(MenuItem::Control { .. })) {
            return None;
        }
        let across = x - rect.min[0];
        if across < LABEL_WIDTH {
            None
        } else if across < LABEL_WIDTH + CELL_WIDTH {
            Some(Slot::Primary)
        } else {
            Some(Slot::Secondary)
        }
    }

    /// Left and Right: pick a `Control` row's slot, or step a settings row.
    fn step(&mut self, direction: i8) {
        if self.focused_is_control() {
            let slot = if direction < 0 {
                Slot::Primary
            } else {
                Slot::Secondary
            };
            self.set_focus(self.focus, slot);
        } else {
            self.nudge(self.focus, direction);
        }
    }
}

impl<A: Clone> Open<A> {
    /// What a press does while capturing: Escape cancels, a protected input is refused with a
    /// note and capture goes on, anything else is the binding and ends capture.
    fn capture(&mut self, input: Input) -> Option<MenuEvent<A>> {
        if input == Input::Key(Key::Escape) {
            self.capturing = false;
            return None;
        }
        if Binding::from(input).is_protected() {
            self.shell_note = Some(PROTECTED_NOTE.to_owned());
            return None;
        }
        self.capturing = false;
        match self.screen.items.get(self.focus)? {
            MenuItem::Control { action, .. } => Some(MenuEvent::Captured {
                action: action.clone(),
                slot: self.slot,
                input,
            }),
            _ => None,
        }
    }

    /// Clears the focused slot, if the focus is on a `Control` row.
    fn clear(&self) -> Option<MenuEvent<A>> {
        match self.screen.items.get(self.focus)? {
            MenuItem::Control { action, .. } => Some(MenuEvent::Cleared {
                action: action.clone(),
                slot: self.slot,
            }),
            _ => None,
        }
    }
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
            MenuItem::Control { .. } => None,
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
                    let last = staged.choice_count(*setting).saturating_sub(1);
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
        let Some(rect) = self.rects(size).get(row).copied() else {
            return;
        };
        if let (Some(staged), Some(MenuItem::Setting(setting))) =
            (&mut self.staged, self.screen.items.get(row))
            && let Control::Slider { min, max, .. } = setting.control()
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
            let count = staged.choice_count(setting);
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
        self.screens.push(Open::new(screen));
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

    /// Replaces the top screen's items, keeping its focus, slot, capture and scroll.
    pub fn set_items(&mut self, items: Vec<MenuItem<A>>) {
        if let Some(open) = self.screens.last_mut() {
            open.focus = open.focus.min(items.len().saturating_sub(1));
            open.screen.items = items;
            open.shell_note = None;
            open.notes_hidden = false;
        }
    }

    /// Ends a capture in progress without an event.
    pub fn cancel_capture(&mut self) {
        if let Some(open) = self.screens.last_mut() {
            open.capturing = false;
            open.shell_note = None;
        }
    }

    /// Scrolls the top screen by `rows` rows, keeping a fractional remainder.
    pub fn scroll(&mut self, rows: f32) {
        if let Some(open) = self.screens.last_mut() {
            let size = open.size.get();
            open.settle(size);
            let last = max_scroll(&open.screen.items, viewport(size)) as f32;
            let target = (open.scroll.get() as f32 + open.remainder + rows).clamp(0.0, last);
            let whole = target.floor();
            open.scroll.set(whole as usize);
            open.remainder = target - whole;
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Moves the dragged slider to `point`, in logical points on a window `size` points across.
    pub fn drag(&mut self, point: [f32; 2], size: [f32; 2]) {
        if let (Some(open), Some(row)) = (self.screens.last_mut(), self.dragging) {
            open.set_slider_from(row, point[0], size);
        }
    }

    /// Whether the top screen is waiting for a key or button to bind.
    pub(crate) fn is_capturing(&self) -> bool {
        self.screens.last().is_some_and(|open| open.capturing)
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
            && !open.capturing
            && let Some((row, rect)) = open.row_at(point, size)
        {
            let slot = open.slot_at(row, rect, point[0]).unwrap_or(open.slot);
            open.set_focus(row, slot);
        }
    }

    /// The text of the top screen on a window `size` points across: its title above the rows,
    /// then one centre-anchored text per row with the focused one in its own colour.
    pub(crate) fn texts(&self, size: [f32; 2]) -> Vec<ScreenText> {
        let Some(open) = self.screens.last() else {
            return Vec::new();
        };
        let rows = open.rects(size);
        let visible = open.visible(size);
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
            let first_centre = rows
                .get(visible.start)
                .map_or(size[1] / 2.0, |r| centre_y(*r));
            let mut title_centre = first_centre - ROW_PITCH;
            if matches!(
                open.screen.items.get(visible.start),
                Some(MenuItem::Control { .. })
            ) {
                // Control rows are short, so the title band above them is too thin to hold it.
                title_centre = title_centre.max(ROW_SIZE / 2.0);
            }
            texts.push(centred(title, title_centre, COLOR));
        }
        for row in visible {
            let (item, rect) = (&open.screen.items[row], rows[row]);
            let color = if row == open.focus {
                FOCUS_COLOR
            } else {
                COLOR
            };
            match item {
                MenuItem::Button { label, .. } => texts.push(centred(label, centre_y(rect), color)),
                MenuItem::Apply => texts.push(centred("Apply", centre_y(rect), color)),
                MenuItem::Control {
                    label, slots, note, ..
                } => {
                    let y = centre_y(rect);
                    let shown_note = if row == open.focus {
                        open.shell_note.as_deref()
                    } else {
                        None
                    }
                    .or_else(|| note.as_deref().filter(|_| !open.notes_hidden));
                    let (label_text, label_color) = match shown_note {
                        Some(note) => (note.to_owned(), WARNING_COLOR),
                        None if slots.iter().all(Option::is_none) => (format!("! {label}"), color),
                        None => (label.clone(), color),
                    };
                    texts.push(text(
                        &label_text,
                        Anchor::Left,
                        rect.min[0],
                        y,
                        SETTING_SIZE,
                        label_color,
                    ));
                    for (slot, cell_x, binding) in [
                        (Slot::Primary, LABEL_WIDTH, slots[0]),
                        (Slot::Secondary, LABEL_WIDTH + CELL_WIDTH, slots[1]),
                    ] {
                        let cell_color = if row == open.focus && slot == open.slot {
                            FOCUS_COLOR
                        } else {
                            COLOR
                        };
                        let label = binding.map_or_else(|| "Unbound".to_owned(), Binding::label);
                        texts.push(text(
                            &label,
                            Anchor::Left,
                            rect.min[0] + cell_x,
                            y,
                            SETTING_SIZE,
                            cell_color,
                        ));
                    }
                }
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
        if open.capturing {
            if !repeat {
                self.events.extend(open.capture(input));
            }
            return;
        }
        let count = open.screen.items.len();
        match input {
            Input::Key(Key::ArrowDown) if count > 0 => {
                open.set_focus((open.focus + 1) % count, open.slot);
            }
            Input::Key(Key::ArrowUp) if count > 0 => {
                open.set_focus((open.focus + count - 1) % count, open.slot);
            }
            Input::Key(Key::ArrowLeft) => open.step(-1),
            Input::Key(Key::ArrowRight) => open.step(1),
            Input::Key(Key::Enter) if !repeat => {
                if open.focused_is_control() {
                    open.start_capture();
                } else {
                    self.events.extend(open.choose(open.focus));
                }
            }
            Input::Key(Key::Backspace) | Input::Mouse(MouseButton::Right) if !repeat => {
                self.events.extend(open.clear());
            }
            Input::Key(Key::Escape) if !repeat => {
                if let Some(back) = open.screen.back.clone() {
                    self.events.push(MenuEvent::Chosen(back));
                }
            }
            _ => {}
        }
    }

    /// A right press at `point`: while capturing it is the binding; otherwise it clears the
    /// focused slot only when `point` is over that slot's cell.
    pub fn right_click(&mut self, point: [f32; 2], size: [f32; 2]) {
        let Some(open) = self.screens.last_mut() else {
            return;
        };
        if open.capturing {
            self.on_key(Input::Mouse(MouseButton::Right), true, false);
        } else if let Some((row, rect)) = open.row_at(point, size)
            && row == open.focus
            && open.slot_at(row, rect, point[0]) == Some(open.slot)
        {
            self.events.extend(open.clear());
        }
    }

    /// Chooses the row under `point` (see [`MenuStack::hover`]); a point over no row chooses
    /// nothing. A press on a slider row sets it from `point` and starts a drag that
    /// [`MenuStack::drag`] follows until the left button is released.
    pub fn click(&mut self, point: [f32; 2], size: [f32; 2]) {
        let Some(open) = self.screens.last_mut() else {
            return;
        };
        if open.capturing {
            self.dragging = None;
            self.events
                .extend(open.capture(Input::Mouse(MouseButton::Left)));
        } else if let Some((row, rect)) = open.row_at(point, size) {
            let on_slider = matches!(
                open.screen.items[row],
                MenuItem::Setting(setting) if matches!(setting.control(), Control::Slider { .. })
            );
            if let Some(slot) = open.slot_at(row, rect, point[0]) {
                open.set_focus(row, slot);
                open.start_capture();
            } else if on_slider {
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

/// Height available to rows: the window less a title band above and below.
fn viewport(size: [f32; 2]) -> f32 {
    size[1] - 2.0 * BAND
}

fn item_height<A>(item: &MenuItem<A>) -> f32 {
    match item {
        MenuItem::Control { .. } => CONTROL_HEIGHT,
        _ => ROW_PITCH,
    }
}

fn item_width<A>(item: &MenuItem<A>) -> f32 {
    match item {
        MenuItem::Control { .. } => CONTROL_WIDTH,
        _ => ROW_WIDTH,
    }
}

/// How many whole rows from `from` on fit in `viewport` points.
fn fitting_from<A>(items: &[MenuItem<A>], from: usize, viewport: f32) -> usize {
    let mut used = 0.0;
    items
        .iter()
        .skip(from)
        .take_while(|item| {
            used += item_height(item);
            used <= viewport
        })
        .count()
}

/// The last first-visible row that leaves no gap after the final row; 0 when everything fits.
fn max_scroll<A>(items: &[MenuItem<A>], viewport: f32) -> usize {
    let mut used = 0.0;
    let fitting = items
        .iter()
        .rev()
        .take_while(|item| {
            used += item_height(item);
            used <= viewport
        })
        .count();
    items.len().saturating_sub(fitting.max(1))
}

/// The rectangles of `items` with row `scroll` first in view; rows above it lie above the title
/// band. A column that fits is centred vertically, any other starts at the band.
fn rects<A>(items: &[MenuItem<A>], scroll: usize, size: [f32; 2]) -> Vec<Rect> {
    let total: f32 = items.iter().map(item_height).sum();
    let top = if total <= viewport(size) {
        size[1] / 2.0 - total / 2.0
    } else {
        BAND - items.iter().take(scroll).map(item_height).sum::<f32>()
    };
    let mut y = top;
    items
        .iter()
        .map(|item| {
            let (height, width) = (item_height(item), item_width(item));
            let rect = Rect {
                min: [size[0] / 2.0 - width / 2.0, y],
                max: [size[0] / 2.0 + width / 2.0, y + height],
            };
            y += height;
            rect
        })
        .collect()
}

/// The row rectangles of `screen` on a window `size` points across at scroll 0, top to bottom:
/// buttons and settings 240 by 56 points, control rows 640 by 32, centred on the window, the
/// column centred vertically when it fits and starting at the title band when it does not.
pub fn layout<A>(screen: &MenuScreen<A>, size: [f32; 2]) -> Vec<Rect> {
    rects(&screen.items, 0, size)
}

#[cfg(test)]
mod tests {
    use moho_input::bindings::Modifiers;
    use moho_input::key::{Key, MouseButton};
    use proptest::prelude::{any, prop, prop_assert, proptest};

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
                MenuEvent::SettingsApplied(_)
                | MenuEvent::Captured { .. }
                | MenuEvent::Cleared { .. } => None,
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

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Bind {
        Row(usize),
        Reset,
        Apply,
        Back,
    }

    const R: Option<Binding> = Some(Binding::key(Key::R));
    const NOTE: &str = "Q is used by Lean left";
    /// Where a press lands in a 640-wide control row: over the primary cell and over the
    /// secondary cell, whatever the width of the label column between a quarter and a half of
    /// the row.
    const PRIMARY_X: f32 = 360.0;
    const SECONDARY_X: f32 = 560.0;
    /// Rows 32 points tall fill the window less a 56-point band above and below; 19 fit.
    const VIEW_BAND: f32 = 56.0;

    fn control(index: usize, slots: [Option<Binding>; 2]) -> MenuItem<Bind> {
        MenuItem::Control {
            label: format!("Action {index:02}"),
            action: Bind::Row(index),
            slots,
            note: None,
        }
    }

    fn noted(index: usize, note: &str) -> MenuItem<Bind> {
        MenuItem::Control {
            label: format!("Action {index:02}"),
            action: Bind::Row(index),
            slots: [R, None],
            note: Some(note.to_owned()),
        }
    }

    fn controls_of(count: usize) -> Vec<MenuItem<Bind>> {
        (0..count).map(|index| control(index, [R, None])).collect()
    }

    fn bind_button(label: &str, action: Bind) -> MenuItem<Bind> {
        MenuItem::Button {
            label: label.to_owned(),
            action,
        }
    }

    fn controls_screen(items: Vec<MenuItem<Bind>>) -> MenuScreen<Bind> {
        MenuScreen {
            title: None,
            items,
            back: Some(Bind::Back),
            settings: None,
        }
    }

    fn controls_stack(items: Vec<MenuItem<Bind>>) -> MenuStack<Bind> {
        let mut stack = MenuStack::new();
        stack.push(controls_screen(items));
        stack
    }

    fn key_c(stack: &mut MenuStack<Bind>, key: Key) {
        stack.on_key(Input::Key(key), true, false);
    }

    fn captured(action: Bind, slot: Slot, input: Input) -> MenuEvent<Bind> {
        MenuEvent::Captured {
            action,
            slot,
            input,
        }
    }

    fn shows(stack: &MenuStack<Bind>, size: [f32; 2], needle: &str) -> bool {
        stack.texts(size).iter().any(|t| t.text.contains(needle))
    }

    fn row_shown(stack: &MenuStack<Bind>, index: usize) -> bool {
        shows(stack, SIZE, &format!("Action {index:02}"))
    }

    fn shown_labels(stack: &MenuStack<Bind>) -> Vec<String> {
        stack
            .texts(SIZE)
            .into_iter()
            .map(|t| t.text)
            .filter(|text| text.contains("Action "))
            .collect()
    }

    /// The texts of the top screen's row `row`, found by sharing the row's centre line.
    fn row_texts(stack: &MenuStack<Bind>, row: usize) -> Vec<ScreenText> {
        let rect = layout(stack.top().expect("a screen is open"), SIZE)[row];
        let line = rect_centre(&rect)[1];
        stack
            .texts(SIZE)
            .into_iter()
            .filter(|t| (SIZE[1] / 2.0 + t.offset[1] - line).abs() < 0.01)
            .collect()
    }

    fn row_strings(stack: &MenuStack<Bind>, row: usize) -> Vec<String> {
        row_texts(stack, row).into_iter().map(|t| t.text).collect()
    }

    fn is_warning(text: &ScreenText) -> bool {
        text.color != COLOR && text.color != FOCUS
    }

    fn cell_point(stack: &MenuStack<Bind>, row: usize, x_offset: f32) -> [f32; 2] {
        let rect = layout(stack.top().expect("a screen is open"), SIZE)[row];
        [rect.min[0] + x_offset, rect_centre(&rect)[1]]
    }

    #[test]
    fn a_control_row_shows_each_slots_label_unbound_and_a_mark_when_both_are_empty() {
        let stack = controls_stack(vec![
            control(0, [R, Some(Binding::mouse(MouseButton::Right))]),
            control(1, [Some(Binding::key(Key::R).held()), None]),
            control(2, [None, None]),
        ]);

        let both = row_strings(&stack, 0);
        let hold = row_strings(&stack, 1);
        let neither = row_strings(&stack, 2);

        assert!(both.iter().any(|t| t == "R"), "{both:?}");
        assert!(both.iter().any(|t| t == "Mouse Right"), "{both:?}");
        assert!(both.iter().any(|t| t.contains("Action 00")), "{both:?}");
        assert!(hold.iter().any(|t| t == "Hold R"), "{hold:?}");
        assert_eq!(hold.iter().filter(|t| *t == "Unbound").count(), 1);
        assert_eq!(neither.iter().filter(|t| *t == "Unbound").count(), 2);
        assert!(
            neither.iter().any(|t| t.contains("Action 02")),
            "{neither:?}"
        );
        assert_eq!(neither.iter().filter(|t| t.contains('!')).count(), 1);
        assert!(!both.iter().any(|t| t.contains('!')), "{both:?}");
        assert!(!hold.iter().any(|t| t.contains('!')), "{hold:?}");
    }

    #[test]
    fn enter_or_a_click_on_a_slot_starts_capture_of_that_slot() {
        let rows = || vec![control(0, [R, R]), control(1, [R, R]), control(2, [R, R])];
        let mut idle = controls_stack(rows());
        let mut by_enter = controls_stack(rows());
        let mut by_enter_secondary = controls_stack(rows());
        let mut by_click_secondary = controls_stack(rows());
        let mut by_click_primary = controls_stack(rows());

        key_c(&mut idle, Key::Q);
        key_c(&mut by_enter, Key::Enter);
        key_c(&mut by_enter, Key::Q);
        key_c(&mut by_enter_secondary, Key::ArrowRight);
        key_c(&mut by_enter_secondary, Key::Enter);
        key_c(&mut by_enter_secondary, Key::Q);
        let secondary = cell_point(&by_click_secondary, 1, SECONDARY_X);
        by_click_secondary.click(secondary, SIZE);
        key_c(&mut by_click_secondary, Key::W);
        let primary = cell_point(&by_click_primary, 2, PRIMARY_X);
        by_click_primary.click(primary, SIZE);
        key_c(&mut by_click_primary, Key::W);

        assert_eq!(
            idle.take_events(),
            [],
            "a key is no capture before one starts"
        );
        assert_eq!(
            by_enter.take_events(),
            [captured(Bind::Row(0), Slot::Primary, Input::Key(Key::Q))]
        );
        assert_eq!(
            by_enter_secondary.take_events(),
            [captured(Bind::Row(0), Slot::Secondary, Input::Key(Key::Q))]
        );
        assert_eq!(
            by_click_secondary.take_events(),
            [captured(Bind::Row(1), Slot::Secondary, Input::Key(Key::W))]
        );
        assert_eq!(
            by_click_primary.take_events(),
            [captured(Bind::Row(2), Slot::Primary, Input::Key(Key::W))]
        );
    }

    #[test]
    fn a_captured_key_or_button_becomes_an_event_for_the_captured_slot_and_ends_capture() {
        let mut stack = controls_stack(vec![control(0, [R, R]), control(1, [R, R])]);

        key_c(&mut stack, Key::Enter);
        key_c(&mut stack, Key::Q);
        key_c(&mut stack, Key::W);
        let after_key = stack.take_events();
        key_c(&mut stack, Key::ArrowRight);
        key_c(&mut stack, Key::Enter);
        stack.on_key(Input::Mouse(MouseButton::Right), true, false);
        key_c(&mut stack, Key::Space);
        let after_button = stack.take_events();

        assert_eq!(
            after_key,
            [captured(Bind::Row(0), Slot::Primary, Input::Key(Key::Q))],
            "the next press is the capture and the one after is not"
        );
        assert_eq!(
            after_button,
            [captured(
                Bind::Row(0),
                Slot::Secondary,
                Input::Mouse(MouseButton::Right)
            )]
        );
    }

    #[test]
    fn a_left_click_while_capturing_is_captured_not_a_click() {
        let mut over_control = controls_stack(vec![control(0, [R, R]), control(1, [R, R])]);
        let mut over_button =
            controls_stack(vec![control(0, [R, R]), bind_button("Reset", Bind::Reset)]);

        key_c(&mut over_control, Key::Enter);
        let point = cell_point(&over_control, 1, SECONDARY_X);
        over_control.click(point, SIZE);
        key_c(&mut over_control, Key::Q);
        key_c(&mut over_button, Key::Enter);
        let point = cell_point(&over_button, 1, 120.0);
        over_button.click(point, SIZE);

        assert_eq!(
            over_control.take_events(),
            [captured(
                Bind::Row(0),
                Slot::Primary,
                Input::Mouse(MouseButton::Left)
            )],
            "the click ended capture and did not start one on row 1"
        );
        assert_eq!(
            over_button.take_events(),
            [captured(
                Bind::Row(0),
                Slot::Primary,
                Input::Mouse(MouseButton::Left)
            )],
            "the click did not choose the button under it"
        );
    }

    #[test]
    fn set_items_shows_the_answer_and_keeps_focus_slot_and_scroll() {
        let mut stack = controls_stack(controls_of(30));
        for _ in 0..22 {
            key_c(&mut stack, Key::ArrowDown);
        }
        key_c(&mut stack, Key::ArrowRight);
        let before = shown_labels(&stack);
        let mut answer = controls_of(30);
        answer[22] = control(22, [R, Some(Binding::key(Key::R).with(Modifiers::CTRL))]);

        stack.set_items(answer);
        let after = shown_labels(&stack);
        let shows_answer = shows(&stack, SIZE, "Ctrl+R");
        key_c(&mut stack, Key::Enter);
        key_c(&mut stack, Key::Q);

        assert!(before.iter().any(|t| t.contains("Action 22")), "{before:?}");
        assert_eq!(after, before, "the same rows stay in view");
        assert!(shows_answer, "the new label is drawn at once");
        assert_eq!(
            stack.take_events(),
            [captured(Bind::Row(22), Slot::Secondary, Input::Key(Key::Q))],
            "focus and slot survived the update"
        );
    }

    #[test]
    fn a_game_note_shows_in_place_of_the_label_until_the_next_answer() {
        let plain = || vec![control(0, [R, None]), control(1, [R, None])];
        let mut stack = controls_stack(plain());

        stack.set_items(vec![noted(0, NOTE), control(1, [R, None])]);
        let texts = row_texts(&stack, 0);
        let note_shown = texts.iter().find(|t| t.text == NOTE);
        let label_hidden = !shows(&stack, SIZE, "Action 00");
        stack.set_items(plain());
        let answered = (shows(&stack, SIZE, "Action 00"), shows(&stack, SIZE, NOTE));
        stack.set_items(vec![noted(0, NOTE), control(1, [R, None])]);
        key_c(&mut stack, Key::ArrowDown);
        let moved_off = (shows(&stack, SIZE, "Action 00"), shows(&stack, SIZE, NOTE));
        key_c(&mut stack, Key::ArrowUp);
        let moved_back = (shows(&stack, SIZE, "Action 00"), shows(&stack, SIZE, NOTE));

        assert!(
            note_shown.is_some_and(is_warning),
            "the note is in a warning colour: {texts:?}"
        );
        assert!(label_hidden, "the note takes the label's place");
        assert_eq!(answered, (true, false));
        assert_eq!(moved_off, (true, false), "moving the focus clears the note");
        assert_eq!(moved_back, (true, false));
    }

    #[test]
    fn escape_while_capturing_cancels_with_no_event() {
        let original = controls_screen(vec![control(0, [R, R])]);
        let mut stack = MenuStack::new();
        stack.push(original.clone());

        key_c(&mut stack, Key::Enter);
        key_c(&mut stack, Key::Escape);
        key_c(&mut stack, Key::Q);

        assert_eq!(stack.take_events(), [], "no event, and capture is over");
        assert_eq!(stack.top(), Some(&original));
    }

    #[test]
    fn escape_outside_capture_still_chooses_back() {
        let mut idle = controls_stack(controls_of(2));
        let mut after_cancel = controls_stack(controls_of(2));

        key_c(&mut idle, Key::Escape);
        key_c(&mut after_cancel, Key::Enter);
        key_c(&mut after_cancel, Key::Escape);
        key_c(&mut after_cancel, Key::Escape);

        assert_eq!(idle.take_events(), [MenuEvent::Chosen(Bind::Back)]);
        assert_eq!(
            after_cancel.take_events(),
            [MenuEvent::Chosen(Bind::Back)],
            "the first Escape cancelled, the second chose Back"
        );
    }

    #[test]
    fn enter_and_unmodified_arrows_are_refused_at_capture_with_a_note() {
        for refused in [
            Key::Enter,
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
        ] {
            let mut stack = controls_stack(controls_of(2));

            key_c(&mut stack, Key::Enter);
            key_c(&mut stack, refused);
            let refusal = stack.take_events();
            let note = row_texts(&stack, 0).into_iter().find(is_warning);
            let label_hidden = !shows(&stack, SIZE, "Action 00");
            key_c(&mut stack, Key::Q);

            assert_eq!(refusal, [], "{refused:?} is refused, not captured");
            assert!(note.is_some_and(|n| !n.text.is_empty()), "{refused:?}");
            assert!(label_hidden, "{refused:?}: the note replaces the label");
            assert_eq!(
                stack.take_events(),
                [captured(Bind::Row(0), Slot::Primary, Input::Key(Key::Q))],
                "{refused:?}: capture continued on the same slot"
            );
        }
    }

    #[test]
    fn backspace_or_a_right_click_clears_the_focused_slot() {
        let rows = || {
            vec![
                control(0, [R, R]),
                control(1, [R, R]),
                bind_button("Back", Bind::Back),
            ]
        };
        let right = Input::Mouse(MouseButton::Right);
        let mut by_key = controls_stack(rows());
        let mut by_button = controls_stack(rows());
        let mut capturing_key = controls_stack(rows());
        let mut capturing_button = controls_stack(rows());
        let mut on_button_row = controls_stack(rows());

        key_c(&mut by_key, Key::ArrowRight);
        key_c(&mut by_key, Key::Backspace);
        key_c(&mut by_button, Key::ArrowDown);
        by_button.on_key(right, true, false);
        key_c(&mut capturing_key, Key::Enter);
        key_c(&mut capturing_key, Key::Backspace);
        key_c(&mut capturing_button, Key::Enter);
        capturing_button.on_key(right, true, false);
        key_c(&mut on_button_row, Key::ArrowUp);
        key_c(&mut on_button_row, Key::Backspace);
        on_button_row.on_key(right, true, false);

        assert_eq!(
            by_key.take_events(),
            [MenuEvent::Cleared {
                action: Bind::Row(0),
                slot: Slot::Secondary
            }]
        );
        assert_eq!(
            by_button.take_events(),
            [MenuEvent::Cleared {
                action: Bind::Row(1),
                slot: Slot::Primary
            }]
        );
        assert_eq!(
            capturing_key.take_events(),
            [captured(
                Bind::Row(0),
                Slot::Primary,
                Input::Key(Key::Backspace)
            )],
            "while capturing, Backspace is bindable"
        );
        assert_eq!(
            capturing_button.take_events(),
            [captured(Bind::Row(0), Slot::Primary, right)]
        );
        assert_eq!(
            on_button_row.take_events(),
            [],
            "a button has no slot to clear"
        );
    }

    #[test]
    fn a_back_and_an_apply_are_plain_events_and_the_stack_stages_no_bindings() {
        let original = controls_screen(vec![
            control(0, [R, None]),
            bind_button("Reset", Bind::Reset),
            bind_button("Apply", Bind::Apply),
            bind_button("Back", Bind::Back),
        ]);
        let mut stack = MenuStack::new();
        stack.push(original.clone());

        key_c(&mut stack, Key::Enter);
        key_c(&mut stack, Key::Q);
        let unchanged_by_capture = stack.top() == Some(&original);
        for _ in 0..3 {
            key_c(&mut stack, Key::ArrowDown);
            key_c(&mut stack, Key::Enter);
        }

        assert!(unchanged_by_capture, "the game owns the staged bindings");
        assert_eq!(
            stack.take_events(),
            [
                captured(Bind::Row(0), Slot::Primary, Input::Key(Key::Q)),
                MenuEvent::Chosen(Bind::Reset),
                MenuEvent::Chosen(Bind::Apply),
                MenuEvent::Chosen(Bind::Back),
            ]
        );
        assert_eq!(stack.top(), Some(&original), "no hidden pop or staging");
    }

    #[test]
    fn moving_the_focus_scrolls_the_row_into_view() {
        let mut stack = controls_stack(controls_of(30));
        let at_top = shown_labels(&stack);

        for _ in 0..19 {
            key_c(&mut stack, Key::ArrowDown);
        }
        let down = (row_shown(&stack, 19), row_shown(&stack, 0));
        let down_count = shown_labels(&stack).len();
        for _ in 0..19 {
            key_c(&mut stack, Key::ArrowUp);
        }

        assert_eq!(at_top.len(), 19, "608 points hold 19 rows of 32");
        assert!(at_top.iter().any(|t| t.contains("Action 18")));
        assert!(!at_top.iter().any(|t| t.contains("Action 19")));
        assert_eq!(down, (true, false));
        assert_eq!(down_count, 19, "only whole rows are drawn");
        assert!(row_shown(&stack, 0));
        assert!(!row_shown(&stack, 19));
    }

    #[test]
    fn up_from_the_first_row_wraps_to_the_last_and_scrolls_to_the_end() {
        let mut items = controls_of(27);
        items.push(bind_button("Reset", Bind::Reset));
        items.push(bind_button("Apply", Bind::Apply));
        items.push(bind_button("Back", Bind::Back));
        let mut stack = controls_stack(items);

        key_c(&mut stack, Key::ArrowUp);
        let shown = (
            shows(&stack, SIZE, "Back"),
            shows(&stack, SIZE, "Apply"),
            shows(&stack, SIZE, "Reset"),
            row_shown(&stack, 26),
            row_shown(&stack, 0),
        );
        key_c(&mut stack, Key::Enter);

        assert_eq!(shown, (true, true, true, true, false));
        assert_eq!(stack.take_events(), [MenuEvent::Chosen(Bind::Back)]);
    }

    proptest! {
        #[test]
        fn the_focused_row_is_always_inside_the_viewport(
            count in 1_usize..60,
            height in 300.0_f32..1400.0,
            moves in prop::collection::vec(any::<bool>(), 0..80),
        ) {
            let size = [1280.0, height];
            let mut stack = controls_stack(controls_of(count));

            for down in moves {
                key_c(&mut stack, if down { Key::ArrowDown } else { Key::ArrowUp });

                let focused: Vec<f32> = stack
                    .texts(size)
                    .iter()
                    .filter(|t| t.color == FOCUS)
                    .map(|t| size[1] / 2.0 + t.offset[1])
                    .collect();
                prop_assert!(!focused.is_empty(), "no focused row is drawn");
                for y in focused {
                    prop_assert!(
                        y >= VIEW_BAND - 0.01 && y <= height - VIEW_BAND + 0.01,
                        "focused row at {} outside {}..{}", y, VIEW_BAND, height - VIEW_BAND
                    );
                }
            }
        }
    }

    #[test]
    fn set_items_to_fewer_rows_clamps_focus_and_scroll() {
        let mut stack = controls_stack(controls_of(30));
        for _ in 0..25 {
            key_c(&mut stack, Key::ArrowDown);
        }

        stack.set_items(controls_of(5));
        let drawn = shown_labels(&stack).len();
        let focused = row_texts(&stack, 4).iter().any(|t| t.color == FOCUS);
        key_c(&mut stack, Key::Enter);
        key_c(&mut stack, Key::Q);

        assert_eq!(drawn, 5, "the scroll fell back to show every row");
        assert!(focused, "the focus clamped to the last row");
        assert_eq!(
            stack.take_events(),
            [captured(Bind::Row(4), Slot::Primary, Input::Key(Key::Q))]
        );
    }

    #[test]
    fn a_screen_that_fits_keeps_todays_layout() {
        let controls = controls_screen(controls_of(3));

        let buttons = layout(&main_screen(), SIZE);
        let rows = layout(&controls, SIZE);

        for rect in &buttons {
            assert_eq!(rect.max[0] - rect.min[0], 240.0);
            assert_eq!(rect.max[1] - rect.min[1], 56.0);
        }
        let button_centres: Vec<[f32; 2]> = buttons.iter().map(rect_centre).collect();
        assert_eq!(
            button_centres,
            [[640.0, 304.0], [640.0, 360.0], [640.0, 416.0]]
        );
        for rect in &rows {
            assert_eq!(
                rect.max[0] - rect.min[0],
                640.0,
                "a control row is 640 wide"
            );
            assert_eq!(rect.max[1] - rect.min[1], 32.0, "and 32 tall");
        }
        let row_centres: Vec<[f32; 2]> = rows.iter().map(rect_centre).collect();
        assert_eq!(
            row_centres,
            [[640.0, 328.0], [640.0, 360.0], [640.0, 392.0]]
        );
        let stack = controls_stack(controls_of(3));
        assert!(
            !row_texts(&stack, 0).is_empty(),
            "the rows are drawn on their rects"
        );
    }

    #[test]
    fn hover_repeats_and_releases_do_not_disturb_capture() {
        let mut stack = controls_stack(controls_of(3));

        key_c(&mut stack, Key::Enter);
        let over_last = cell_point(&stack, 2, SECONDARY_X);
        stack.hover(over_last, SIZE);
        stack.on_key(Input::Key(Key::Q), true, true);
        stack.on_key(Input::Key(Key::Q), false, false);
        stack.on_key(Input::Key(Key::ArrowDown), true, true);
        stack.on_key(Input::Mouse(MouseButton::Left), false, false);
        let quiet = stack.take_events();
        key_c(&mut stack, Key::W);

        assert_eq!(quiet, []);
        assert_eq!(
            stack.take_events(),
            [captured(Bind::Row(0), Slot::Primary, Input::Key(Key::W))],
            "focus and slot stayed where capture began"
        );
    }

    #[test]
    fn a_modifier_key_alone_is_captured() {
        for modifier in [Key::Ctrl, Key::Shift, Key::Alt] {
            let mut stack = controls_stack(controls_of(2));

            key_c(&mut stack, Key::Enter);
            key_c(&mut stack, modifier);

            assert_eq!(
                stack.take_events(),
                [captured(Bind::Row(0), Slot::Primary, Input::Key(modifier))],
                "{modifier:?}"
            );
        }
    }

    #[test]
    fn a_twelve_row_settings_screen_scrolls_at_720_points() {
        let mut items: Vec<MenuItem<Act>> = EngineSetting::ALL
            .into_iter()
            .map(MenuItem::Setting)
            .collect();
        items.push(MenuItem::Apply);
        items.push(button("Back", Act::Back));
        let mut stack = stack_of(MenuScreen {
            title: None,
            items,
            back: Some(Act::Back),
            settings: None,
        });
        let drawn =
            |stack: &MenuStack<Act>, text: &str| stack.texts(SIZE).iter().any(|t| t.text == text);
        let first = EngineSetting::ALL[0].label();
        let second = EngineSetting::ALL[1].label();
        let third = EngineSetting::ALL[2].label();

        let top = (
            drawn(&stack, first),
            drawn(&stack, "Apply"),
            drawn(&stack, "Back"),
        );
        press(&mut stack, Key::ArrowUp);
        let end = (
            drawn(&stack, first),
            drawn(&stack, second),
            drawn(&stack, third),
            drawn(&stack, "Back"),
        );

        assert_eq!(top, (true, false, false), "ten of the twelve rows fit");
        assert_eq!(
            end,
            (false, false, true, true),
            "the end of the list is reachable"
        );
    }

    /// The y of the centre of the first text reading `text` on `SIZE`.
    fn text_y<A: Clone>(stack: &MenuStack<A>, text: &str) -> f32 {
        let found = stack.texts(SIZE).into_iter().find(|t| t.text == text);
        let found = found.unwrap_or_else(|| panic!("{text:?} is not drawn"));
        SIZE[1] / 2.0 + found.offset[1]
    }

    fn scrolled_base() -> EngineSettings {
        EngineSettings {
            mouse_sensitivity: 2.5,
            mouse_smoothing: false,
            invert_mouse_y: false,
            sound_effect_volume: 5.0,
            music_volume: 6.0,
            ui_volume: 7.0,
            shadow_quality: 3,
            ssao_quality: 3,
            window_mode: moho_core::prefs::WindowMode::Windowed,
            resolution: (1920, 1080),
        }
    }

    /// Twelve settings-screen rows at 720 points, scrolled to the end: rows 2 to 11 are in view.
    fn scrolled_settings_stack() -> MenuStack<Act> {
        let mut items: Vec<MenuItem<Act>> = EngineSetting::ALL
            .into_iter()
            .map(MenuItem::Setting)
            .collect();
        items.push(MenuItem::Apply);
        items.push(button("Back", Act::Back));
        let mut stack = stack_of(MenuScreen {
            title: None,
            items,
            back: Some(Act::Back),
            settings: Some(scrolled_base()),
        });
        press(&mut stack, Key::ArrowUp);
        assert!(
            !stack
                .texts(SIZE)
                .iter()
                .any(|t| t.text == EngineSetting::ALL[0].label()),
            "the first row must be scrolled out of view"
        );
        stack
    }

    #[test]
    fn a_slider_press_and_drag_on_a_scrolled_settings_screen_hit_the_scrolled_row() {
        let volume = EngineSetting::SoundEffectVolume;
        let mut pressed = scrolled_settings_stack();
        let mut dragged = scrolled_settings_stack();
        let slider_y = text_y(&pressed, volume.label());
        let apply_at = [640.0, text_y(&pressed, "Apply")];

        pressed.click([592.0, slider_y], SIZE);
        pressed.click(apply_at, SIZE);
        dragged.click([592.0, slider_y], SIZE);
        let dragging = dragged.is_dragging();
        dragged.drag([712.0, slider_y], SIZE);
        dragged.on_key(Input::Mouse(MouseButton::Left), false, false);
        dragged.click(apply_at, SIZE);

        let applied = |sound_effect_volume| {
            vec![MenuEvent::SettingsApplied(EngineSettings {
                sound_effect_volume,
                ..scrolled_base()
            })]
        };
        assert!(dragging, "the press started a drag");
        assert_eq!(pressed.take_events(), applied(3.0));
        assert_eq!(dragged.take_events(), applied(8.0));
    }

    #[test]
    fn starting_capture_hides_the_games_note_and_shows_the_label() {
        let mut stack = controls_stack(vec![noted(0, NOTE), control(1, [R, None])]);
        let before = (shows(&stack, SIZE, NOTE), shows(&stack, SIZE, "Action 00"));

        key_c(&mut stack, Key::Enter);

        assert_eq!(before, (true, false), "the note shows until capture starts");
        assert!(!shows(&stack, SIZE, NOTE), "capture hides the note");
        assert!(shows(&stack, SIZE, "Action 00"), "the label is back");
    }

    #[test]
    fn the_shell_note_beats_the_games_note_when_both_are_present() {
        let mut stack = controls_stack(vec![control(0, [R, None]), control(1, [R, None])]);
        key_c(&mut stack, Key::Enter);
        stack.set_items(vec![noted(0, NOTE), control(1, [R, None])]);
        let game_note_alone = shows(&stack, SIZE, NOTE);

        key_c(&mut stack, Key::Enter);

        assert!(game_note_alone, "the answer's note shows during capture");
        assert!(shows(&stack, SIZE, PROTECTED_NOTE));
        assert!(!shows(&stack, SIZE, NOTE), "the shell's note wins");
        assert_eq!(stack.take_events(), []);
    }

    #[test]
    fn a_click_on_a_control_rows_label_chooses_nothing_and_starts_no_capture() {
        for offset in [1.0, 120.0, 239.5] {
            let mut stack = controls_stack(vec![control(0, [R, R]), control(1, [R, R])]);
            let point = cell_point(&stack, 1, offset);

            stack.click(point, SIZE);
            let capturing = stack.is_capturing();
            key_c(&mut stack, Key::Q);

            assert!(!capturing, "offset {offset}");
            assert_eq!(stack.take_events(), [], "offset {offset}");
        }
    }

    #[test]
    fn a_click_picks_the_cell_it_lands_on_whichever_slot_had_the_focus() {
        let mut primary_after_secondary = controls_stack(vec![control(0, [R, R])]);
        let mut secondary_after_primary = controls_stack(vec![control(0, [R, R])]);
        let mut at_cell_edge = controls_stack(vec![control(0, [R, R])]);
        let mut below_cell_edge = controls_stack(vec![control(0, [R, R])]);

        key_c(&mut primary_after_secondary, Key::ArrowRight);
        let point = cell_point(&primary_after_secondary, 0, PRIMARY_X);
        primary_after_secondary.click(point, SIZE);
        key_c(&mut primary_after_secondary, Key::Q);
        let point = cell_point(&secondary_after_primary, 0, SECONDARY_X);
        secondary_after_primary.click(point, SIZE);
        key_c(&mut secondary_after_primary, Key::Q);
        let point = cell_point(&at_cell_edge, 0, 440.0);
        at_cell_edge.click(point, SIZE);
        key_c(&mut at_cell_edge, Key::Q);
        let point = cell_point(&below_cell_edge, 0, 439.5);
        below_cell_edge.click(point, SIZE);
        key_c(&mut below_cell_edge, Key::Q);

        let q = Input::Key(Key::Q);
        assert_eq!(
            primary_after_secondary.take_events(),
            [captured(Bind::Row(0), Slot::Primary, q)]
        );
        assert_eq!(
            secondary_after_primary.take_events(),
            [captured(Bind::Row(0), Slot::Secondary, q)]
        );
        assert_eq!(
            at_cell_edge.take_events(),
            [captured(Bind::Row(0), Slot::Secondary, q)]
        );
        assert_eq!(
            below_cell_edge.take_events(),
            [captured(Bind::Row(0), Slot::Primary, q)]
        );
    }

    #[test]
    fn the_title_stays_on_screen_above_a_scrolled_control_page() {
        let screen = MenuScreen {
            title: Some("Controls".to_owned()),
            ..controls_screen(controls_of(30))
        };
        let mut stack = MenuStack::new();
        stack.push(screen);

        for scrolled in [false, true] {
            if scrolled {
                stack.scroll(3.0);
                assert!(!row_shown(&stack, 0), "the page must be scrolled");
            }
            let texts = stack.texts(SIZE);

            let title = texts
                .iter()
                .find(|t| t.text == "Controls")
                .expect("the title is drawn");
            let top = SIZE[1] / 2.0 + title.offset[1] - title.size / 2.0;
            assert!(top >= 0.0, "scrolled {scrolled}: title top at {top}");
        }
    }

    mod settings_rows {
        use moho_core::prefs::WindowMode;

        use super::*;

        /// Tall enough that the twelve 56-point rows of `screen_of` need no scrolling.
        const SIZE: [f32; 2] = [1280.0, 1000.0];

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

        /// The ten rows, Apply, then Back: twelve rows, so the whole column fits on this module's
        /// `SIZE`.
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
                other => panic!("Apply emitted {other:?}"),
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
