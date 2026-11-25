//! Item lists a game describes as data: any number of scrollable lists of rows (label and count)
//! side by side, with a move action between them. The shell owns focus, keys, layout and drawing;
//! the game decides what a move does and answers with new data.

use std::cell::Cell;
use std::ops::Range;

use moho_input::bindings::Input;
use moho_input::key::{Key, MouseButton};

use crate::menu::{
    BAND, COLOR, CONTROL_HEIGHT, DEFAULT_SIZE, FOCUS_COLOR, MenuEvent, ROW_SIZE, Rect,
    SETTING_SIZE, viewport, wheel,
};
use crate::text::{Anchor, ScreenText};

/// The name of a list on a lists screen. The game picks it (normally its container id) and keeps
/// it for as long as that container is shown; ids are unique on a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListId(pub u32);

/// A screen of lists side by side, and the action Escape chooses.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemLists<A> {
    /// The lists, left to right.
    pub lists: Vec<ItemList<A>>,
    /// What Escape chooses on this screen; `None` makes Escape do nothing.
    pub back: Option<A>,
}

/// One titled list of rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemList<A> {
    pub id: ListId,
    pub title: String,
    pub rows: Vec<ListRow<A>>,
}

/// One row of a list.
#[derive(Clone, Debug, PartialEq)]
pub struct ListRow<A> {
    pub label: String,
    /// Drawn right-aligned when `Some`.
    pub count: Option<u32>,
    /// The game's text shown beside the row while it is focused.
    pub tooltip: Option<String>,
    /// What the game is told when the row is moved.
    pub item: A,
    /// The actions the player can pick on this row; empty when the row has none.
    pub actions: Vec<RowAction<A>>,
}

/// One action a row offers: its label, what the game is told and whether it needs a second row.
#[derive(Clone, Debug, PartialEq)]
pub struct RowAction<A> {
    pub label: String,
    pub action: A,
    pub target: ActionTarget,
}

/// What an action needs besides its own row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionTarget {
    /// Nothing: picking the action is enough.
    Itself,
    /// A second row, picked after the action.
    AnotherRow,
}

/// A row named by its item and the list it is in.
#[derive(Clone, Debug, PartialEq)]
pub struct RowRef<A> {
    pub item: A,
    pub list: ListId,
}

const LIST_WIDTH: f32 = 320.0;
const LIST_GAP: f32 = 64.0;
/// Space between a list and the tooltip beside it.
const TOOLTIP_GAP: f32 = 16.0;
const ROW_HEIGHT: f32 = CONTROL_HEIGHT;
/// Starts the label of the row the player has picked up.
const HELD_MARK: &str = "» ";

/// What the shell keeps for one list: its focused row and how far it is scrolled.
#[derive(Debug, Default)]
struct View {
    focus: usize,
    /// The first visible row. A cell so that laying out, which sees the window size, can settle it.
    scroll: Cell<usize>,
    /// Set by a focus change; the next layout scrolls the focused row into view.
    reveal: Cell<bool>,
    /// The fraction of a row the wheel has scrolled and not yet shown.
    remainder: f32,
}

/// The row the player has picked up: its list and its index there, and why.
#[derive(Clone, Debug)]
struct Held<A> {
    list: ListId,
    row: usize,
    purpose: Purpose<A>,
}

/// What a held row is for.
#[derive(Clone, Debug)]
enum Purpose<A> {
    /// To be placed on another list.
    Move,
    /// A target pick for `action`, whose label is `label`.
    Action { action: A, label: String },
}

/// The open action list of a row: its list, its row index there and its focused action.
#[derive(Clone, Copy, Debug)]
struct OpenActions {
    list: ListId,
    row: usize,
    focus: usize,
}

/// Width of the action list.
const ACTION_WIDTH: f32 = 200.0;

/// An item lists screen: the game's data, and the focus, scroll and held row the shell keeps.
#[derive(Debug)]
pub(crate) struct OpenLists<A> {
    data: ItemLists<A>,
    /// One per list of `data`, in the same order.
    views: Vec<View>,
    /// The index of the focused list.
    focus: usize,
    held: Option<Held<A>>,
    actions: Option<OpenActions>,
    /// The window size of the last layout, for the wheel, which knows none.
    size: Cell<[f32; 2]>,
}

/// How many whole rows fit in a window `size` points tall; at least one.
fn fitting(size: [f32; 2]) -> usize {
    (viewport(size) / ROW_HEIGHT).floor().max(1.0) as usize
}

/// The left edge of list `index` of `count` in a window `width` points across.
fn left_of(index: usize, count: usize, width: f32) -> f32 {
    let total = LIST_WIDTH * count as f32 + LIST_GAP * (count as f32 - 1.0);
    width / 2.0 - total / 2.0 + (LIST_WIDTH + LIST_GAP) * index as f32
}

/// The list named `id` and its index.
fn list_by_id<A>(lists: &[ItemList<A>], id: ListId) -> Option<(usize, &ItemList<A>)> {
    lists.iter().enumerate().find(|(_, list)| list.id == id)
}

/// The index one step on from `at` among `count`, wrapping at either end; `count` is not 0.
fn wrap(at: usize, count: usize, forward: bool) -> usize {
    if forward {
        (at + 1) % count
    } else {
        (at + count - 1) % count
    }
}

fn ids_are_unique<A>(data: &ItemLists<A>) -> bool {
    data.lists
        .iter()
        .enumerate()
        .all(|(at, list)| data.lists[..at].iter().all(|other| other.id != list.id))
}

impl<A> OpenLists<A> {
    pub(crate) fn new(data: ItemLists<A>) -> Self {
        debug_assert!(ids_are_unique(&data), "list ids are unique on a screen");
        let views = data.lists.iter().map(|_| View::default()).collect();
        Self {
            data,
            views,
            focus: 0,
            held: None,
            actions: None,
            size: Cell::new(DEFAULT_SIZE),
        }
    }

    pub(crate) fn data(&self) -> &ItemLists<A> {
        &self.data
    }

    /// Replaces the data. Each list keeps its focused row (clamped) and scroll by id, the focus
    /// stays on its list while the id is present, and the hold stays while its list and a row
    /// with the held item remain.
    pub(crate) fn replace(&mut self, data: ItemLists<A>)
    where
        A: PartialEq,
    {
        debug_assert!(ids_are_unique(&data), "list ids are unique on a screen");
        let old = std::mem::replace(&mut self.data, data);
        let old_views = std::mem::take(&mut self.views);
        self.views = self
            .data
            .lists
            .iter()
            .map(|list| match list_by_id(&old.lists, list.id) {
                Some((at, _)) => {
                    let before = &old_views[at];
                    View {
                        focus: before.focus.min(list.rows.len().saturating_sub(1)),
                        scroll: Cell::new(before.scroll.get()),
                        reveal: Cell::new(before.reveal.get()),
                        remainder: before.remainder,
                    }
                }
                None => View::default(),
            })
            .collect();
        let last = self.data.lists.len().saturating_sub(1);
        self.focus = old
            .lists
            .get(self.focus)
            .and_then(|focused| list_by_id(&self.data.lists, focused.id))
            .map_or_else(
                || {
                    let moved = self.focus.min(last);
                    if let Some(view) = self.views.get(moved) {
                        view.reveal.set(true);
                    }
                    moved
                },
                |(at, _)| at,
            );
        let held = self.held.take();
        self.held = held.and_then(|held| {
            let (_, before) = list_by_id(&old.lists, held.list)?;
            let item = &before.rows.get(held.row)?.item;
            let (_, now) = list_by_id(&self.data.lists, held.list)?;
            let row = now.rows.iter().position(|row| row.item == *item)?;
            if let Purpose::Action { action, .. } = &held.purpose
                && !now.rows[row].actions.iter().any(|a| a.action == *action)
            {
                return None;
            }
            Some(Held { row, ..held })
        });
        let open = self.actions.take();
        self.actions = open.and_then(|open| {
            let (_, before) = list_by_id(&old.lists, open.list)?;
            let old_row = before.rows.get(open.row)?;
            let focused = &old_row.actions.get(open.focus)?.action;
            let (_, now) = list_by_id(&self.data.lists, open.list)?;
            let row = now
                .rows
                .iter()
                .position(|row| row.item == old_row.item && !row.actions.is_empty())?;
            let actions = &now.rows[row].actions;
            let focus = actions
                .iter()
                .position(|a| a.action == *focused)
                .unwrap_or_else(|| open.focus.min(actions.len() - 1));
            Some(OpenActions {
                list: open.list,
                row,
                focus,
            })
        });
    }

    /// Where the open action list is on a window `size` points across: the left edge of its
    /// column, the top of its first action and how many actions it has.
    fn action_layout(&self, size: [f32; 2]) -> Option<(f32, f32, usize)> {
        let open = self.actions?;
        let (index, list) = list_by_id(&self.data.lists, open.list)?;
        let count = list.rows.get(open.row)?.actions.len();
        let left = left_of(index, self.data.lists.len(), size[0]);
        let x = if index == 0 {
            left - TOOLTIP_GAP - ACTION_WIDTH
        } else {
            left + LIST_WIDTH + TOOLTIP_GAP
        };
        let scroll = self.views[index].scroll.get();
        let top = BAND + ROW_HEIGHT * (open.row as f32 - scroll as f32);
        let height = ROW_HEIGHT * count as f32;
        let bottom = size[1] - BAND;
        Some((x, top.min(bottom - height), count))
    }

    /// The rectangles of the open action list's actions, if one is open.
    fn action_rects(&self, size: [f32; 2]) -> Vec<Rect> {
        let Some((x, top, count)) = self.action_layout(size) else {
            return Vec::new();
        };
        (0..count)
            .map(|i| Rect {
                min: [x, top + ROW_HEIGHT * i as f32],
                max: [x + ACTION_WIDTH, top + ROW_HEIGHT * (i + 1) as f32],
            })
            .collect()
    }

    /// The action under `point` in the open action list.
    fn action_at(&self, point: [f32; 2], size: [f32; 2]) -> Option<usize> {
        self.settle_all(size);
        self.action_rects(size).iter().position(|r| {
            point[0] > r.min[0] && point[0] < r.max[0] && point[1] > r.min[1] && point[1] < r.max[1]
        })
    }

    /// Opens the actions of the focused row, if it has any.
    fn open_actions(&mut self) {
        let Some(list) = self.data.lists.get(self.focus) else {
            return;
        };
        let row = self.views[self.focus].focus;
        if list
            .rows
            .get(row)
            .is_some_and(|row| !row.actions.is_empty())
        {
            self.actions = Some(OpenActions {
                list: list.id,
                row,
                focus: 0,
            });
        }
    }

    /// A right press at `point`: closes the actions, lets go of a held row, or focuses the row
    /// under `point` and opens its actions.
    pub(crate) fn right_click(&mut self, point: [f32; 2], size: [f32; 2]) {
        if self.actions.take().is_some() {
            return;
        }
        if self.held.is_some() {
            self.release();
            return;
        }
        if let Some((list, Some(row))) = self.hit(point, size) {
            self.set_focus(list, row);
            self.open_actions();
        }
    }

    /// Clamps the scroll of list `index` for a window `size` points across, then brings its
    /// focused row into view if the focus moved.
    fn settle(&self, index: usize, size: [f32; 2]) {
        let (Some(list), Some(view)) = (self.data.lists.get(index), self.views.get(index)) else {
            return;
        };
        let fit = fitting(size);
        let last = list.rows.len().saturating_sub(fit);
        let mut scroll = view.scroll.get().min(last);
        if view.reveal.take() {
            if view.focus < scroll {
                scroll = view.focus;
            } else if view.focus >= scroll + fit {
                scroll = view.focus + 1 - fit;
            }
        }
        view.scroll.set(scroll);
    }

    fn settle_all(&self, size: [f32; 2]) {
        self.size.set(size);
        for index in 0..self.data.lists.len() {
            self.settle(index, size);
        }
    }

    /// The rows of list `index` fully in view, after `settle_all`.
    fn visible(&self, index: usize, size: [f32; 2]) -> Range<usize> {
        let scroll = self.views[index].scroll.get();
        scroll..(scroll + fitting(size)).min(self.data.lists[index].rows.len())
    }

    /// The list column under `point` and the visible row there, if any.
    fn hit(&self, point: [f32; 2], size: [f32; 2]) -> Option<(usize, Option<usize>)> {
        self.settle_all(size);
        if point[1] <= BAND || point[1] >= size[1] - BAND {
            return None;
        }
        let count = self.data.lists.len();
        let index = (0..count).find(|&index| {
            let left = left_of(index, count, size[0]);
            point[0] > left && point[0] < left + LIST_WIDTH
        })?;
        let row = self.views[index].scroll.get() + ((point[1] - BAND) / ROW_HEIGHT) as usize;
        Some((
            index,
            self.visible(index, size).contains(&row).then_some(row),
        ))
    }

    fn set_focus(&mut self, list: usize, row: usize) {
        self.focus = list;
        let view = &mut self.views[list];
        view.focus = row;
        view.reveal.set(true);
    }

    /// Focuses the row under `point`; a point over no row keeps the focus.
    pub(crate) fn hover(&mut self, point: [f32; 2], size: [f32; 2]) {
        if self.actions.is_some() {
            if let Some(at) = self.action_at(point, size)
                && let Some(open) = &mut self.actions
            {
                open.focus = at;
            }
            return;
        }
        if let Some((list, Some(row))) = self.hit(point, size) {
            self.set_focus(list, row);
        }
    }

    /// Scrolls the focused list by `rows` rows, keeping a fractional remainder.
    pub(crate) fn scroll(&mut self, rows: f32) {
        if self.actions.is_some() {
            return;
        }
        let size = self.size.get();
        self.settle(self.focus, size);
        let (Some(list), Some(view)) = (
            self.data.lists.get(self.focus),
            self.views.get_mut(self.focus),
        ) else {
            return;
        };
        let last = list.rows.len().saturating_sub(fitting(size));
        let (scroll, remainder) = wheel(view.scroll.get(), view.remainder, rows, last);
        view.scroll.set(scroll);
        view.remainder = remainder;
    }

    /// Lets go of the held row; a target pick focuses its source row again.
    pub(crate) fn release(&mut self) {
        if let Some(held) = self.held.take()
            && matches!(held.purpose, Purpose::Action { .. })
            && let Some((index, _)) = list_by_id(&self.data.lists, held.list)
        {
            self.set_focus(index, held.row);
        }
    }

    /// The text of every list on a window `size` points across: each title in the top band, the
    /// visible rows and the focused row's tooltip.
    pub(crate) fn texts(&self, size: [f32; 2]) -> Vec<ScreenText> {
        self.settle_all(size);
        let count = self.data.lists.len();
        let text = |text: String, anchor, x: f32, y: f32, text_size, color| ScreenText {
            text,
            anchor,
            offset: [x, y - size[1] / 2.0],
            size: text_size,
            color,
        };
        let mut texts = Vec::new();
        let mut tooltip = None;
        for (index, list) in self.data.lists.iter().enumerate() {
            let left = left_of(index, count, size[0]);
            let view = &self.views[index];
            texts.push(text(
                list.title.clone(),
                Anchor::Center,
                left + LIST_WIDTH / 2.0 - size[0] / 2.0,
                BAND / 2.0,
                ROW_SIZE,
                COLOR,
            ));
            for row in self.visible(index, size) {
                let y = BAND + ROW_HEIGHT * (row - view.scroll.get()) as f32 + ROW_HEIGHT / 2.0;
                let focused = index == self.focus && row == view.focus;
                let color = if focused { FOCUS_COLOR } else { COLOR };
                let held = self
                    .held
                    .as_ref()
                    .filter(|held| held.list == list.id && held.row == row);
                let item = &list.rows[row];
                let label = match held.map(|held| &held.purpose) {
                    Some(Purpose::Action { label: action, .. }) => {
                        format!("{HELD_MARK}{action}: {}", item.label)
                    }
                    Some(Purpose::Move) => format!("{HELD_MARK}{}", item.label),
                    None => item.label.clone(),
                };
                texts.push(text(label, Anchor::Left, left, y, SETTING_SIZE, color));
                if let Some(count) = item.count {
                    texts.push(text(
                        count.to_string(),
                        Anchor::Right,
                        left + LIST_WIDTH - size[0],
                        y,
                        SETTING_SIZE,
                        color,
                    ));
                }
                if focused
                    && self.actions.is_none()
                    && let Some(tip) = &item.tooltip
                {
                    tooltip = Some((index == 0, left, y, tip.clone()));
                }
            }
        }
        if let Some((first, left, y, tip)) = tooltip {
            let (anchor, x) = if first {
                (Anchor::Right, left - TOOLTIP_GAP - size[0])
            } else {
                (Anchor::Left, left + LIST_WIDTH + TOOLTIP_GAP)
            };
            texts.push(text(tip, anchor, x, y, SETTING_SIZE, COLOR));
        }
        if let (Some(open), Some((x, top, _))) = (self.actions, self.action_layout(size))
            && let Some((index, list)) = list_by_id(&self.data.lists, open.list)
            && let Some(row) = list.rows.get(open.row)
        {
            let (anchor, x) = if index == 0 {
                (Anchor::Right, x + ACTION_WIDTH - size[0])
            } else {
                (Anchor::Left, x)
            };
            for (at, action) in row.actions.iter().enumerate() {
                let color = if at == open.focus { FOCUS_COLOR } else { COLOR };
                let y = top + ROW_HEIGHT * at as f32 + ROW_HEIGHT / 2.0;
                texts.push(text(
                    action.label.clone(),
                    anchor,
                    x,
                    y,
                    SETTING_SIZE,
                    color,
                ));
            }
        }
        texts
    }

    /// The rectangles of every list's title and visible rows, list by list.
    pub(crate) fn visible_rows(&self, size: [f32; 2]) -> Vec<Rect> {
        self.settle_all(size);
        let count = self.data.lists.len();
        let mut rects = Vec::new();
        for index in 0..count {
            let left = left_of(index, count, size[0]);
            rects.push(Rect {
                min: [left, 0.0],
                max: [left + LIST_WIDTH, BAND],
            });
            let scroll = self.views[index].scroll.get();
            for row in self.visible(index, size) {
                let top = BAND + ROW_HEIGHT * (row - scroll) as f32;
                rects.push(Rect {
                    min: [left, top],
                    max: [left + LIST_WIDTH, top + ROW_HEIGHT],
                });
            }
        }
        rects.extend(self.action_rects(size));
        rects
    }
}

impl<A: Clone> OpenLists<A> {
    /// Up and Down move the focus in the focused list and wrap; Left and Right switch list and
    /// wrap; a non-repeat Enter picks up or places; a non-repeat Escape lets go of the held row
    /// or chooses the back action; a non-repeat right click lets go.
    pub(crate) fn on_key(&mut self, input: Input, repeat: bool) -> Option<MenuEvent<A>> {
        if self.actions.is_some() {
            return self.on_key_in_actions(input, repeat);
        }
        match input {
            Input::Key(Key::Space) if !repeat && self.held.is_none() => self.open_actions(),
            Input::Key(Key::ArrowDown) => self.step_row(true),
            Input::Key(Key::ArrowUp) => self.step_row(false),
            Input::Key(Key::ArrowLeft) => self.step_list(false),
            Input::Key(Key::ArrowRight) => self.step_list(true),
            Input::Key(Key::Enter) if !repeat => {
                let row = self
                    .views
                    .get(self.focus)
                    .map(|view| view.focus)
                    .filter(|&row| row < self.data.lists[self.focus].rows.len());
                return self.press_on(self.focus, row);
            }
            Input::Key(Key::Escape) if !repeat => {
                return if self.held.is_some() {
                    self.release();
                    None
                } else {
                    self.data.back.clone().map(MenuEvent::Chosen)
                };
            }
            Input::Mouse(MouseButton::Right) if !repeat => self.release(),
            _ => {}
        }
        None
    }

    /// A left press at `point`: picks up the row under it, or places the held row on the list
    /// column under it.
    pub(crate) fn click(&mut self, point: [f32; 2], size: [f32; 2]) -> Option<MenuEvent<A>> {
        if self.actions.is_some() {
            let at = self.action_at(point, size);
            if let (Some(at), Some(open)) = (at, &mut self.actions) {
                open.focus = at;
                return self.pick_action();
            }
            self.actions = None;
            return None;
        }
        let (list, row) = self.hit(point, size)?;
        if self.held.is_none() {
            self.set_focus(list, row?);
        }
        self.press_on(list, row)
    }

    /// The keys while the action list is open: Up and Down move among the actions and wrap, a
    /// non-repeat Enter picks the focused one, and a non-repeat Escape, Space or right click
    /// closes the list. Nothing else does anything.
    fn on_key_in_actions(&mut self, input: Input, repeat: bool) -> Option<MenuEvent<A>> {
        let count = self.action_count();
        match input {
            Input::Key(Key::ArrowDown | Key::ArrowUp) if count > 0 => {
                if let Some(open) = &mut self.actions {
                    open.focus = wrap(open.focus, count, input == Input::Key(Key::ArrowDown));
                }
            }
            Input::Key(Key::Enter) if !repeat => return self.pick_action(),
            Input::Key(Key::Escape | Key::Space) | Input::Mouse(MouseButton::Right) if !repeat => {
                self.actions = None;
            }
            _ => {}
        }
        None
    }

    /// How many actions the open action list has.
    fn action_count(&self) -> usize {
        self.actions
            .and_then(|open| {
                let (_, list) = list_by_id(&self.data.lists, open.list)?;
                Some(list.rows.get(open.row)?.actions.len())
            })
            .unwrap_or(0)
    }

    /// Picks the focused action and closes the list: an `Itself` action is sent at once, an
    /// `AnotherRow` action holds the row to ask for a second one.
    fn pick_action(&mut self) -> Option<MenuEvent<A>> {
        let open = self.actions.take()?;
        let (_, list) = list_by_id(&self.data.lists, open.list)?;
        let row = list.rows.get(open.row)?;
        let picked = row.actions.get(open.focus)?;
        match picked.target {
            ActionTarget::Itself => Some(MenuEvent::ActionPicked {
                action: picked.action.clone(),
                item: row.item.clone(),
                list: open.list,
                target: None,
            }),
            ActionTarget::AnotherRow => {
                self.held = Some(Held {
                    list: open.list,
                    row: open.row,
                    purpose: Purpose::Action {
                        action: picked.action.clone(),
                        label: picked.label.clone(),
                    },
                });
                None
            }
        }
    }

    fn step_row(&mut self, forward: bool) {
        let Some(len) = self.data.lists.get(self.focus).map(|list| list.rows.len()) else {
            return;
        };
        if len > 0 {
            let view = &mut self.views[self.focus];
            view.focus = wrap(view.focus, len, forward);
            view.reveal.set(true);
        }
    }

    fn step_list(&mut self, forward: bool) {
        let count = self.data.lists.len();
        if count > 0 {
            self.focus = wrap(self.focus, count, forward);
            self.views[self.focus].reveal.set(true);
        }
    }

    /// Enter or a click on list `index`, over `row` if any: picks `row` up with nothing held,
    /// lets go on the held row's own list, and otherwise asks to move the held row here.
    fn press_on(&mut self, index: usize, row: Option<usize>) -> Option<MenuEvent<A>> {
        let id = self.data.lists.get(index)?.id;
        let Some(held) = self.held.clone() else {
            self.held = Some(Held {
                list: id,
                row: row?,
                purpose: Purpose::Move,
            });
            return None;
        };
        if let Purpose::Action { action, .. } = held.purpose {
            let row = row?;
            self.held = None;
            if (held.list, held.row) == (id, row) {
                return None;
            }
            let (_, from) = list_by_id(&self.data.lists, held.list)?;
            let item = from.rows.get(held.row)?.item.clone();
            let target = RowRef {
                item: self.data.lists[index].rows[row].item.clone(),
                list: id,
            };
            return Some(MenuEvent::ActionPicked {
                action,
                item,
                list: held.list,
                target: Some(target),
            });
        }
        self.held = None;
        if held.list == id {
            return None;
        }
        let (_, from) = list_by_id(&self.data.lists, held.list)?;
        let item = from.rows.get(held.row)?.item.clone();
        Some(MenuEvent::MoveRequested {
            item,
            from: held.list,
            to: id,
        })
    }
}

#[cfg(test)]
mod tests {
    use moho_input::bindings::Input;
    use moho_input::key::Key;
    use proptest::prelude::{prop, prop_assert, proptest};
    use proptest::test_runner::TestCaseError;

    use super::*;
    use crate::menu::{MenuEvent, MenuStack, TEXT_COLORS};
    use crate::text::{Anchor, ScreenText};

    const SIZE: [f32; 2] = [1280.0, 720.0];
    const WIDE: [f32; 2] = [1600.0, 720.0];
    const COLOR: [u8; 4] = TEXT_COLORS[0];
    const FOCUS: [u8; 4] = TEXT_COLORS[1];

    /// A requested move as `(item, from, to)`.
    type Moved = (u32, ListId, ListId);

    fn row(label: &str, count: Option<u32>, tooltip: Option<&str>, item: u32) -> ListRow<u32> {
        ListRow {
            label: label.to_owned(),
            count,
            tooltip: tooltip.map(str::to_owned),
            item,
            actions: vec![],
        }
    }

    fn list(id: u32, title: &str, rows: Vec<ListRow<u32>>) -> ItemList<u32> {
        ItemList {
            id: ListId(id),
            title: title.to_owned(),
            rows,
        }
    }

    fn lists_of(lists: Vec<ItemList<u32>>) -> ItemLists<u32> {
        ItemLists { lists, back: None }
    }

    fn with_back(lists: Vec<ItemList<u32>>, back: u32) -> ItemLists<u32> {
        ItemLists {
            lists,
            back: Some(back),
        }
    }

    fn stack_of(lists: Vec<ItemList<u32>>) -> MenuStack<u32> {
        let mut stack = MenuStack::new();
        stack.push_lists(lists_of(lists));
        stack
    }

    /// Bare rows named `<prefix>00`, `<prefix>01`, ... with items `first`, `first + 1`, ...
    fn rows_named(prefix: &str, count: usize, first: u32) -> Vec<ListRow<u32>> {
        (0..count)
            .map(|i| row(&format!("{prefix}{i:02}"), None, None, first + i as u32))
            .collect()
    }

    fn press(stack: &mut MenuStack<u32>, key: Key) {
        stack.on_key(Input::Key(key), true, false);
    }

    fn presses(stack: &mut MenuStack<u32>, key: Key, times: usize) {
        for _ in 0..times {
            press(stack, key);
        }
    }

    /// The move requests since the last call; any other event fails the test.
    fn moves(stack: &mut MenuStack<u32>) -> Vec<Moved> {
        stack
            .take_events()
            .into_iter()
            .map(|event| match event {
                MenuEvent::MoveRequested { item, from, to } => (item, from, to),
                other => panic!("expected only move requests, got {other:?}"),
            })
            .collect()
    }

    /// The vertical centre of a text, whatever its anchor.
    fn y_of(text: &ScreenText) -> f32 {
        SIZE[1] / 2.0 + text.offset[1]
    }

    fn find<'a>(texts: &'a [ScreenText], text: &str) -> &'a ScreenText {
        texts
            .iter()
            .find(|t| t.text == text)
            .unwrap_or_else(|| panic!("no text {text:?} among {texts:?}"))
    }

    fn is_shown(texts: &[ScreenText], text: &str) -> bool {
        texts.iter().any(|t| t.text == text)
    }

    /// The labels drawn in the focus colour.
    fn focused(texts: &[ScreenText]) -> Vec<&str> {
        texts
            .iter()
            .filter(|t| t.color == FOCUS)
            .map(|t| t.text.as_str())
            .collect()
    }

    /// The texts of the held row: those that start with the hold marker.
    fn marked(stack: &MenuStack<u32>) -> Vec<String> {
        stack
            .texts(SIZE)
            .into_iter()
            .filter(|t| t.text.starts_with("» "))
            .map(|t| t.text)
            .collect()
    }

    fn focused_labels(stack: &MenuStack<u32>) -> String {
        focused(&stack.texts(SIZE)).join(",")
    }

    /// The centre of row `index` of any list: rows start at the top band, 32 points each.
    fn row_y(index: usize) -> f32 {
        56.0 + 32.0 * index as f32 + 16.0
    }

    /// The left edge of list `index` of `of` lists in a window `width` wide: 320 wide, 64 apart,
    /// centred on the window.
    fn list_left_in(width: f32, index: usize, of: usize) -> f32 {
        let total = 320.0 * of as f32 + 64.0 * (of as f32 - 1.0);
        width / 2.0 - total / 2.0 + 384.0 * index as f32
    }

    /// A point over the middle of row `row` of list `index` of `of` lists, in a window `width`
    /// wide.
    fn over_in(width: f32, index: usize, of: usize, row: usize) -> [f32; 2] {
        [list_left_in(width, index, of) + 160.0, row_y(row)]
    }

    fn over(index: usize, of: usize, row: usize) -> [f32; 2] {
        over_in(SIZE[0], index, of, row)
    }

    #[test]
    fn a_row_draws_its_label_left_and_its_count_right() {
        let stack = stack_of(vec![
            list(
                0,
                "Carried",
                vec![
                    row("Bandage", Some(3), None, 1),
                    row("Rounds", Some(60), None, 2),
                    row("Note", None, None, 3),
                ],
            ),
            list(1, "Container", vec![row("Rifle", Some(1), None, 4)]),
        ]);

        let texts = stack.texts(SIZE);

        let at = |anchor, x: f32, row: usize, color| (anchor, [x, row_y(row) - 360.0], 18.0, color);
        let shape = |text: &str| {
            let t = find(&texts, text);
            (t.anchor, t.offset, t.size, t.color)
        };
        assert_eq!(shape("Bandage"), at(Anchor::Left, 288.0, 0, FOCUS));
        assert_eq!(shape("3"), at(Anchor::Right, 608.0 - 1280.0, 0, FOCUS));
        assert_eq!(shape("Rounds"), at(Anchor::Left, 288.0, 1, COLOR));
        assert_eq!(shape("60"), at(Anchor::Right, 608.0 - 1280.0, 1, COLOR));
        assert_eq!(shape("Note"), at(Anchor::Left, 288.0, 2, COLOR));
        let on_note_row = texts.iter().filter(|t| y_of(t) == row_y(2)).count();
        assert_eq!(on_note_row, 1, "a row without a count draws only its label");
        assert_eq!(shape("Rifle"), at(Anchor::Left, 672.0, 0, COLOR));
        assert_eq!(shape("1"), at(Anchor::Right, 992.0 - 1280.0, 0, COLOR));
    }

    #[test]
    fn each_title_is_centred_over_its_list_in_the_top_band() {
        let stack = stack_of(vec![
            list(0, "Carried", rows_named("a", 1, 1)),
            list(1, "Container", rows_named("b", 1, 2)),
        ]);

        let texts = stack.texts(SIZE);

        for (title, centre) in [("Carried", 448.0), ("Container", 832.0)] {
            let t = find(&texts, title);
            assert_eq!(t.anchor, Anchor::Center, "{title}");
            assert_eq!(SIZE[0] / 2.0 + t.offset[0], centre, "{title}");
            assert!((0.0..56.0).contains(&y_of(t)), "{title} at {}", y_of(t));
            assert_eq!(t.color, COLOR, "{title}");
        }
    }

    #[test]
    fn down_to_the_last_row_scrolls_it_into_view() {
        let mut stack = stack_of(vec![list(0, "Carried", rows_named("Row ", 40, 0))]);

        presses(&mut stack, Key::ArrowDown, 18);
        let at_the_bottom_edge = stack.texts(SIZE);
        press(&mut stack, Key::ArrowDown);
        let one_past = stack.texts(SIZE);
        presses(&mut stack, Key::ArrowDown, 20);
        let last = stack.texts(SIZE);

        assert_eq!(y_of(find(&at_the_bottom_edge, "Row 00")), row_y(0));
        assert_eq!(y_of(find(&at_the_bottom_edge, "Row 18")), row_y(18));
        assert!(!is_shown(&one_past, "Row 00"));
        assert_eq!(y_of(find(&one_past, "Row 19")), row_y(18));
        assert_eq!(y_of(find(&last, "Row 39")), 648.0);
        assert_eq!(find(&last, "Row 39").color, FOCUS);
        assert!(!is_shown(&last, "Row 00"));
        assert!(!is_shown(&last, "Row 20"));
    }

    /// What the property test expects the shell to show: each list's id, row count and focused
    /// row, and the focused list.
    #[derive(Clone, Debug)]
    struct Model {
        lists: Vec<(u32, usize, usize)>,
        at: usize,
        next_id: u32,
    }

    impl Model {
        fn new(counts: &[usize]) -> Self {
            Self {
                lists: counts
                    .iter()
                    .enumerate()
                    .map(|(i, &rows)| (i as u32, rows, 0))
                    .collect(),
                at: 0,
                next_id: counts.len() as u32,
            }
        }

        fn data(&self) -> ItemLists<u32> {
            lists_of(
                self.lists
                    .iter()
                    .map(|&(id, rows, _)| {
                        list(
                            id,
                            &format!("T{id}"),
                            rows_named(&format!("L{id} R"), rows, 1000 * id),
                        )
                    })
                    .collect(),
            )
        }

        fn step_row(&mut self, forward: bool) {
            if let Some((_, rows, at)) = self.lists.get_mut(self.at)
                && *rows > 0
            {
                *at = if forward {
                    (*at + 1) % *rows
                } else {
                    (*at + *rows - 1) % *rows
                };
            }
        }

        fn step_list(&mut self, forward: bool) {
            let len = self.lists.len();
            if len > 0 {
                self.at = if forward {
                    (self.at + 1) % len
                } else {
                    (self.at + len - 1) % len
                };
            }
        }

        fn remove(&mut self, arg: usize) {
            if self.lists.is_empty() {
                return;
            }
            let gone = arg % self.lists.len();
            self.lists.remove(gone);
            if gone < self.at {
                self.at -= 1;
            }
            self.at = self.at.min(self.lists.len().saturating_sub(1));
        }

        fn add(&mut self, rows: usize) {
            if self.lists.len() < 6 {
                self.lists.push((self.next_id, rows, 0));
                self.next_id += 1;
            }
        }

        fn check(&self, stack: &MenuStack<u32>, size: [f32; 2]) -> Result<(), TestCaseError> {
            let Some(&(id, rows, at)) = self.lists.get(self.at) else {
                return Ok(());
            };
            if rows == 0 {
                return Ok(());
            }
            let label = format!("L{id} R{at:02}");
            let texts = stack.texts(size);
            let shown = texts.iter().find(|t| t.text == label);
            prop_assert!(shown.is_some(), "{label} is not drawn");
            let y = size[1] / 2.0 + shown.map_or(0.0, |t| t.offset[1]);
            prop_assert!(
                (56.0 + 16.0..=size[1] - 56.0 - 16.0).contains(&y),
                "{label} is centred at {y} in a window {} tall",
                size[1]
            );
            Ok(())
        }
    }

    proptest! {
        #[test]
        fn the_focused_row_is_always_visible(
            counts in prop::collection::vec(0usize..=100, 1..=4),
            steps in prop::collection::vec((0u8..6, 0usize..=100), 0..80),
            height in 480.0f32..=1440.0,
        ) {
            let size = [1280.0, height];
            let mut model = Model::new(&counts);
            let mut stack = MenuStack::new();
            stack.push_lists(model.data());
            model.check(&stack, size)?;

            for (op, arg) in steps {
                match op {
                    0 => {
                        model.step_row(false);
                        press(&mut stack, Key::ArrowUp);
                    }
                    1 => {
                        model.step_row(true);
                        press(&mut stack, Key::ArrowDown);
                    }
                    2 => {
                        model.step_list(false);
                        press(&mut stack, Key::ArrowLeft);
                    }
                    3 => {
                        model.step_list(true);
                        press(&mut stack, Key::ArrowRight);
                    }
                    4 => {
                        model.remove(arg);
                        stack.set_lists(model.data());
                    }
                    _ => {
                        model.add(arg);
                        stack.set_lists(model.data());
                    }
                }
                model.check(&stack, size)?;
            }
        }
    }

    #[test]
    fn up_and_down_wrap_within_the_focused_list() {
        let mut stack = stack_of(vec![list(0, "Carried", rows_named("a", 3, 10))]);

        press(&mut stack, Key::ArrowUp);
        let up_from_the_first = focused_labels(&stack);
        press(&mut stack, Key::ArrowDown);
        let down_from_the_last = focused_labels(&stack);
        stack.on_key(Input::Key(Key::ArrowDown), true, true);
        let a_repeat_still_moves = focused_labels(&stack);

        assert_eq!(up_from_the_first, "a02");
        assert_eq!(down_from_the_last, "a00");
        assert_eq!(a_repeat_still_moves, "a01");
    }

    #[test]
    fn pick_up_then_place_by_key_or_click_requests_a_move_and_changes_no_rows() {
        let data = lists_of(vec![
            list(
                5,
                "Carried",
                vec![
                    row("Bandage", Some(3), None, 10),
                    row("Rounds", Some(60), None, 11),
                ],
            ),
            list(6, "Container", vec![row("Rifle", Some(1), None, 20)]),
        ]);
        let mut stack = MenuStack::new();
        stack.push_lists(data.clone());

        stack.on_key(Input::Key(Key::Enter), true, true);
        stack.on_key(Input::Key(Key::Enter), false, false);
        let repeat_and_release = (moves(&mut stack), marked(&stack));
        press(&mut stack, Key::Enter);
        let picked = (moves(&mut stack), marked(&stack), stack.texts(SIZE));
        press(&mut stack, Key::ArrowRight);
        press(&mut stack, Key::Enter);
        let placed = (moves(&mut stack), marked(&stack));
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowLeft);
        press(&mut stack, Key::Enter);
        let placed_back = (moves(&mut stack), marked(&stack));
        stack.click([640.0, row_y(0)], SIZE);
        stack.click([5.0, 5.0], SIZE);
        stack.click(over(0, 2, 5), SIZE);
        let misses = (moves(&mut stack), marked(&stack));
        stack.click(over(0, 2, 1), SIZE);
        let clicked_up = (moves(&mut stack), marked(&stack));
        stack.click(over(1, 2, 5), SIZE);
        let clicked_down_on_the_empty_space = (moves(&mut stack), marked(&stack));
        stack.click(over(1, 2, 0), SIZE);
        stack.click(over(0, 2, 0), SIZE);
        let clicked_across_rows = (moves(&mut stack), marked(&stack));

        assert_eq!(repeat_and_release, (vec![], vec![]));
        assert_eq!(picked.0, []);
        assert_eq!(picked.1, ["» Bandage"], "the held row's label has a prefix");
        assert_eq!(
            find(&picked.2, "» Bandage").anchor,
            Anchor::Left,
            "the held label stays a left-aligned label"
        );
        assert!(is_shown(&picked.2, "3"), "the count is still drawn");
        assert_eq!(placed, (vec![(10, ListId(5), ListId(6))], vec![]));
        assert_eq!(placed_back, (vec![(20, ListId(6), ListId(5))], vec![]));
        assert_eq!(misses, (vec![], vec![]));
        assert_eq!(clicked_up, (vec![], vec!["» Rounds".to_owned()]));
        assert_eq!(
            clicked_down_on_the_empty_space,
            (vec![(11, ListId(5), ListId(6))], vec![])
        );
        assert_eq!(
            clicked_across_rows,
            (vec![(20, ListId(6), ListId(5))], vec![])
        );
        assert_eq!(stack.top_lists(), Some(&data), "the shell moves no row");
    }

    #[test]
    fn a_held_row_stays_held_while_focus_moves() {
        let mut stack = stack_of(vec![list(0, "A", rows_named("a", 3, 10))]);

        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowDown);
        let texts = stack.texts(SIZE);

        assert_eq!(marked(&stack), ["» a00"]);
        assert_eq!(find(&texts, "» a00").color, COLOR);
        assert_eq!(focused(&texts), ["a01"]);
    }

    #[test]
    fn placing_on_its_own_list_escape_or_right_click_releases_the_hold_and_requests_nothing() {
        let mut stack = MenuStack::new();
        stack.push_lists(with_back(
            vec![
                list(1, "A", rows_named("a", 3, 10)),
                list(2, "B", rows_named("b", 2, 20)),
            ],
            99,
        ));
        let mut after = Vec::new();
        let mut held_then = |stack: &mut MenuStack<u32>, release: &dyn Fn(&mut MenuStack<u32>)| {
            press(stack, Key::Enter);
            let was_held = marked(stack).len();
            release(stack);
            after.push((was_held, marked(stack), stack.take_events()));
        };

        held_then(&mut stack, &|s| press(s, Key::Enter));
        held_then(&mut stack, &|s| press(s, Key::Escape));
        held_then(&mut stack, &|s| s.right_click(over(1, 2, 0), SIZE));
        held_then(&mut stack, &|s| s.right_click([5.0, 5.0], SIZE));
        held_then(&mut stack, &|s| s.click(over(0, 2, 2), SIZE));
        held_then(&mut stack, &|s| s.click(over(0, 2, 7), SIZE));
        let nothing_held = (stack.take_events(), marked(&stack));
        stack.right_click(over(0, 2, 0), SIZE);
        let right_click_with_nothing_held = (stack.take_events(), marked(&stack));
        press(&mut stack, Key::Escape);
        let escape_with_nothing_held = stack.take_events();

        for (case, (was_held, still_marked, events)) in after.iter().enumerate() {
            assert_eq!(*was_held, 1, "case {case} held a row");
            assert_eq!(*still_marked, Vec::<String>::new(), "case {case}");
            assert_eq!(*events, [], "case {case} requests nothing");
        }
        assert_eq!(nothing_held, (vec![], vec![]));
        assert_eq!(right_click_with_nothing_held, (vec![], vec![]));
        assert_eq!(escape_with_nothing_held, [MenuEvent::Chosen(99)]);
    }

    #[test]
    fn a_held_row_can_be_placed_on_an_empty_list() {
        let mut stack = stack_of(vec![
            list(1, "Pack", rows_named("a", 2, 10)),
            list(2, "Ground", Vec::new()),
        ]);

        press(&mut stack, Key::ArrowDown);
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowRight);
        press(&mut stack, Key::Enter);
        let by_key = (moves(&mut stack), marked(&stack));
        stack.click(over(0, 2, 0), SIZE);
        stack.click(over(1, 2, 0), SIZE);
        let by_click = (moves(&mut stack), marked(&stack));

        assert_eq!(by_key, (vec![(11, ListId(1), ListId(2))], vec![]));
        assert_eq!(by_click, (vec![(10, ListId(1), ListId(2))], vec![]));
    }

    #[test]
    fn set_lists_keeps_the_hold_only_while_its_list_and_item_remain() {
        let rows_of = |items: &[u32], count: Option<u32>| -> Vec<ListRow<u32>> {
            items
                .iter()
                .map(|&item| row(&format!("k{item}"), count, None, item))
                .collect()
        };
        let a = |items: &[u32], count| list(1, "A", rows_of(items, count));
        let b = || list(2, "B", rows_of(&[20, 21], None));
        let held = || {
            let mut stack = stack_of(vec![a(&[10, 11, 12], None), b()]);
            press(&mut stack, Key::ArrowDown);
            press(&mut stack, Key::Enter);
            stack
        };

        let mut moved_and_reordered = held();
        moved_and_reordered.set_lists(lists_of(vec![
            list(3, "C", rows_of(&[30], None)),
            a(&[11, 10, 12], None),
            b(),
        ]));
        let still_held = marked(&moved_and_reordered);
        press(&mut moved_and_reordered, Key::ArrowRight);
        press(&mut moved_and_reordered, Key::Enter);
        let placed = moves(&mut moved_and_reordered);

        let mut recounted = held();
        recounted.set_lists(lists_of(vec![a(&[10, 11, 12], Some(9)), b()]));
        let held_with_a_new_count = marked(&recounted);

        let mut list_gone = held();
        list_gone.set_lists(lists_of(vec![b()]));
        let after_list_gone = marked(&list_gone);
        press(&mut list_gone, Key::Enter);
        let picks_not_places = (moves(&mut list_gone), marked(&list_gone));

        let mut item_gone = held();
        item_gone.set_lists(lists_of(vec![a(&[10, 12], None), b()]));
        let after_item_gone = marked(&item_gone);
        press(&mut item_gone, Key::ArrowRight);
        press(&mut item_gone, Key::Enter);
        let picks_not_places_either = (moves(&mut item_gone), marked(&item_gone));

        assert_eq!(still_held, ["» k11"]);
        assert_eq!(placed, [(11, ListId(1), ListId(2))]);
        assert_eq!(held_with_a_new_count, ["» k11"]);
        assert_eq!(after_list_gone, Vec::<String>::new());
        assert_eq!(picks_not_places, (vec![], vec!["» k20".to_owned()]));
        assert_eq!(after_item_gone, Vec::<String>::new());
        assert_eq!(picks_not_places_either, (vec![], vec!["» k20".to_owned()]));
    }

    #[test]
    fn three_lists_lay_out_left_to_right_and_moves_name_source_and_destination() {
        let mut stack = stack_of(vec![
            list(7, "Rig", vec![row("Magazine", Some(2), None, 1)]),
            list(4, "Backpack", vec![row("Bandage", Some(3), None, 2)]),
            list(9, "Body", vec![row("Pistol", Some(1), None, 3)]),
        ]);

        let texts = stack.texts(SIZE);
        presses(&mut stack, Key::ArrowRight, 2);
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowLeft);
        press(&mut stack, Key::Enter);
        stack.click(over(2, 3, 0), SIZE);
        stack.click(over(0, 3, 0), SIZE);
        let requested = moves(&mut stack);

        for (title, label, count, left) in [
            ("Rig", "Magazine", "2", 96.0),
            ("Backpack", "Bandage", "3", 480.0),
            ("Body", "Pistol", "1", 864.0),
        ] {
            let centre = left + 160.0;
            assert_eq!(find(&texts, title).anchor, Anchor::Center, "{title}");
            assert_eq!(
                SIZE[0] / 2.0 + find(&texts, title).offset[0],
                centre,
                "{title}"
            );
            assert_eq!(find(&texts, label).offset[0], left, "{label}");
            assert_eq!(
                find(&texts, count).offset[0],
                left + 320.0 - SIZE[0],
                "{count}"
            );
        }
        assert_eq!(
            requested,
            [(3, ListId(9), ListId(4)), (3, ListId(9), ListId(7))]
        );
    }

    #[test]
    fn with_four_lists_every_ordered_pair_is_one_pick_and_one_place() {
        let lists = || -> Vec<ItemList<u32>> {
            (0..4)
                .map(|i| {
                    list(
                        40 + i,
                        &format!("List {i}"),
                        vec![row(&format!("row {i}"), None, None, i + 1)],
                    )
                })
                .collect()
        };
        let pairs: Vec<(usize, usize)> = (0..4)
            .flat_map(|from| (0..4).map(move |to| (from, to)))
            .filter(|(from, to)| from != to)
            .collect();
        let expected: Vec<Moved> = pairs
            .iter()
            .map(|&(from, to)| {
                (
                    from as u32 + 1,
                    ListId(40 + from as u32),
                    ListId(40 + to as u32),
                )
            })
            .collect();
        let mut by_key = stack_of(lists());
        let mut by_click = stack_of(lists());

        let mut at = 0;
        for &(from, to) in &pairs {
            presses(&mut by_key, Key::ArrowRight, (from + 4 - at) % 4);
            press(&mut by_key, Key::Enter);
            presses(&mut by_key, Key::ArrowRight, (to + 4 - from) % 4);
            press(&mut by_key, Key::Enter);
            at = to;
            by_click.click(over_in(WIDE[0], from, 4, 0), WIDE);
            by_click.click(over_in(WIDE[0], to, 4, 0), WIDE);
        }

        assert_eq!(pairs.len(), 12);
        assert_eq!(moves(&mut by_key), expected);
        assert_eq!(moves(&mut by_click), expected);
        assert_eq!(marked(&by_key), Vec::<String>::new());
    }

    #[test]
    fn left_and_right_switch_list_and_each_keeps_its_focus() {
        let mut stack = stack_of(vec![
            list(1, "A", rows_named("a", 3, 10)),
            list(2, "B", rows_named("b", 2, 20)),
            list(3, "C", rows_named("c", 1, 30)),
        ]);
        let mut shown = vec![focused_labels(&stack)];

        for key in [
            Key::ArrowDown,
            Key::ArrowRight,
            Key::ArrowDown,
            Key::ArrowRight,
            Key::ArrowRight,
            Key::ArrowLeft,
            Key::ArrowLeft,
        ] {
            press(&mut stack, key);
            shown.push(focused_labels(&stack));
        }
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowLeft);
        press(&mut stack, Key::Enter);

        assert_eq!(
            shown,
            ["a00", "a01", "b00", "b01", "c00", "a01", "c00", "b01"]
        );
        assert_eq!(moves(&mut stack), [(21, ListId(2), ListId(1))]);
    }

    #[test]
    fn only_the_focused_rows_tooltip_is_drawn_outside_its_list() {
        const TIPS: [&str; 4] = [
            "Clean cloth.\nStops bleeding.",
            "Loose rifle rounds.",
            "Semi-auto.\nNeeds rounds.",
            "Water.",
        ];
        let mut stack = stack_of(vec![
            list(
                0,
                "Carried",
                vec![
                    row("Bandage", Some(3), Some(TIPS[0]), 1),
                    row("Rounds", Some(60), Some(TIPS[1]), 2),
                    row("Note", None, None, 3),
                ],
            ),
            list(
                1,
                "Container",
                vec![
                    row("Rifle", Some(1), Some(TIPS[2]), 4),
                    row("Flask", Some(1), Some(TIPS[3]), 5),
                ],
            ),
        ]);
        let tips = |stack: &MenuStack<u32>| -> Vec<(String, Anchor, [f32; 2])> {
            stack
                .texts(SIZE)
                .into_iter()
                .filter(|t| TIPS.contains(&t.text.as_str()))
                .inspect(|t| assert!(TEXT_COLORS.contains(&t.color), "{t:?}"))
                .map(|t| (t.text, t.anchor, t.offset))
                .collect()
        };
        let left_of_first = |row: usize, tip: &str| {
            (
                tip.to_owned(),
                Anchor::Right,
                [288.0 - 16.0 - 1280.0, row_y(row) - 360.0],
            )
        };
        let right_of_last = |row: usize, tip: &str| {
            (
                tip.to_owned(),
                Anchor::Left,
                [992.0 + 16.0, row_y(row) - 360.0],
            )
        };

        let on_bandage = tips(&stack);
        press(&mut stack, Key::ArrowDown);
        let on_rounds = tips(&stack);
        press(&mut stack, Key::ArrowDown);
        let on_note = tips(&stack);
        press(&mut stack, Key::ArrowRight);
        let on_rifle = tips(&stack);
        press(&mut stack, Key::ArrowDown);
        let on_flask = tips(&stack);
        stack.hover(over(0, 2, 1), SIZE);
        let hovering_rounds = tips(&stack);

        assert_eq!(on_bandage, [left_of_first(0, TIPS[0])]);
        assert_eq!(on_rounds, [left_of_first(1, TIPS[1])]);
        assert_eq!(on_note, []);
        assert_eq!(on_rifle, [right_of_last(0, TIPS[2])]);
        assert_eq!(on_flask, [right_of_last(1, TIPS[3])]);
        assert_eq!(hovering_rounds, [left_of_first(1, TIPS[1])]);
    }

    #[test]
    fn a_middle_lists_tooltip_grows_right_from_its_own_edge() {
        let stack = {
            let mut stack = stack_of(vec![
                list(1, "A", rows_named("a", 1, 1)),
                list(2, "B", vec![row("b", None, Some("Middle"), 2)]),
                list(3, "C", rows_named("c", 1, 3)),
            ]);
            press(&mut stack, Key::ArrowRight);
            stack
        };

        let texts = stack.texts(SIZE);

        let tip = find(&texts, "Middle");
        assert_eq!(tip.anchor, Anchor::Left);
        assert_eq!(tip.offset, [800.0 + 16.0, row_y(0) - 360.0]);
    }

    #[test]
    fn hover_focuses_the_row_and_list_under_the_cursor_and_a_miss_keeps_it() {
        let mut stack = stack_of(vec![
            list(1, "A", rows_named("a", 3, 10)),
            list(2, "B", rows_named("b", 3, 20)),
        ]);

        stack.hover(over(1, 2, 2), SIZE);
        let on_b2 = focused_labels(&stack);
        press(&mut stack, Key::Enter);
        let held_b2 = marked(&stack);
        stack.hover([640.0, row_y(0)], SIZE);
        stack.hover([0.0, 0.0], SIZE);
        stack.hover([448.0, 20.0], SIZE);
        let after_misses = focused_labels(&stack);
        press(&mut stack, Key::Enter);
        let own_list_releases = (moves(&mut stack), marked(&stack));
        stack.hover(over(0, 2, 1), SIZE);
        let on_a1 = focused_labels(&stack);
        press(&mut stack, Key::Enter);
        stack.hover(over(1, 2, 0), SIZE);
        let on_b0 = focused_labels(&stack);
        press(&mut stack, Key::Enter);

        assert_eq!(on_b2, "b02");
        assert_eq!(held_b2, ["» b02"]);
        assert_eq!(after_misses, "» b02", "a miss keeps the focus where it was");
        assert_eq!(own_list_releases, (vec![], vec![]));
        assert_eq!(on_a1, "a01");
        assert_eq!(on_b0, "b00");
        assert_eq!(moves(&mut stack), [(11, ListId(1), ListId(2))]);
    }

    #[test]
    fn a_focused_empty_list_draws_no_tooltip_and_enter_with_nothing_held_requests_nothing() {
        let mut stack = stack_of(vec![
            list(1, "Empty", Vec::new()),
            list(
                2,
                "Container",
                vec![row("Rifle", Some(1), Some("Semi-auto."), 4)],
            ),
        ]);

        let on_empty = stack.texts(SIZE);
        for key in [Key::Enter, Key::ArrowUp, Key::ArrowDown] {
            press(&mut stack, key);
        }
        stack.click(over(0, 2, 0), SIZE);
        stack.right_click(over(0, 2, 0), SIZE);
        stack.scroll(3.0);
        let after_keys = (moves(&mut stack), marked(&stack), stack.texts(SIZE));
        press(&mut stack, Key::ArrowRight);
        let on_rifle = stack.texts(SIZE);
        press(&mut stack, Key::ArrowLeft);
        let back_on_empty = stack.texts(SIZE);
        press(&mut stack, Key::Enter);

        assert!(!is_shown(&on_empty, "Semi-auto."));
        assert!(
            is_shown(&on_empty, "Empty"),
            "an empty list keeps its title"
        );
        assert_eq!(after_keys.0, []);
        assert_eq!(after_keys.1, Vec::<String>::new());
        assert!(!is_shown(&after_keys.2, "Semi-auto."));
        assert!(is_shown(&on_rifle, "Semi-auto."));
        assert!(!is_shown(&back_on_empty, "Semi-auto."));
        assert_eq!(moves(&mut stack), []);
        assert_eq!(marked(&stack), Vec::<String>::new());
    }

    #[test]
    fn set_lists_keeps_focus_and_scroll_by_list_id_when_lists_come_and_go() {
        let long = |id: u32, prefix: &str| list(id, prefix, rows_named(prefix, 40, 1000 * id));
        let rig = || long(1, "r");
        let backpack = || long(2, "b");
        let body = || long(3, "d");
        let ground = || long(4, "g");
        let mut stack = stack_of(vec![rig(), backpack(), body()]);
        presses(&mut stack, Key::ArrowRight, 2);
        presses(&mut stack, Key::ArrowDown, 25);
        let before = stack.texts(SIZE);

        stack.set_lists(lists_of(vec![rig(), body()]));
        let backpack_dropped = stack.texts(SIZE);
        stack.set_lists(lists_of(vec![rig(), ground(), body()]));
        let ground_added = stack.texts(SIZE);
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowLeft);
        let new_list = stack.texts(SIZE);
        press(&mut stack, Key::Enter);

        for texts in [&before, &backpack_dropped, &ground_added] {
            assert_eq!(focused(texts), ["d25"]);
            assert_eq!(y_of(find(texts, "d25")), row_y(18));
            assert_eq!(y_of(find(texts, "d07")), row_y(0), "the scroll is kept");
        }
        assert_eq!(focused(&new_list), ["g00"], "a new id starts on row 0");
        assert_eq!(y_of(find(&new_list, "g00")), row_y(0), "and unscrolled");
        assert_eq!(moves(&mut stack), [(3025, ListId(3), ListId(4))]);
    }

    #[test]
    fn set_lists_reordering_the_lists_keeps_the_held_rows_list_by_id() {
        let mut stack = stack_of(vec![
            list(8, "A", rows_named("a", 1, 10)),
            list(3, "B", rows_named("b", 1, 20)),
        ]);
        press(&mut stack, Key::Enter);

        stack.set_lists(lists_of(vec![
            list(3, "B", rows_named("b", 1, 20)),
            list(8, "A", rows_named("a", 1, 10)),
        ]));
        press(&mut stack, Key::ArrowLeft);
        press(&mut stack, Key::Enter);

        assert_eq!(moves(&mut stack), [(10, ListId(8), ListId(3))]);
    }

    #[test]
    fn set_lists_keeps_a_wheel_scrolled_lists_scroll() {
        let data = || lists_of(vec![list(1, "A", rows_named("R", 40, 100))]);
        let mut stack = MenuStack::new();
        stack.push_lists(data());

        stack.scroll(10.0);
        stack.set_lists(data());
        let kept = stack.texts(SIZE);

        assert!(!is_shown(&kept, "R00"));
        assert_eq!(y_of(find(&kept, "R10")), row_y(0));
        assert_eq!(stack.top_lists(), Some(&data()));
    }

    #[test]
    fn set_lists_clamps_each_kept_lists_focus_and_scroll() {
        let data = |a: usize, b: usize| {
            lists_of(vec![
                list(1, "A", rows_named("a", a, 100)),
                list(2, "B", rows_named("b", b, 200)),
            ])
        };
        let mut stack = MenuStack::new();
        stack.push_lists(data(40, 40));
        presses(&mut stack, Key::ArrowDown, 30);
        press(&mut stack, Key::ArrowRight);
        presses(&mut stack, Key::ArrowDown, 5);

        stack.set_lists(data(25, 3));
        let focused_list = focused_labels(&stack);
        press(&mut stack, Key::ArrowLeft);
        let clamped_to_the_last_row = stack.texts(SIZE);
        stack.set_lists(data(15, 3));
        let scroll_clamped = stack.texts(SIZE);

        assert_eq!(focused_list, "b02");
        assert_eq!(focused(&clamped_to_the_last_row), ["a24"]);
        assert_eq!(y_of(find(&clamped_to_the_last_row, "a24")), 648.0);
        assert_eq!(y_of(find(&clamped_to_the_last_row, "a06")), row_y(0));
        assert_eq!(focused(&scroll_clamped), ["a14"]);
        assert_eq!(y_of(find(&scroll_clamped, "a00")), row_y(0));
        assert_eq!(y_of(find(&scroll_clamped, "a14")), row_y(14));
        assert_eq!(stack.top_lists(), Some(&data(15, 3)));
    }

    #[test]
    fn set_lists_with_the_focused_list_gone_focuses_the_same_position() {
        let list_of = |id: u32| {
            let name = ["", "a", "b", "c", "d"][id as usize];
            list(id, name, rows_named(name, 3, 10 * id))
        };
        let without = |gone: u32| lists_of((1..=4).filter(|&id| id != gone).map(list_of).collect());
        let on = |id: u32, row: usize| {
            let mut stack = stack_of((1..=4).map(list_of).collect());
            presses(&mut stack, Key::ArrowRight, id as usize - 1);
            presses(&mut stack, Key::ArrowDown, row);
            stack
        };

        let mut middle = on(3, 1);
        middle.set_lists(without(3));
        let middle_gone = focused_labels(&middle);

        let mut last = on(4, 2);
        last.set_lists(without(4));
        let last_gone = focused_labels(&last);

        let mut first = on(1, 1);
        first.set_lists(without(1));
        let first_gone = focused_labels(&first);

        let mut before_focus = on(3, 1);
        before_focus.set_lists(without(2));
        let earlier_gone = focused_labels(&before_focus);

        assert_eq!(middle_gone, "d00", "the list now at the same position");
        assert_eq!(last_gone, "c00", "clamped to the last list");
        assert_eq!(first_gone, "b00");
        assert_eq!(
            earlier_gone, "c01",
            "a list before the focus changes nothing"
        );
    }

    #[test]
    fn zero_lists_draw_nothing_and_escape_chooses_back() {
        let mut stack = MenuStack::new();
        stack.push_lists(with_back(Vec::new(), 99));

        let drawn = stack.texts(SIZE);
        for key in [
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::Enter,
        ] {
            press(&mut stack, key);
        }
        stack.click(over(0, 1, 0), SIZE);
        stack.right_click(over(0, 1, 0), SIZE);
        stack.hover(over(0, 1, 0), SIZE);
        stack.scroll(3.0);
        let quiet = stack.take_events();
        press(&mut stack, Key::Escape);
        let escaped = stack.take_events();
        stack.set_lists(with_back(vec![list(1, "A", rows_named("a", 2, 10))], 98));
        press(&mut stack, Key::ArrowDown);
        stack.set_lists(with_back(Vec::new(), 97));
        let emptied = stack.texts(SIZE);
        press(&mut stack, Key::Escape);
        let escaped_again = stack.take_events();
        stack.set_lists(with_back(vec![list(1, "A", rows_named("a", 2, 10))], 98));

        assert_eq!(drawn, []);
        assert_eq!(quiet, []);
        assert_eq!(escaped, [MenuEvent::Chosen(99)]);
        assert_eq!(emptied, []);
        assert_eq!(escaped_again, [MenuEvent::Chosen(97)]);
        assert_eq!(focused_labels(&stack), "a00");
    }

    #[test]
    fn set_lists_with_two_rows_sharing_an_item_moves_the_hold_to_the_first() {
        let data = || {
            lists_of(vec![
                list(
                    1,
                    "A",
                    vec![row("first", None, None, 7), row("second", None, None, 7)],
                ),
                list(2, "B", rows_named("b", 1, 20)),
            ])
        };
        let mut stack = MenuStack::new();
        stack.push_lists(data());
        press(&mut stack, Key::ArrowDown);
        press(&mut stack, Key::Enter);
        let before = marked(&stack);

        stack.set_lists(data());
        let after = marked(&stack);
        press(&mut stack, Key::ArrowRight);
        press(&mut stack, Key::Enter);

        assert_eq!(before, ["» second"]);
        assert_eq!(after, ["» first"]);
        assert_eq!(moves(&mut stack), [(7, ListId(1), ListId(2))]);
    }

    #[test]
    fn set_lists_dropping_the_held_list_and_an_earlier_one_keeps_focus_on_the_focused_id() {
        let list_of = |id: u32| {
            list(
                id,
                &format!("T{id}"),
                rows_named(&format!("r{id}_"), 3, id * 10),
            )
        };
        let mut stack = stack_of([10, 20, 30, 40].map(list_of).to_vec());
        press(&mut stack, Key::Enter);
        presses(&mut stack, Key::ArrowRight, 2);
        press(&mut stack, Key::ArrowDown);

        stack.set_lists(lists_of([20, 30].map(list_of).to_vec()));
        let released = marked(&stack);
        let on_30 = focused_labels(&stack);
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowRight);
        press(&mut stack, Key::Enter);

        assert_eq!(released, Vec::<String>::new());
        assert_eq!(on_30, "r30_01");
        assert_eq!(moves(&mut stack), [(301, ListId(30), ListId(20))]);
    }

    #[test]
    fn set_lists_moving_focus_to_a_wheel_scrolled_list_reveals_its_focused_row() {
        let a = || list(1, "A", rows_named("a", 40, 100));
        let b = || list(2, "B", rows_named("b", 40, 200));
        let mut stack = stack_of(vec![a(), b()]);
        press(&mut stack, Key::ArrowRight);
        stack.scroll(10.0);
        press(&mut stack, Key::ArrowLeft);

        stack.set_lists(lists_of(vec![b()]));
        let texts = stack.texts(SIZE);

        assert_eq!(focused(&texts), ["b00"]);
        assert_eq!(y_of(find(&texts, "b00")), row_y(0));
    }

    #[test]
    fn scroll_moves_the_focused_list_and_keeps_the_remainder() {
        let mut stack = stack_of(vec![
            list(1, "A", rows_named("R", 40, 100)),
            list(2, "B", rows_named("S", 40, 200)),
        ]);

        stack.scroll(0.5);
        let half = stack.texts(SIZE);
        stack.scroll(0.5);
        let whole = stack.texts(SIZE);
        press(&mut stack, Key::ArrowRight);
        stack.scroll(3.0);
        let second_list = stack.texts(SIZE);
        stack.scroll(-100.0);
        let back_to_top = stack.texts(SIZE);
        stack.scroll(1000.0);
        let at_the_end = stack.texts(SIZE);

        assert_eq!(y_of(find(&half, "R00")), row_y(0));
        assert!(!is_shown(&whole, "R00"));
        assert_eq!(y_of(find(&whole, "R01")), row_y(0));
        assert_eq!(y_of(find(&whole, "S00")), row_y(0), "the other list stays");
        assert_eq!(y_of(find(&second_list, "S03")), row_y(0));
        assert_eq!(y_of(find(&second_list, "R01")), row_y(0));
        assert_eq!(y_of(find(&back_to_top, "S00")), row_y(0));
        assert_eq!(y_of(find(&at_the_end, "S21")), row_y(0));
        assert!(!is_shown(&at_the_end, "S20"));
        assert_eq!(y_of(find(&at_the_end, "S39")), 648.0);
    }

    use proptest::prelude::{any, prop_assert_eq, prop_assume};
    use proptest::strategy::Strategy;

    /// Needs a second row.
    const LOAD: u32 = 1;
    /// Needs nothing else.
    const UNLOAD: u32 = 2;
    const LOAD_LABEL: &str = "Load from stack";
    const PICKING_LOAD_FROM_MAGAZINE: &str = "» Load from stack: Magazine";

    fn action(label: &str, action: u32, target: ActionTarget) -> RowAction<u32> {
        RowAction {
            label: label.to_owned(),
            action,
            target,
        }
    }

    fn load() -> RowAction<u32> {
        action(LOAD_LABEL, LOAD, ActionTarget::AnotherRow)
    }

    fn unload() -> RowAction<u32> {
        action("Unload", UNLOAD, ActionTarget::Itself)
    }

    fn acting(mut row: ListRow<u32>, actions: Vec<RowAction<u32>>) -> ListRow<u32> {
        row.actions = actions;
        row
    }

    fn magazine(actions: Vec<RowAction<u32>>) -> ListRow<u32> {
        acting(row("Magazine", Some(2), Some("Holds 30."), 10), actions)
    }

    fn bandage() -> ListRow<u32> {
        row("Bandage", Some(3), None, 11)
    }

    /// The rig, list 7: the magazine with both actions, then a bandage with none.
    fn rig(rows: Vec<ListRow<u32>>) -> ItemList<u32> {
        list(7, "Rig", rows)
    }

    /// The backpack, list 8: rounds with no actions, then a flask that can be unloaded.
    fn backpack() -> ItemList<u32> {
        list(
            8,
            "Backpack",
            vec![
                row("Rounds", Some(60), None, 20),
                acting(row("Flask", Some(1), Some("Water."), 21), vec![unload()]),
            ],
        )
    }

    fn rig_and_backpack() -> Vec<ItemList<u32>> {
        vec![
            rig(vec![magazine(vec![load(), unload()]), bandage()]),
            backpack(),
        ]
    }

    fn rig_with(rows: Vec<ListRow<u32>>) -> ItemLists<u32> {
        lists_of(vec![rig(rows), backpack()])
    }

    fn rb_stack() -> MenuStack<u32> {
        stack_of(rig_and_backpack())
    }

    fn action_picked(
        action: u32,
        item: u32,
        list: u32,
        target: Option<(u32, u32)>,
    ) -> MenuEvent<u32> {
        MenuEvent::ActionPicked {
            action,
            item,
            list: ListId(list),
            target: target.map(|(item, list)| RowRef {
                item,
                list: ListId(list),
            }),
        }
    }

    /// What of a text the shell decides: its anchor, offset, font size and colour.
    fn shape(texts: &[ScreenText], text: &str) -> (Anchor, [f32; 2], f32, [u8; 4]) {
        let t = find(texts, text);
        (t.anchor, t.offset, t.size, t.color)
    }

    /// The left of the first list's action list is 16 points off the list: 288 - 16 - 1280.
    const LEFT_OF_FIRST: f32 = -1008.0;
    /// The second of two lists starts at 672 and is 320 wide: 672 + 320 + 16.
    const RIGHT_OF_SECOND: f32 = 1008.0;

    #[test]
    fn space_or_a_right_click_shows_the_rows_actions_in_the_tooltips_place() {
        let mut by_space = rb_stack();
        let closed = by_space.texts(SIZE);
        press(&mut by_space, Key::Space);
        let by_space_texts = by_space.texts(SIZE);
        let mut by_click = rb_stack();
        by_click.right_click(over(0, 2, 0), SIZE);
        let by_click_texts = by_click.texts(SIZE);
        let mut on_the_flask = rb_stack();
        on_the_flask.right_click(over(1, 2, 1), SIZE);
        let flask = on_the_flask.texts(SIZE);

        assert!(!is_shown(&closed, LOAD_LABEL));
        assert!(
            is_shown(&closed, "Holds 30."),
            "the tooltip shows while closed"
        );
        for texts in [&by_space_texts, &by_click_texts] {
            assert_eq!(
                shape(texts, LOAD_LABEL),
                (
                    Anchor::Right,
                    [LEFT_OF_FIRST, row_y(0) - 360.0],
                    18.0,
                    FOCUS
                )
            );
            assert_eq!(
                shape(texts, "Unload"),
                (
                    Anchor::Right,
                    [LEFT_OF_FIRST, row_y(1) - 360.0],
                    18.0,
                    COLOR
                )
            );
        }
        assert_eq!(
            shape(&flask, "Unload"),
            (
                Anchor::Left,
                [RIGHT_OF_SECOND, row_y(1) - 360.0],
                18.0,
                FOCUS
            )
        );
        assert!(
            focused(&flask).contains(&"Flask"),
            "the right click focused the row"
        );
        assert_eq!(by_space.take_events(), []);
        assert_eq!(by_click.take_events(), []);
    }

    #[test]
    fn a_row_with_no_actions_or_an_empty_list_opens_nothing() {
        let mut stack = stack_of(vec![
            rig(vec![magazine(vec![load(), unload()]), bandage()]),
            list(9, "Ground", Vec::new()),
        ]);
        press(&mut stack, Key::ArrowDown);
        let on_bandage = stack.texts(SIZE);

        press(&mut stack, Key::Space);
        let after_space = stack.texts(SIZE);
        stack.right_click(over(0, 2, 1), SIZE);
        let after_right_click = stack.texts(SIZE);
        stack.right_click([5.0, 5.0], SIZE);
        stack.right_click(over(0, 2, 7), SIZE);
        let after_no_row = stack.texts(SIZE);
        press(&mut stack, Key::ArrowRight);
        let on_empty = stack.texts(SIZE);
        press(&mut stack, Key::Space);
        stack.right_click(over(1, 2, 0), SIZE);
        let after_empty = stack.texts(SIZE);
        let silent = stack.take_events();
        press(&mut stack, Key::ArrowLeft);
        press(&mut stack, Key::ArrowUp);
        press(&mut stack, Key::Space);
        let on_magazine = stack.texts(SIZE);

        assert_eq!(after_space, on_bandage);
        assert_eq!(after_right_click, on_bandage);
        assert_eq!(after_no_row, on_bandage);
        assert_eq!(after_empty, on_empty);
        assert_eq!(silent, []);
        assert!(
            is_shown(&on_magazine, LOAD_LABEL),
            "the magazine row does open its actions"
        );
    }

    #[test]
    fn picking_an_itself_action_by_key_or_click_sends_action_row_and_list_and_changes_no_rows() {
        let data = lists_of(rig_and_backpack());
        let mut by_key = MenuStack::new();
        by_key.push_lists(data.clone());
        press(&mut by_key, Key::Space);
        press(&mut by_key, Key::ArrowDown);
        let on_unload = find(&by_key.texts(SIZE), "Unload").color;
        by_key.on_key(Input::Key(Key::Enter), true, true);
        let after_repeat = (
            by_key.take_events(),
            is_shown(&by_key.texts(SIZE), "Unload"),
        );
        press(&mut by_key, Key::Enter);
        let by_key_texts = by_key.texts(SIZE);
        let by_key_events = by_key.take_events();
        let mut by_click = MenuStack::new();
        by_click.push_lists(data.clone());
        by_click.right_click(over(0, 2, 0), SIZE);
        by_click.click([172.0, row_y(1)], SIZE);
        let by_click_texts = by_click.texts(SIZE);
        let by_click_events = by_click.take_events();
        let mut on_the_flask = MenuStack::new();
        on_the_flask.push_lists(data.clone());
        on_the_flask.right_click(over(1, 2, 1), SIZE);
        on_the_flask.click([1108.0, row_y(1)], SIZE);
        let flask_events = on_the_flask.take_events();

        assert_eq!(on_unload, FOCUS);
        assert_eq!(after_repeat, (vec![], true), "a repeat picks nothing");
        assert_eq!(by_key_events, [action_picked(UNLOAD, 10, 7, None)]);
        assert_eq!(by_click_events, [action_picked(UNLOAD, 10, 7, None)]);
        assert_eq!(flask_events, [action_picked(UNLOAD, 21, 8, None)]);
        for texts in [&by_key_texts, &by_click_texts] {
            assert!(!is_shown(texts, "Unload"), "picking closes the list");
            assert!(!is_shown(texts, LOAD_LABEL));
        }
        assert_eq!(marked(&by_key), Vec::<String>::new());
        assert_eq!(by_key.top_lists(), Some(&data), "the shell moves no row");
        assert_eq!(by_click.top_lists(), Some(&data));
    }

    #[test]
    fn an_another_row_action_then_a_row_in_another_list_sends_both_rows_and_lists() {
        let data = lists_of(rig_and_backpack());
        let mut by_key = MenuStack::new();
        by_key.push_lists(data.clone());
        press(&mut by_key, Key::Space);
        press(&mut by_key, Key::Enter);
        let started = (
            by_key.take_events(),
            marked(&by_key),
            is_shown(&by_key.texts(SIZE), "Unload"),
        );
        by_key.on_key(Input::Key(Key::Enter), true, true);
        let after_repeat = (by_key.take_events(), marked(&by_key));
        press(&mut by_key, Key::ArrowRight);
        press(&mut by_key, Key::Enter);
        let by_key_done = (by_key.take_events(), marked(&by_key));
        let mut by_click = MenuStack::new();
        by_click.push_lists(data.clone());
        by_click.right_click(over(0, 2, 0), SIZE);
        by_click.click([172.0, row_y(0)], SIZE);
        let click_started = (by_click.take_events(), marked(&by_click));
        by_click.click(over(1, 2, 0), SIZE);
        let by_click_done = (by_click.take_events(), marked(&by_click));

        let picking = vec![PICKING_LOAD_FROM_MAGAZINE.to_owned()];
        assert_eq!(started, (vec![], picking.clone(), false));
        assert_eq!(after_repeat, (vec![], picking.clone()));
        assert_eq!(click_started, (vec![], picking));
        let expected = vec![action_picked(LOAD, 10, 7, Some((20, 8)))];
        assert_eq!(by_key_done, (expected.clone(), vec![]));
        assert_eq!(by_click_done, (expected, vec![]));
        assert_eq!(by_key.top_lists(), Some(&data), "the shell moves no row");
    }

    #[test]
    fn escape_or_right_click_during_a_target_pick_sends_nothing_and_focuses_the_source_row() {
        let cancelled_by = |cancel: &dyn Fn(&mut MenuStack<u32>)| {
            let mut stack = MenuStack::new();
            stack.push_lists(with_back(rig_and_backpack(), 99));
            press(&mut stack, Key::Space);
            press(&mut stack, Key::Enter);
            press(&mut stack, Key::ArrowRight);
            press(&mut stack, Key::ArrowDown);
            let picking = marked(&stack);
            cancel(&mut stack);
            (
                picking,
                marked(&stack),
                stack.take_events(),
                focused_labels(&stack),
            )
        };

        let by_escape = cancelled_by(&|stack| press(stack, Key::Escape));
        let by_right_click = cancelled_by(&|stack| stack.right_click(over(1, 2, 1), SIZE));

        let expected = (
            vec![PICKING_LOAD_FROM_MAGAZINE.to_owned()],
            Vec::<String>::new(),
            vec![],
            "Magazine,2".to_owned(),
        );
        assert_eq!(by_escape, expected);
        assert_eq!(by_right_click, expected);
    }

    /// A row's label, count and tooltip as the property test varies them.
    type Spec = (String, Option<u32>, Option<String>);

    fn specs() -> impl Strategy<Value = Vec<Spec>> {
        prop::collection::vec(
            (
                "[a-z0-9 ]{0,10}",
                prop::option::of(any::<u32>()),
                prop::option::of("[a-z0-9 ]{0,10}"),
            ),
            160,
        )
    }

    /// Lists `0..` of `sizes` rows, every row offering "Act1" (itself) and "Act2" (another row),
    /// whose label, count and tooltip are read from `specs` by position. Items are
    /// `list * 100 + row`.
    fn acting_lists(sizes: &[usize], specs: &[Spec]) -> ItemLists<u32> {
        lists_of(
            sizes
                .iter()
                .enumerate()
                .map(|(at, &rows)| {
                    list(
                        at as u32,
                        &format!("T{at}"),
                        (0..rows)
                            .map(|index| {
                                let (label, count, tip) = &specs[at * 40 + index];
                                let tip = tip.as_ref().map(|tip| format!("t{tip}"));
                                acting(
                                    row(
                                        &format!("r{label}"),
                                        *count,
                                        tip.as_deref(),
                                        (at * 100 + index) as u32,
                                    ),
                                    vec![
                                        action("Act1", 1, ActionTarget::Itself),
                                        action("Act2", 2, ActionTarget::AnotherRow),
                                    ],
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        )
    }

    proptest! {
        #[test]
        fn set_lists_with_changed_counts_keeps_an_open_action_list_or_target_pick_and_its_focus(
            sizes in prop::collection::vec(0usize..=40, 1..=4),
            before in specs(),
            after in specs(),
            source in (0usize..4, 0usize..40),
            picking in any::<bool>(),
            moved in (0usize..4, 0usize..45),
        ) {
            let lists = sizes.len();
            let source_list = source.0 % lists;
            prop_assume!(sizes[source_list] > 0);
            let source_row = source.1 % sizes[source_list];
            let source_item = (source_list * 100 + source_row) as u32;
            let mut stack = MenuStack::new();
            stack.push_lists(acting_lists(&sizes, &before));
            presses(&mut stack, Key::ArrowRight, source_list);
            presses(&mut stack, Key::ArrowDown, source_row);
            press(&mut stack, Key::Space);
            press(&mut stack, Key::ArrowDown);
            let mut pick_focus = None;
            if picking {
                press(&mut stack, Key::Enter);
                let to = (source_list + moved.0) % lists;
                presses(&mut stack, Key::ArrowRight, moved.0);
                presses(&mut stack, Key::ArrowDown, moved.1);
                let rows = sizes[to];
                let was = if to == source_list { source_row } else { 0 };
                pick_focus = (rows > 0).then(|| (to, (was + moved.1) % rows));
            }
            let settled = stack.texts(SIZE);
            if !picking {
                prop_assert!(is_shown(&settled, "Act2"), "the action list is open");
                prop_assert_eq!(find(&settled, "Act2").color, FOCUS);
            }
            prop_assert_eq!(stack.take_events(), vec![]);

            stack.set_lists(acting_lists(&sizes, &after));

            prop_assert_eq!(stack.take_events(), vec![]);
            if picking {
                press(&mut stack, Key::Enter);
                let events = stack.take_events();
                match pick_focus {
                    Some((list_at, row_at)) if (list_at, row_at) != (source_list, source_row) => {
                        prop_assert_eq!(
                            events,
                            vec![action_picked(
                                2,
                                source_item,
                                source_list as u32,
                                Some(((list_at * 100 + row_at) as u32, list_at as u32)),
                            )]
                        );
                    }
                    Some(_) => prop_assert_eq!(events, vec![]),
                    None => {
                        prop_assert_eq!(events, vec![]);
                        press(&mut stack, Key::Escape);
                        let label = format!("r{}", after[source_list * 40 + source_row].0);
                        prop_assert!(
                            focused(&stack.texts(SIZE)).contains(&label.as_str()),
                            "Escape focuses the source row, so the pick was still on"
                        );
                    }
                }
            } else {
                let texts = stack.texts(SIZE);
                prop_assert!(is_shown(&texts, "Act1") && is_shown(&texts, "Act2"));
                prop_assert_eq!(find(&texts, "Act2").color, FOCUS);
                press(&mut stack, Key::Enter);
                prop_assert_eq!(stack.take_events(), vec![]);
                let label = format!("r{}", after[source_list * 40 + source_row].0);
                prop_assert_eq!(marked(&stack), vec![format!("» Act2: {label}")]);
            }
        }
    }

    #[test]
    fn set_lists_without_the_source_row_or_its_action_ends_the_pick_and_sends_nothing() {
        let picking = || {
            let mut stack = rb_stack();
            press(&mut stack, Key::Space);
            press(&mut stack, Key::Enter);
            stack
        };
        let cases = [
            ("the list is gone", lists_of(vec![backpack()])),
            ("the row is gone", rig_with(vec![bandage()])),
            (
                "the action is gone",
                rig_with(vec![magazine(vec![unload()]), bandage()]),
            ),
            (
                "the row has no actions",
                rig_with(vec![magazine(vec![]), bandage()]),
            ),
        ];

        let mut kept = picking();
        kept.set_lists(rig_with(vec![magazine(vec![load(), unload()]), bandage()]));
        let kept_marks = marked(&kept);
        for (case, data) in cases {
            let mut stack = picking();
            let before = marked(&stack);
            stack.set_lists(data);
            let after = (marked(&stack), stack.take_events());
            press(&mut stack, Key::Enter);
            press(&mut stack, Key::Enter);
            let later = stack.take_events();

            assert_eq!(before, [PICKING_LOAD_FROM_MAGAZINE], "{case}");
            assert_eq!(after, (vec![], vec![]), "{case}");
            assert_eq!(later, [], "{case}: no later key sends the action");
        }
        assert_eq!(kept_marks, [PICKING_LOAD_FROM_MAGAZINE]);
    }

    #[test]
    fn while_actions_are_open_up_down_wrap_and_left_right_wheel_and_escape_back_do_nothing() {
        let mut stack = MenuStack::new();
        stack.push_lists(with_back(rig_and_backpack(), 99));
        let on_action = |stack: &MenuStack<u32>| -> String {
            let texts = stack.texts(SIZE);
            [LOAD_LABEL, "Unload"]
                .into_iter()
                .filter(|label| find(&texts, label).color == FOCUS)
                .collect::<Vec<_>>()
                .join(",")
        };
        press(&mut stack, Key::Space);
        let mut seen = vec![on_action(&stack)];

        for key in [Key::ArrowUp, Key::ArrowDown, Key::ArrowDown, Key::ArrowDown] {
            press(&mut stack, key);
            seen.push(on_action(&stack));
        }
        press(&mut stack, Key::ArrowLeft);
        press(&mut stack, Key::ArrowRight);
        stack.scroll(5.0);
        let after_inert_keys = (
            on_action(&stack),
            focused(&stack.texts(SIZE)).contains(&"Magazine"),
        );
        let silent = stack.take_events();
        press(&mut stack, Key::Escape);
        let after_escape = (
            is_shown(&stack.texts(SIZE), LOAD_LABEL),
            stack.take_events(),
        );
        let mut long = MenuStack::new();
        long.push_lists(lists_of(vec![list(
            1,
            "A",
            (0..40)
                .map(|i| acting(row(&format!("r{i:02}"), None, None, i), vec![unload()]))
                .collect(),
        )]));
        press(&mut long, Key::Space);
        long.scroll(5.0);
        let after_wheel = long.texts(SIZE);

        assert_eq!(
            seen,
            [LOAD_LABEL, "Unload", LOAD_LABEL, "Unload", LOAD_LABEL]
        );
        assert_eq!(after_inert_keys, (LOAD_LABEL.to_owned(), true));
        assert_eq!(silent, []);
        assert_eq!(
            after_escape,
            (false, vec![]),
            "Escape closes and is not back"
        );
        assert_eq!(
            y_of(find(&after_wheel, "r00")),
            row_y(0),
            "the rows did not scroll"
        );
        assert!(is_shown(&after_wheel, "Unload"));
    }

    #[test]
    fn escape_space_right_click_or_a_click_outside_closes_the_actions_and_sends_nothing() {
        let closed_by = |close: &dyn Fn(&mut MenuStack<u32>)| {
            let mut stack = MenuStack::new();
            stack.push_lists(with_back(rig_and_backpack(), 99));
            press(&mut stack, Key::Space);
            let was_open = is_shown(&stack.texts(SIZE), LOAD_LABEL);
            close(&mut stack);
            let texts = stack.texts(SIZE);
            (
                was_open,
                is_shown(&texts, LOAD_LABEL) || is_shown(&texts, "Unload"),
                stack.take_events(),
                marked(&stack),
                focused_labels(&stack),
            )
        };
        let expected = (
            true,
            false,
            vec![],
            Vec::<String>::new(),
            "Magazine,2".to_owned(),
        );

        let by_escape = closed_by(&|stack| press(stack, Key::Escape));
        let by_space = closed_by(&|stack| press(stack, Key::Space));
        let by_right_click = closed_by(&|stack| stack.right_click(over(0, 2, 0), SIZE));
        let by_right_click_elsewhere = closed_by(&|stack| stack.right_click(over(1, 2, 1), SIZE));
        let by_click_on_a_row = closed_by(&|stack| stack.click(over(1, 2, 0), SIZE));
        let by_click_on_nothing = closed_by(&|stack| stack.click([5.0, 5.0], SIZE));

        assert_eq!(by_escape, expected);
        assert_eq!(by_space, expected);
        assert_eq!(by_right_click, expected);
        assert_eq!(by_right_click_elsewhere, expected);
        assert_eq!(by_click_on_a_row, expected);
        assert_eq!(by_click_on_nothing, expected);
    }

    #[test]
    fn set_lists_keeps_the_focused_action_by_value_and_closes_when_the_row_loses_its_actions() {
        let colour_of = |stack: &MenuStack<u32>, label: &str| find(&stack.texts(SIZE), label).color;
        let mut stack = rb_stack();
        press(&mut stack, Key::Space);
        press(&mut stack, Key::ArrowDown);

        stack.set_lists(rig_with(vec![magazine(vec![unload(), load()]), bandage()]));
        let reordered = (
            colour_of(&stack, "Unload"),
            colour_of(&stack, LOAD_LABEL),
            find(&stack.texts(SIZE), "Unload").offset[1],
        );
        stack.set_lists(rig_with(vec![magazine(vec![load()]), bandage()]));
        let clamped = (
            colour_of(&stack, LOAD_LABEL),
            is_shown(&stack.texts(SIZE), "Unload"),
        );
        stack.set_lists(rig_with(vec![magazine(vec![]), bandage()]));
        let lost_actions = is_shown(&stack.texts(SIZE), LOAD_LABEL);
        stack.set_lists(rig_with(vec![magazine(vec![load(), unload()]), bandage()]));
        let not_reopened = is_shown(&stack.texts(SIZE), LOAD_LABEL);
        let mut closed_by_the_data = Vec::new();
        for data in [lists_of(vec![backpack()]), rig_with(vec![bandage()])] {
            let mut other = rb_stack();
            press(&mut other, Key::Space);
            other.set_lists(data);
            closed_by_the_data.push((
                is_shown(&other.texts(SIZE), LOAD_LABEL),
                other.take_events(),
            ));
        }

        assert_eq!(reordered, (FOCUS, COLOR, row_y(0) - 360.0));
        assert_eq!(clamped, (FOCUS, false));
        assert!(!lost_actions);
        assert!(!not_reopened);
        assert_eq!(closed_by_the_data, [(false, vec![]), (false, vec![])]);
        assert_eq!(stack.take_events(), []);
    }

    #[test]
    fn a_target_in_the_source_rows_own_list_is_valid() {
        let data = lists_of(rig_and_backpack());
        let mut by_key = MenuStack::new();
        by_key.push_lists(data.clone());
        press(&mut by_key, Key::Space);
        press(&mut by_key, Key::Enter);
        press(&mut by_key, Key::ArrowDown);
        press(&mut by_key, Key::Enter);
        let mut by_click = MenuStack::new();
        by_click.push_lists(data);
        by_click.right_click(over(0, 2, 0), SIZE);
        by_click.click([172.0, row_y(0)], SIZE);
        by_click.click(over(0, 2, 1), SIZE);

        let expected = vec![action_picked(LOAD, 10, 7, Some((11, 7)))];
        assert_eq!(by_key.take_events(), expected);
        assert_eq!(by_click.take_events(), expected);
        assert_eq!(marked(&by_key), Vec::<String>::new());
    }

    #[test]
    fn picking_the_source_row_itself_releases_and_sends_nothing() {
        let released_by = |release: &dyn Fn(&mut MenuStack<u32>)| {
            let mut stack = rb_stack();
            press(&mut stack, Key::Space);
            press(&mut stack, Key::Enter);
            let was = marked(&stack);
            release(&mut stack);
            (
                was,
                marked(&stack),
                stack.take_events(),
                is_shown(&stack.texts(SIZE), LOAD_LABEL),
            )
        };
        let expected = (
            vec![PICKING_LOAD_FROM_MAGAZINE.to_owned()],
            Vec::<String>::new(),
            vec![],
            false,
        );

        let by_enter = released_by(&|stack| press(stack, Key::Enter));
        let by_click = released_by(&|stack| stack.click(over(0, 2, 0), SIZE));

        assert_eq!(by_enter, expected);
        assert_eq!(by_click, expected);
    }

    #[test]
    fn enter_on_an_empty_list_or_a_click_on_no_row_keeps_picking() {
        let mut stack = stack_of(vec![
            rig(vec![magazine(vec![load()]), bandage()]),
            list(9, "Ground", Vec::new()),
        ]);
        press(&mut stack, Key::Space);
        press(&mut stack, Key::Enter);
        let started = marked(&stack);
        press(&mut stack, Key::ArrowRight);
        press(&mut stack, Key::Enter);
        let on_empty_list = (marked(&stack), stack.take_events());
        stack.click(over(1, 2, 0), SIZE);
        stack.click([5.0, 5.0], SIZE);
        stack.click(over(0, 2, 7), SIZE);
        let on_no_row = (marked(&stack), stack.take_events());
        stack.click(over(0, 2, 1), SIZE);
        let placed = (marked(&stack), stack.take_events());

        let picking = vec![PICKING_LOAD_FROM_MAGAZINE.to_owned()];
        assert_eq!(started, picking);
        assert_eq!(on_empty_list, (picking.clone(), vec![]));
        assert_eq!(on_no_row, (picking, vec![]));
        assert_eq!(
            placed,
            (vec![], vec![action_picked(LOAD, 10, 7, Some((11, 7)))])
        );
    }

    #[test]
    fn with_a_move_hold_space_does_nothing_and_a_right_click_releases() {
        let mut stack = rb_stack();
        press(&mut stack, Key::Enter);
        let held = marked(&stack);

        press(&mut stack, Key::Space);
        let after_space = (
            marked(&stack),
            is_shown(&stack.texts(SIZE), LOAD_LABEL),
            stack.take_events(),
        );
        stack.right_click(over(0, 2, 0), SIZE);
        let after_right_click = (
            marked(&stack),
            is_shown(&stack.texts(SIZE), LOAD_LABEL),
            stack.take_events(),
        );
        press(&mut stack, Key::Space);
        let reopened = is_shown(&stack.texts(SIZE), LOAD_LABEL);

        assert_eq!(held, ["» Magazine"]);
        assert_eq!(after_space, (vec!["» Magazine".to_owned()], false, vec![]));
        assert_eq!(after_right_click, (vec![], false, vec![]));
        assert!(reopened, "with nothing held, Space opens the actions");
    }

    #[test]
    fn the_held_row_shows_the_action_label_only_during_a_target_pick() {
        let mut stack = rb_stack();

        press(&mut stack, Key::Enter);
        let moving = marked(&stack);
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::Space);
        press(&mut stack, Key::Enter);
        let picking = marked(&stack);
        let texts = stack.texts(SIZE);

        assert_eq!(moving, ["» Magazine"]);
        assert_eq!(picking, [PICKING_LOAD_FROM_MAGAZINE]);
        let held = find(&texts, PICKING_LOAD_FROM_MAGAZINE);
        assert_eq!(held.anchor, Anchor::Left, "still a left-aligned label");
        assert_eq!(
            held.color, FOCUS,
            "the focused held row keeps the focus colour"
        );
        assert!(is_shown(&texts, "2"), "the count is still drawn");
    }

    #[test]
    fn actions_draw_left_of_the_first_list_and_right_of_others_and_hide_the_tooltip() {
        let three = || {
            stack_of(
                [("a", 1), ("b", 2), ("c", 3)]
                    .map(|(name, item)| {
                        list(
                            item,
                            name,
                            vec![acting(
                                row(name, None, Some(&format!("tip {name}")), item),
                                vec![unload()],
                            )],
                        )
                    })
                    .to_vec(),
            )
        };
        let places = [
            (Anchor::Right, -1200.0, "tip a"),
            (Anchor::Left, 816.0, "tip b"),
            (Anchor::Left, 1200.0, "tip c"),
        ];

        for (at, (anchor, x, tip)) in places.into_iter().enumerate() {
            let mut stack = three();
            presses(&mut stack, Key::ArrowRight, at);
            let closed = stack.texts(SIZE);
            press(&mut stack, Key::Space);
            let open = stack.texts(SIZE);
            press(&mut stack, Key::Escape);
            let again = stack.texts(SIZE);

            assert!(is_shown(&closed, tip), "{tip} shows while closed");
            assert_eq!(
                shape(&open, "Unload"),
                (anchor, [x, row_y(0) - 360.0], 18.0, FOCUS),
                "list {at}"
            );
            assert!(!is_shown(&open, tip), "{tip} is hidden while open");
            assert!(is_shown(&again, tip), "{tip} returns when closed");
        }
    }

    #[test]
    fn an_action_list_near_the_bottom_moves_up_inside_the_band() {
        let mut stack = stack_of(vec![list(
            1,
            "A",
            (0..20)
                .map(|i| {
                    acting(
                        row(&format!("r{i:02}"), None, None, i),
                        vec![load(), unload()],
                    )
                })
                .collect(),
        )]);
        // A single list starts at 480: 480 - 16 - 1280.
        let left = -816.0;
        presses(&mut stack, Key::ArrowDown, 16);
        press(&mut stack, Key::Space);
        let fits = stack.texts(SIZE);
        press(&mut stack, Key::Escape);
        presses(&mut stack, Key::ArrowDown, 2);
        press(&mut stack, Key::Space);
        let too_low = stack.texts(SIZE);

        assert_eq!(
            shape(&fits, LOAD_LABEL),
            (Anchor::Right, [left, 584.0 - 360.0], 18.0, FOCUS)
        );
        assert_eq!(y_of(find(&fits, "Unload")), 616.0);
        assert_eq!(
            shape(&too_low, LOAD_LABEL),
            (Anchor::Right, [left, 616.0 - 360.0], 18.0, FOCUS)
        );
        assert_eq!(
            y_of(find(&too_low, "Unload")),
            648.0,
            "the last action ends at the band's bottom, 664"
        );
        assert_eq!(y_of(find(&too_low, "r18")), row_y(18), "the rows stay put");
    }

    #[test]
    fn hover_focuses_an_action_and_elsewhere_changes_nothing_while_open() {
        let mut stack = rb_stack();
        press(&mut stack, Key::Space);
        let colours = |stack: &MenuStack<u32>| {
            let texts = stack.texts(SIZE);
            (
                find(&texts, LOAD_LABEL).color,
                find(&texts, "Unload").color,
                focused(&texts).contains(&"Magazine"),
            )
        };

        stack.hover([172.0, row_y(1)], SIZE);
        let on_unload = colours(&stack);
        stack.hover(over(1, 2, 1), SIZE);
        stack.hover([5.0, 5.0], SIZE);
        stack.hover(over(0, 2, 7), SIZE);
        let elsewhere = colours(&stack);
        stack.hover([172.0, row_y(0)], SIZE);
        let on_load = colours(&stack);
        stack.hover([172.0, row_y(1)], SIZE);
        press(&mut stack, Key::Enter);

        assert_eq!(on_unload, (COLOR, FOCUS, true));
        assert_eq!(elsewhere, (COLOR, FOCUS, true));
        assert_eq!(on_load, (FOCUS, COLOR, true));
        assert_eq!(stack.take_events(), [action_picked(UNLOAD, 10, 7, None)]);
    }

    fn new_row() -> ListRow<u32> {
        row("New", None, None, 12)
    }

    #[test]
    fn a_row_inserted_before_the_magazine_keeps_its_actions_open_on_the_magazine() {
        let mut stack = rb_stack();
        press(&mut stack, Key::Space);

        stack.set_lists(rig_with(vec![
            new_row(),
            magazine(vec![load(), unload()]),
            bandage(),
        ]));
        let texts = stack.texts(SIZE);
        press(&mut stack, Key::ArrowDown);
        press(&mut stack, Key::Enter);

        assert_eq!(
            shape(&texts, LOAD_LABEL),
            (
                Anchor::Right,
                [LEFT_OF_FIRST, row_y(1) - 360.0],
                18.0,
                FOCUS
            ),
            "the actions follow the magazine to its new row"
        );
        assert_eq!(stack.take_events(), [action_picked(UNLOAD, 10, 7, None)]);
    }

    #[test]
    fn a_row_inserted_before_the_source_row_keeps_the_target_pick_on_the_magazine() {
        let inserted = || rig_with(vec![new_row(), magazine(vec![load(), unload()]), bandage()]);
        let mut cancelled = rb_stack();
        press(&mut cancelled, Key::Space);
        press(&mut cancelled, Key::Enter);
        cancelled.set_lists(inserted());
        let still_picking = marked(&cancelled);
        press(&mut cancelled, Key::Escape);
        let after_escape = (focused_labels(&cancelled), cancelled.take_events());
        let mut placed = rb_stack();
        press(&mut placed, Key::Space);
        press(&mut placed, Key::Enter);
        placed.set_lists(inserted());
        press(&mut placed, Key::ArrowRight);
        press(&mut placed, Key::Enter);

        assert_eq!(still_picking, [PICKING_LOAD_FROM_MAGAZINE]);
        assert_eq!(after_escape, ("Magazine,2".to_owned(), vec![]));
        assert_eq!(
            placed.take_events(),
            [action_picked(LOAD, 10, 7, Some((20, 8)))]
        );
    }

    #[test]
    fn a_right_click_over_another_list_during_a_target_pick_focuses_the_source_row() {
        let mut stack = rb_stack();
        press(&mut stack, Key::Space);
        press(&mut stack, Key::Enter);
        let picking = marked(&stack);

        stack.right_click(over(1, 2, 1), SIZE);

        assert_eq!(picking, [PICKING_LOAD_FROM_MAGAZINE]);
        assert_eq!(stack.take_events(), []);
        assert_eq!(marked(&stack), Vec::<String>::new());
        assert_eq!(focused_labels(&stack), "Magazine,2");
        assert!(!is_shown(&stack.texts(SIZE), LOAD_LABEL), "nothing opened");
    }

    #[test]
    fn repeats_do_not_open_pick_or_place() {
        let repeat = |stack: &mut MenuStack<u32>, key| stack.on_key(Input::Key(key), true, true);
        let mut stack = rb_stack();

        repeat(&mut stack, Key::Space);
        let repeat_space = is_shown(&stack.texts(SIZE), LOAD_LABEL);
        press(&mut stack, Key::Space);
        repeat(&mut stack, Key::Enter);
        let open_after_repeat = (
            is_shown(&stack.texts(SIZE), LOAD_LABEL),
            marked(&stack),
            stack.take_events(),
        );
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowRight);
        repeat(&mut stack, Key::Enter);
        let picking_after_repeat = (marked(&stack), stack.take_events());
        press(&mut stack, Key::Enter);

        assert!(!repeat_space);
        assert_eq!(open_after_repeat, (true, vec![], vec![]));
        assert_eq!(
            picking_after_repeat,
            (vec![PICKING_LOAD_FROM_MAGAZINE.to_owned()], vec![])
        );
        assert_eq!(
            stack.take_events(),
            [action_picked(LOAD, 10, 7, Some((20, 8)))],
            "a real Enter still places"
        );
    }

    #[test]
    fn a_right_click_on_a_row_without_actions_focuses_it_and_on_empty_space_changes_nothing() {
        let mut stack = rb_stack();

        stack.right_click(over(0, 2, 1), SIZE);
        let on_bandage = (focused_labels(&stack), stack.texts(SIZE));
        stack.right_click(over(1, 2, 7), SIZE);
        let on_space = (focused_labels(&stack), stack.texts(SIZE));
        stack.right_click(over(0, 2, 0), SIZE);
        let on_magazine = is_shown(&stack.texts(SIZE), LOAD_LABEL);

        assert_eq!(on_bandage.0, "Bandage,3");
        assert!(!is_shown(&on_bandage.1, LOAD_LABEL));
        assert!(!is_shown(&on_bandage.1, "Unload"));
        assert_eq!(on_space, on_bandage);
        assert!(on_magazine, "a row with actions still opens them");
        assert_eq!(stack.take_events(), []);
    }

    #[test]
    fn actions_on_a_row_in_a_scrolled_list_lie_inside_the_band() {
        let mut stack = stack_of(vec![list(
            1,
            "A",
            (0..40)
                .map(|i| {
                    acting(
                        row(&format!("r{i:02}"), None, None, i),
                        vec![load(), unload()],
                    )
                })
                .collect(),
        )]);
        stack.scroll(30.0);
        let closed = stack.visible_rows(SIZE);

        let mut extras = Vec::new();
        for visible_row in [0, 9, 18] {
            stack.hover(over(0, 1, visible_row), SIZE);
            press(&mut stack, Key::Space);
            let open = stack.visible_rows(SIZE);
            press(&mut stack, Key::Escape);
            let added: Vec<Rect> = open
                .into_iter()
                .filter(|rect| !closed.contains(rect))
                .collect();
            extras.push((visible_row, added));
        }

        for (visible_row, added) in extras {
            assert_eq!(added.len(), 2, "row {visible_row}");
            for rect in added {
                assert!(rect.min[1] >= 56.0, "row {visible_row}: {rect:?}");
                assert!(rect.max[1] <= SIZE[1] - 56.0, "row {visible_row}: {rect:?}");
            }
        }
    }
}
