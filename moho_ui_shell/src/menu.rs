//! Menu screens a game describes as data: labelled buttons the engine draws and drives with the
//! mouse and keyboard. Screens stack, each keeping its own focus. The stack never changes itself:
//! a choice becomes a [`MenuEvent`] the game answers by pushing or popping.

use std::cell::Cell;
use std::collections::BTreeMap;

use moho_input::bindings::{ActionMode, Binding, Input, Modifiers, Trigger};
use moho_input::key::{Key, MouseButton};

use crate::lists::{ItemLists, ListId, OpenLists, RowRef};
use crate::settings::{Control, EngineSetting, EngineSettings, GameSlider};
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
        /// The details chevron; `None` draws no chevron and shows every binding.
        chevron: Option<Chevron>,
        /// The hold/toggle checkbox state; `None` draws no checkbox.
        mode: Option<ActionMode>,
    },
    /// One slot's kind and modifier pickers, shown under an open action.
    Details {
        action: A,
        slot: Slot,
        trigger: Trigger,
        modifiers: Modifiers,
    },
    /// A game's own slider row, staged and applied beside the engine's rows.
    GameSlider(GameSlider),
}

/// Whether a [`MenuItem::Control`] row's details are shown under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chevron {
    Closed,
    Open,
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
    /// The player applied the game's own slider rows: every row's value by its key.
    GameSettingsApplied(BTreeMap<String, i32>),
    /// The player pressed `input` while capturing `slot` of `action`.
    Captured { action: A, slot: Slot, input: Input },
    /// The player cleared `slot` of `action`.
    Cleared { action: A, slot: Slot },
    /// The player asked to move `item` from list `from` to list `to` of the open item lists;
    /// `from` and `to` always differ. The game applies or refuses the move and answers with
    /// [`MenuStack::set_lists`].
    MoveRequested { item: A, from: ListId, to: ListId },
    /// The player picked `action` on the row `item` of list `list`. `target` is the second row
    /// the player picked, `Some` exactly when the action needs another row. The game applies or
    /// refuses it and answers with [`MenuStack::set_lists`].
    ActionPicked {
        action: A,
        item: A,
        list: ListId,
        target: Option<RowRef<A>>,
    },
    /// The player toggled the details chevron of `action`.
    DetailsToggled { action: A },
    /// The player stepped a picker of `slot` of `action`; the slot's new kind and modifiers.
    Picked {
        action: A,
        slot: Slot,
        trigger: Trigger,
        modifiers: Modifiers,
    },
    /// The player flipped the hold/toggle checkbox of `action` to `mode`.
    ModeChosen { action: A, mode: ActionMode },
}

/// An axis-aligned rectangle in logical points from the window's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// Font size of every row and title.
pub(crate) const ROW_SIZE: f32 = 36.0;
/// Distance between row centres, in logical points; the bands are this tall and touch.
const ROW_PITCH: f32 = 56.0;
/// Width of a row's clickable band, centred on the window.
const ROW_WIDTH: f32 = 240.0;
pub(crate) const FOCUS_COLOR: [u8; 4] = TEXT_COLORS[1];
pub(crate) const COLOR: [u8; 4] = TEXT_COLORS[0];

/// Font size of a settings row's label and value, which share a 240-point band.
pub(crate) const SETTING_SIZE: f32 = 18.0;

/// Height of a [`MenuItem::Control`] row.
pub(crate) const CONTROL_HEIGHT: f32 = 32.0;
/// Width of a [`MenuItem::Control`] row: a label column, then two equal slot cells.
const CONTROL_WIDTH: f32 = 640.0;
const LABEL_WIDTH: f32 = 240.0;
const CELL_WIDTH: f32 = (CONTROL_WIDTH - LABEL_WIDTH) / 2.0;
/// Free space kept above and below the rows of a column that scrolls; the title sits in the top.
pub(crate) const BAND: f32 = ROW_PITCH;
const WARNING_COLOR: [u8; 4] = TEXT_COLORS[2];
/// Every colour a menu text can have: normal, focused, warning. The backdrop's contrast test
/// covers each, so a colour added later has to go through this list.
pub(crate) const TEXT_COLORS: [[u8; 4]; 3] = [
    [220, 220, 220, 255],
    [255, 220, 80, 255],
    [255, 120, 80, 255],
];
const PROTECTED_NOTE: &str = "That input can't be bound";
/// Window size assumed until a screen has been laid out once.
pub(crate) const DEFAULT_SIZE: [f32; 2] = [1280.0, 720.0];

/// Width of the chevron cell at the start of a row's label column.
const CHEVRON_WIDTH: f32 = 24.0;
/// Width of the hold/toggle checkbox column right of the secondary cell.
const MODE_WIDTH: f32 = 128.0;

/// A place the focus can be on a row. On a `Control` row `First` and `Second` are its slots;
/// on a `Details` row they are the kind and the modifier pickers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Chevron,
    First,
    Second,
    Mode,
}

impl Column {
    /// The slot a `Control` row's cell in this column holds.
    fn slot(self) -> Option<Slot> {
        match self {
            Self::First => Some(Slot::Primary),
            Self::Second => Some(Slot::Secondary),
            Self::Chevron | Self::Mode => None,
        }
    }
}

/// The columns `item` has, left to right; none for a row that is not a `Control` or `Details`.
fn columns<A>(item: &MenuItem<A>) -> &'static [Column] {
    use Column::{Chevron, First, Mode, Second};
    match item {
        MenuItem::Control { chevron, mode, .. } => match (chevron.is_some(), mode.is_some()) {
            (true, true) => &[Chevron, First, Second, Mode],
            (true, false) => &[Chevron, First, Second],
            (false, true) => &[First, Second, Mode],
            (false, false) => &[First, Second],
        },
        MenuItem::Details { .. } => &[First, Second],
        _ => &[],
    }
}

/// `wanted` if the row has it, else the nearest column it has; a row with no columns keeps it.
fn nearest_column<A>(item: Option<&MenuItem<A>>, wanted: Column) -> Column {
    let has = item.map_or(&[][..], columns);
    if has.is_empty() || has.contains(&wanted) {
        wanted
    } else if wanted == Column::Chevron {
        Column::First
    } else {
        Column::Second
    }
}

/// The next kind a picker steps to.
fn next_kind(kind: Trigger) -> Trigger {
    match kind {
        Trigger::Press => Trigger::Hold,
        Trigger::Hold => Trigger::Double,
        Trigger::Double => Trigger::Press,
    }
}

/// The next modifiers a picker steps to: None, Ctrl, Shift, Alt, then None; a set of two or
/// more steps to None.
fn next_modifiers(modifiers: Modifiers) -> Modifiers {
    if modifiers == Modifiers::NONE {
        Modifiers::CTRL
    } else if modifiers == Modifiers::CTRL {
        Modifiers::SHIFT
    } else if modifiers == Modifiers::SHIFT {
        Modifiers::ALT
    } else {
        Modifiers::NONE
    }
}

fn kind_text(kind: Trigger) -> &'static str {
    match kind {
        Trigger::Press => "Press",
        Trigger::Hold => "Hold",
        Trigger::Double => "Double",
    }
}

fn modifiers_text(modifiers: Modifiers) -> String {
    let names: Vec<&str> = [
        (Modifiers::CTRL, "Ctrl"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::ALT, "Alt"),
    ]
    .into_iter()
    .filter(|(modifier, _)| modifiers.contains(*modifier))
    .map(|(_, name)| name)
    .collect();
    if names.is_empty() {
        "None".to_owned()
    } else {
        names.join("+")
    }
}

/// Whether two rows are the same row of a game's screen across a `set_items`.
fn same_row<A: PartialEq>(old: &MenuItem<A>, new: &MenuItem<A>) -> bool {
    match (old, new) {
        (MenuItem::Control { action: was, .. }, MenuItem::Control { action: now, .. })
        | (MenuItem::Button { action: was, .. }, MenuItem::Button { action: now, .. }) => {
            was == now
        }
        (
            MenuItem::Details {
                action: was_action,
                slot: was_slot,
                ..
            },
            MenuItem::Details {
                action: now_action,
                slot: now_slot,
                ..
            },
        ) => was_action == now_action && was_slot == now_slot,
        (MenuItem::Setting(was), MenuItem::Setting(now)) => was == now,
        (MenuItem::GameSlider(was), MenuItem::GameSlider(now)) => was.key == now.key,
        _ => false,
    }
}

/// Where the focus goes when `items` replace the rows it was on at `focus`: the same index if
/// that row is the same one, else the nearest row that is, else the index clamped to the rows.
fn refound_focus<A: PartialEq>(
    old: Option<&MenuItem<A>>,
    focus: usize,
    items: &[MenuItem<A>],
) -> usize {
    let clamped = focus.min(items.len().saturating_sub(1));
    let Some(old) = old else {
        return clamped;
    };
    if items.get(focus).is_some_and(|item| same_row(old, item)) {
        return focus;
    }
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| same_row(old, item))
        .min_by_key(|(row, _)| row.abs_diff(focus))
        .map_or(clamped, |(row, _)| row)
}

/// A screen, the row that has the focus on it and the settings values staged on it.
#[derive(Debug)]
struct Open<A> {
    screen: MenuScreen<A>,
    focus: usize,
    /// A copy of the screen's `settings` that the rows edit until Apply.
    staged: Option<EngineSettings>,
    /// The game rows the player changed, by key; a row without an entry shows its own `value`.
    game_staged: BTreeMap<String, i32>,
    /// The column of the focused row the focus is on.
    column: Column,
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
            game_staged: BTreeMap::new(),
            column: Column::First,
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
    fn set_focus(&mut self, row: usize, column: Column) {
        let column = nearest_column(self.screen.items.get(row), column);
        if (row, column) != (self.focus, self.column) {
            self.focus = row;
            self.column = column;
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

    /// Starts capturing the focused slot when the focus is on a slot of a `Control` row.
    fn start_capture(&mut self) {
        if self.focused_is_control() && self.column.slot().is_some() {
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

    /// The column of row `row` (laid out in `rect`) that holds `x`; `None` over a label or on a
    /// row without columns.
    fn column_at(&self, row: usize, rect: Rect, x: f32) -> Option<Column> {
        let item = self.screen.items.get(row)?;
        let has = columns(item);
        let across = x - rect.min[0];
        let column = if across < CHEVRON_WIDTH && has.contains(&Column::Chevron) {
            Column::Chevron
        } else if across < LABEL_WIDTH {
            return None;
        } else if across < LABEL_WIDTH + CELL_WIDTH {
            Column::First
        } else if across < CONTROL_WIDTH {
            Column::Second
        } else {
            Column::Mode
        };
        has.contains(&column).then_some(column)
    }

    /// Left and Right: walk the focused row's columns, or step a settings row.
    fn step(&mut self, direction: i8) {
        let has = self.screen.items.get(self.focus).map(columns);
        match has {
            Some(has) if !has.is_empty() => {
                let at = has.iter().position(|c| *c == self.column).unwrap_or(0);
                let to = if direction < 0 {
                    at.saturating_sub(1)
                } else {
                    (at + 1).min(has.len() - 1)
                };
                self.set_focus(self.focus, has[to]);
            }
            _ => self.nudge(self.focus, direction),
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
                slot: self.column.slot()?,
                input,
            }),
            _ => None,
        }
    }

    /// Clears the focused slot, if the focus is on a slot of a `Control` row.
    fn clear(&self) -> Option<MenuEvent<A>> {
        match self.screen.items.get(self.focus)? {
            MenuItem::Control { action, .. } => Some(MenuEvent::Cleared {
                action: action.clone(),
                slot: self.column.slot()?,
            }),
            _ => None,
        }
    }

    /// What Enter or a click does on the focused column: the chevron toggles the details, a slot
    /// of a `Control` row starts capture, a picker steps, the checkbox reports the other mode;
    /// any other row is chosen.
    fn activate(&mut self) -> Vec<MenuEvent<A>> {
        match (self.screen.items.get(self.focus), self.column) {
            (Some(MenuItem::Control { action, .. }), Column::Chevron) => {
                vec![MenuEvent::DetailsToggled {
                    action: action.clone(),
                }]
            }
            (Some(MenuItem::Control { action, mode, .. }), Column::Mode) => mode
                .map(|mode| MenuEvent::ModeChosen {
                    action: action.clone(),
                    mode: ActionMode::from_toggle(!mode.is_toggle()),
                })
                .into_iter()
                .collect(),
            (Some(MenuItem::Control { .. }), Column::First | Column::Second) => {
                self.start_capture();
                Vec::new()
            }
            (
                Some(MenuItem::Details {
                    action,
                    slot,
                    trigger,
                    modifiers,
                }),
                column,
            ) => {
                let (trigger, modifiers) = if column == Column::First {
                    (next_kind(*trigger), *modifiers)
                } else {
                    (*trigger, next_modifiers(*modifiers))
                };
                vec![MenuEvent::Picked {
                    action: action.clone(),
                    slot: *slot,
                    trigger,
                    modifiers,
                }]
            }
            _ => self.choose(self.focus),
        }
    }
}

impl<A: Clone> Open<A> {
    /// What choosing `row` does: a button chooses its action, a settings row flips or cycles
    /// its staged value, Apply hands over the staged values and makes them the baseline.
    fn choose(&mut self, row: usize) -> Vec<MenuEvent<A>> {
        match self.screen.items.get(row) {
            Some(MenuItem::Button { action, .. }) => vec![MenuEvent::Chosen(action.clone())],
            Some(MenuItem::Setting(setting)) => {
                if let Some(staged) = &mut self.staged {
                    cycle(staged, *setting);
                }
                Vec::new()
            }
            Some(MenuItem::Apply) => self.apply(),
            Some(MenuItem::Control { .. } | MenuItem::Details { .. } | MenuItem::GameSlider(_))
            | None => Vec::new(),
        }
    }

    /// Applies the staged engine values and every game row's value, making them the baseline.
    /// The engine values come first; the game values are the whole set, one entry per key, the
    /// first row in item order winning a key shared by rows that start apart.
    fn apply(&mut self) -> Vec<MenuEvent<A>> {
        let mut events = Vec::new();
        if let Some(staged) = self.staged {
            self.screen.settings = Some(staged);
            events.push(MenuEvent::SettingsApplied(staged));
        }
        let mut games = BTreeMap::new();
        for item in &self.screen.items {
            if let MenuItem::GameSlider(slider) = item {
                games
                    .entry(slider.key.clone())
                    .or_insert_with(|| self.game_value(slider));
            }
        }
        for item in &mut self.screen.items {
            if let MenuItem::GameSlider(slider) = item {
                slider.value = games[&slider.key];
            }
        }
        self.game_staged.clear();
        if !games.is_empty() {
            events.push(MenuEvent::GameSettingsApplied(games));
        }
        events
    }
}

impl<A> Open<A> {
    /// What `slider` shows: the value staged under its key, else its own.
    fn game_value(&self, slider: &GameSlider) -> i32 {
        self.game_staged
            .get(&slider.key)
            .copied()
            .unwrap_or(slider.value)
    }

    /// The color of the cell in `column` of row `row`: focused when the focus is on it.
    fn cell_color(&self, row: usize, column: Column) -> [u8; 4] {
        if row == self.focus && column == self.column {
            FOCUS_COLOR
        } else {
            COLOR
        }
    }

    /// The texts of the `Control` row `row` in `rect`: chevron, label, two slot cells and the
    /// checkbox, each only if the row has it.
    fn control_texts(
        &self,
        row: usize,
        rect: Rect,
        color: [u8; 4],
        size: [f32; 2],
    ) -> Vec<ScreenText> {
        let Some(MenuItem::Control {
            label,
            slots,
            note,
            chevron,
            mode,
            ..
        }) = self.screen.items.get(row)
        else {
            return Vec::new();
        };
        let (y, left) = (centre_y(rect), rect.min[0]);
        let at = |text: &str, x: f32, color| left_text(text, x, y, color, size);
        let collapsed = *chevron == Some(Chevron::Closed);
        let shown_note = if row == self.focus {
            self.shell_note.as_deref()
        } else {
            None
        }
        .or_else(|| note.as_deref().filter(|_| !self.notes_hidden));
        let (label_text, label_color) = match shown_note {
            Some(note) => (note.to_owned(), WARNING_COLOR),
            None if slots.iter().all(Option::is_none) => (format!("! {label}"), color),
            None if collapsed && slots.iter().flatten().any(|b| !b.is_plain()) => {
                (format!("{label} +"), color)
            }
            None => (label.clone(), color),
        };
        let mut texts = Vec::with_capacity(5);
        let mut label_x = left;
        if let Some(chevron) = chevron {
            let glyph = match chevron {
                Chevron::Closed => ">",
                Chevron::Open => "v",
            };
            texts.push(at(glyph, left, self.cell_color(row, Column::Chevron)));
            label_x += CHEVRON_WIDTH;
        }
        texts.push(at(&label_text, label_x, label_color));
        for (column, cell_x, binding) in [
            (Column::First, LABEL_WIDTH, slots[0]),
            (Column::Second, LABEL_WIDTH + CELL_WIDTH, slots[1]),
        ] {
            let shown = match binding {
                None => "Unbound".to_owned(),
                Some(binding) if collapsed && !binding.is_plain() => "...".to_owned(),
                Some(binding) => binding.label(),
            };
            texts.push(at(&shown, left + cell_x, self.cell_color(row, column)));
        }
        if let Some(mode) = mode {
            let box_text = if mode.is_toggle() {
                "[x] Toggle"
            } else {
                "[ ] Toggle"
            };
            let color = self.cell_color(row, Column::Mode);
            texts.push(at(box_text, left + CONTROL_WIDTH, color));
        }
        texts
    }

    /// The texts of the `Details` row `row` in `rect`: the slot's name, indented, then the kind
    /// and modifier pickers.
    fn details_texts(
        &self,
        row: usize,
        rect: Rect,
        color: [u8; 4],
        size: [f32; 2],
    ) -> Vec<ScreenText> {
        let Some(MenuItem::Details {
            slot,
            trigger,
            modifiers,
            ..
        }) = self.screen.items.get(row)
        else {
            return Vec::new();
        };
        let (y, left) = (centre_y(rect), rect.min[0]);
        let name = match slot {
            Slot::Primary => "Primary",
            Slot::Secondary => "Secondary",
        };
        vec![
            left_text(name, left + CHEVRON_WIDTH, y, color, size),
            left_text(
                &format!("Kind: {}", kind_text(*trigger)),
                left + LABEL_WIDTH,
                y,
                self.cell_color(row, Column::First),
                size,
            ),
            left_text(
                &format!("Mod: {}", modifiers_text(*modifiers)),
                left + LABEL_WIDTH + CELL_WIDTH,
                y,
                self.cell_color(row, Column::Second),
                size,
            ),
        ]
    }

    /// Moves the staged value of the settings row `row` one step towards `direction`.
    fn nudge(&mut self, row: usize, direction: i8) {
        if let Some(MenuItem::GameSlider(slider)) = self.screen.items.get(row) {
            let moved = slider.stepped(self.game_value(slider), direction);
            self.game_staged.insert(slider.key.clone(), moved);
        } else if let (Some(staged), Some(MenuItem::Setting(setting))) =
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
        let fraction = ((x - rect.min[0]) / (rect.max[0] - rect.min[0])).clamp(0.0, 1.0);
        if let Some(MenuItem::GameSlider(slider)) = self.screen.items.get(row) {
            self.game_staged
                .insert(slider.key.clone(), slider.at_fraction(fraction));
        } else if let (Some(staged), Some(MenuItem::Setting(setting))) =
            (&mut self.staged, self.screen.items.get(row))
            && let Control::Slider { min, max, .. } = setting.control()
        {
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

/// One entry of the stack: a menu screen or an item lists screen.
#[derive(Debug)]
enum Layer<A> {
    Menu(Open<A>),
    Lists(OpenLists<A>),
}

/// The open screens, bottom to top, and the events not yet taken.
#[derive(Debug)]
pub struct MenuStack<A> {
    screens: Vec<Layer<A>>,
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
        self.screens.push(Layer::Menu(Open::new(screen)));
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

    /// Opens `lists` over the current screen, the first list focused on its first row.
    pub fn push_lists(&mut self, lists: ItemLists<A>) {
        self.dragging = None;
        self.screens.push(Layer::Lists(OpenLists::new(lists)));
    }

    /// Replaces the top lists screen's data, keeping each list's focused row (clamped) and scroll
    /// by [`ListId`], the focused list while its id is present and a held row while its list and
    /// item remain; does nothing when the top screen is not a lists screen.
    pub fn set_lists(&mut self, lists: ItemLists<A>)
    where
        A: PartialEq,
    {
        if let Some(Layer::Lists(open)) = self.screens.last_mut() {
            open.replace(lists);
        }
    }

    /// The data of the top screen when it is an item lists screen.
    pub fn top_lists(&self) -> Option<&ItemLists<A>> {
        match self.screens.last() {
            Some(Layer::Lists(open)) => Some(open.data()),
            _ => None,
        }
    }

    /// Ends a capture in progress without an event.
    pub fn cancel_capture(&mut self) {
        if let Some(Layer::Menu(open)) = self.screens.last_mut() {
            open.capturing = false;
            open.shell_note = None;
        }
    }

    /// Scrolls the top screen by `rows` rows, keeping a fractional remainder.
    pub fn scroll(&mut self, rows: f32) {
        match self.screens.last_mut() {
            Some(Layer::Menu(open)) => {
                let size = open.size.get();
                open.settle(size);
                let last = max_scroll(&open.screen.items, viewport(size));
                let (scroll, remainder) = wheel(open.scroll.get(), open.remainder, rows, last);
                open.scroll.set(scroll);
                open.remainder = remainder;
            }
            Some(Layer::Lists(open)) => open.scroll(rows),
            None => {}
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Moves the dragged slider to `point`, in logical points on a window `size` points across.
    pub fn drag(&mut self, point: [f32; 2], size: [f32; 2]) {
        if let (Some(Layer::Menu(open)), Some(row)) = (self.screens.last_mut(), self.dragging) {
            open.set_slider_from(row, point[0], size);
        }
    }

    /// Whether the top screen is waiting for a key or button to bind.
    pub(crate) fn is_capturing(&self) -> bool {
        matches!(self.screens.last(), Some(Layer::Menu(open)) if open.capturing)
    }

    pub fn is_open(&self) -> bool {
        !self.screens.is_empty()
    }

    /// The top screen when it is a menu screen; `None` while an item lists screen is on top.
    pub fn top(&self) -> Option<&MenuScreen<A>> {
        match self.screens.last() {
            Some(Layer::Menu(open)) => Some(&open.screen),
            _ => None,
        }
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<MenuEvent<A>> {
        std::mem::take(&mut self.events)
    }

    /// Focuses the row under `point`, in logical points on a window `size` points across;
    /// a point over no row keeps the focus.
    pub fn hover(&mut self, point: [f32; 2], size: [f32; 2]) {
        match self.screens.last_mut() {
            Some(Layer::Menu(open)) => {
                if !open.capturing
                    && let Some((row, rect)) = open.row_at(point, size)
                {
                    let column = open.column_at(row, rect, point[0]).unwrap_or(open.column);
                    open.set_focus(row, column);
                }
            }
            Some(Layer::Lists(open)) => open.hover(point, size),
            None => {}
        }
    }

    /// The text of the top screen on a window `size` points across: its title above the rows,
    /// then one centre-anchored text per row with the focused one in its own colour.
    pub(crate) fn texts(&self, size: [f32; 2]) -> Vec<ScreenText> {
        let open = match self.screens.last() {
            Some(Layer::Menu(open)) => open,
            Some(Layer::Lists(open)) => return open.texts(size),
            None => return Vec::new(),
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
        let setting_row = |label: &str, value: Option<&str>, rect: Rect, color| {
            let y = centre_y(rect);
            let label = text(label, Anchor::Left, rect.min[0], y, SETTING_SIZE, color);
            let value = value.map(|value| {
                text(
                    value,
                    Anchor::Right,
                    rect.max[0] - size[0],
                    y,
                    SETTING_SIZE,
                    color,
                )
            });
            std::iter::once(label).chain(value)
        };
        let mut texts = Vec::new();
        if let Some(title) = &open.screen.title {
            let first_centre = rows
                .get(visible.start)
                .map_or(size[1] / 2.0, |r| centre_y(*r));
            let mut title_centre = first_centre - ROW_PITCH;
            if matches!(
                open.screen.items.get(visible.start),
                Some(MenuItem::Control { .. } | MenuItem::Details { .. })
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
                MenuItem::Control { .. } => {
                    texts.extend(open.control_texts(row, rect, color, size));
                }
                MenuItem::Details { .. } => {
                    texts.extend(open.details_texts(row, rect, color, size));
                }
                MenuItem::GameSlider(slider) => {
                    let value = slider.value_text(open.game_value(slider));
                    texts.extend(setting_row(&slider.label, Some(&value), rect, color));
                }
                MenuItem::Setting(setting) => {
                    let value = open
                        .staged
                        .as_ref()
                        .map(|staged| staged.value_text(*setting));
                    texts.extend(setting_row(setting.label(), value.as_deref(), rect, color));
                }
            }
        }
        texts
    }

    /// The rectangles of the top screen's rows fully in view on a window `size` points across;
    /// empty with no screen open.
    pub(crate) fn visible_rows(&self, size: [f32; 2]) -> Vec<Rect> {
        let open = match self.screens.last() {
            Some(Layer::Menu(open)) => open,
            Some(Layer::Lists(open)) => return open.visible_rows(size),
            None => return Vec::new(),
        };
        let rows = open.rects(size);
        rows[open.visible(size)].to_vec()
    }
}

impl<A: PartialEq> MenuStack<A> {
    /// Replaces the top screen's items, keeping its column and scroll; a capture goes on only
    /// while the same `Control` row has the focus, else it ends without an event. The focus stays
    /// on the same row: the same index if the row there is the same one (see `same_row`),
    /// else the nearest row that is, else the old index clamped to the new rows.
    pub fn set_items(&mut self, items: Vec<MenuItem<A>>) {
        if let Some(Layer::Menu(open)) = self.screens.last_mut() {
            let old = open.screen.items.get(open.focus);
            let focus = refound_focus(old, open.focus, &items);
            let same = old
                .zip(items.get(focus))
                .is_some_and(|(old, new)| same_row(old, new));
            if focus != open.focus {
                open.reveal.set(true);
            }
            open.focus = focus;
            open.column = nearest_column(items.get(focus), open.column);
            open.screen.items = items;
            // A capture belongs to the row it started on and ends when that row is gone.
            if !(same && open.focused_is_control() && open.column.slot().is_some()) {
                open.capturing = false;
            }
            open.shell_note = None;
            open.notes_hidden = false;
        }
    }
}

impl<A: Clone> MenuStack<A> {
    /// Up and Down move the focus and wrap; Left and Right walk a `Control` or `Details` row's
    /// columns (stopping at the ends) or step a settings row; a non-repeat Enter press acts on
    /// the focused column (toggle details, start a capture, step a picker, report the other
    /// mode) or chooses the focused item (flipping or cycling a settings row);
    /// a non-repeat Escape press chooses the top screen's back action, if any. Releasing the
    /// left mouse button ends a slider drag; other releases and inputs do nothing.
    pub fn on_key(&mut self, input: Input, pressed: bool, repeat: bool) {
        let Some(layer) = self.screens.last_mut() else {
            return;
        };
        if !pressed {
            if input == Input::Mouse(MouseButton::Left) {
                self.dragging = None;
            }
            return;
        }
        let open = match layer {
            Layer::Menu(open) => open,
            Layer::Lists(open) => {
                self.events.extend(open.on_key(input, repeat));
                return;
            }
        };
        if open.capturing {
            if !repeat {
                self.events.extend(open.capture(input));
            }
            return;
        }
        let count = open.screen.items.len();
        match input {
            Input::Key(Key::ArrowDown) if count > 0 => {
                open.set_focus((open.focus + 1) % count, open.column);
            }
            Input::Key(Key::ArrowUp) if count > 0 => {
                open.set_focus((open.focus + count - 1) % count, open.column);
            }
            Input::Key(Key::ArrowLeft) => open.step(-1),
            Input::Key(Key::ArrowRight) => open.step(1),
            Input::Key(Key::Enter) if !repeat => {
                self.events.extend(open.activate());
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
        let open = match self.screens.last_mut() {
            Some(Layer::Menu(open)) => open,
            Some(Layer::Lists(open)) => {
                open.release();
                return;
            }
            None => return,
        };
        if open.capturing {
            self.on_key(Input::Mouse(MouseButton::Right), true, false);
        } else if let Some((row, rect)) = open.row_at(point, size)
            && row == open.focus
            && open.column_at(row, rect, point[0]) == Some(open.column)
        {
            self.events.extend(open.clear());
        }
    }

    /// Chooses the row under `point` (see [`MenuStack::hover`]); a point over no row chooses
    /// nothing. A press on a slider row sets it from `point` and starts a drag that
    /// [`MenuStack::drag`] follows until the left button is released.
    pub fn click(&mut self, point: [f32; 2], size: [f32; 2]) {
        let open = match self.screens.last_mut() {
            Some(Layer::Menu(open)) => open,
            Some(Layer::Lists(open)) => {
                self.events.extend(open.click(point, size));
                return;
            }
            None => return,
        };
        if open.capturing {
            self.dragging = None;
            self.events
                .extend(open.capture(Input::Mouse(MouseButton::Left)));
        } else if let Some((row, rect)) = open.row_at(point, size) {
            let on_slider = match open.screen.items[row] {
                MenuItem::Setting(setting) => matches!(setting.control(), Control::Slider { .. }),
                MenuItem::GameSlider(_) => true,
                _ => false,
            };
            if let Some(column) = open.column_at(row, rect, point[0]) {
                open.set_focus(row, column);
                self.events.extend(open.activate());
            } else if on_slider {
                self.dragging = Some(row);
                open.set_slider_from(row, point[0], size);
            } else {
                self.events.extend(open.choose(row));
            }
        }
    }
}

/// The first visible row and the unshown fraction of a row after the wheel scrolls `rows` rows
/// from `scroll` with `remainder` pending, kept within `0..=last`.
pub(crate) fn wheel(scroll: usize, remainder: f32, rows: f32, last: usize) -> (usize, f32) {
    let target = (scroll as f32 + remainder + rows).clamp(0.0, last as f32);
    let whole = target.floor();
    (whole as usize, target - whole)
}

fn centre_y(rect: Rect) -> f32 {
    (rect.min[1] + rect.max[1]) / 2.0
}

/// A left-anchored row text at `x` and `centre_y` on a window `size` points across.
fn left_text(text: &str, x: f32, centre_y: f32, color: [u8; 4], size: [f32; 2]) -> ScreenText {
    ScreenText {
        text: text.to_owned(),
        anchor: Anchor::Left,
        offset: [x, centre_y - size[1] / 2.0],
        size: SETTING_SIZE,
        color,
    }
}

/// Height available to rows: the window less a title band above and below.
pub(crate) fn viewport(size: [f32; 2]) -> f32 {
    size[1] - 2.0 * BAND
}

fn item_height<A>(item: &MenuItem<A>) -> f32 {
    match item {
        MenuItem::Control { .. } | MenuItem::Details { .. } => CONTROL_HEIGHT,
        _ => ROW_PITCH,
    }
}

/// The width of `item`'s row; `with_checkbox` is whether some `Control` row of the screen has
/// the checkbox column, which widens every `Control` and `Details` row to hold it.
fn item_width<A>(item: &MenuItem<A>, with_checkbox: bool) -> f32 {
    match item {
        MenuItem::Control { .. } | MenuItem::Details { .. } if with_checkbox => {
            CONTROL_WIDTH + MODE_WIDTH
        }
        MenuItem::Control { .. } | MenuItem::Details { .. } => CONTROL_WIDTH,
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
    let with_checkbox = items
        .iter()
        .any(|item| matches!(item, MenuItem::Control { mode: Some(_), .. }));
    let mut y = top;
    items
        .iter()
        .map(|item| {
            let (height, width) = (item_height(item), item_width(item, with_checkbox));
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
/// buttons and settings 240 by 56 points, control and details rows 640 by 32 (768 when a control
/// row has the checkbox column), centred on the window. The column is centred vertically when it
/// fits and starts at the title band when it does not.
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
                | MenuEvent::GameSettingsApplied(_)
                | MenuEvent::Captured { .. }
                | MenuEvent::DetailsToggled { .. }
                | MenuEvent::Picked { .. }
                | MenuEvent::ModeChosen { .. }
                | MenuEvent::Cleared { .. } => None,
                MenuEvent::MoveRequested { .. } => None,
                MenuEvent::ActionPicked { .. } => None,
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
            chevron: None,
            mode: None,
        }
    }

    fn noted(index: usize, note: &str) -> MenuItem<Bind> {
        MenuItem::Control {
            label: format!("Action {index:02}"),
            action: Bind::Row(index),
            slots: [R, None],
            note: Some(note.to_owned()),
            chevron: None,
            mode: None,
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

        assert_eq!(top, (true, false, false), "ten of the fourteen rows fit");
        assert_eq!(
            end,
            (false, false, false, true),
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
            master_volume: 10.0,
            sound_effect_volume: 5.0,
            music_volume: 6.0,
            ui_volume: 7.0,
            shadow_quality: 3,
            ssao_quality: 3,
            effects_quality: 3,
            window_mode: moho_core::prefs::WindowMode::Windowed,
            resolution: (1920, 1080),
        }
    }

    /// Fourteen settings-screen rows at 720 points, scrolled to the end: rows 4 to 13 are in view.
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
    fn visible_rows_are_empty_with_no_screen_and_only_the_slice_in_view_when_scrolled() {
        let mut stack = MenuStack::<Bind>::new();
        let closed = stack.visible_rows(SIZE);
        stack.push(controls_screen(controls_of(30)));
        stack.scroll(3.0);
        assert!(!row_shown(&stack, 0), "the page must be scrolled");

        let rows = stack.visible_rows(SIZE);

        assert_eq!(closed, []);
        let texts = stack.texts(SIZE);
        let drawn = texts.len() / 3;
        assert!(drawn > 0 && drawn < 30, "{drawn} rows in view");
        assert_eq!(rows.len(), drawn, "one rect per row in view");
        for (index, rect) in rows.iter().enumerate() {
            let label_y = SIZE[1] / 2.0 + texts[index * 3].offset[1];
            assert_eq!(centre_y(*rect), label_y, "row {index} is the one in view");
        }
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

    fn row_with(
        index: usize,
        slots: [Option<Binding>; 2],
        chevron: Option<Chevron>,
        mode: Option<ActionMode>,
    ) -> MenuItem<Bind> {
        MenuItem::Control {
            label: format!("Action {index:02}"),
            action: Bind::Row(index),
            slots,
            note: None,
            chevron,
            mode,
        }
    }

    fn labelled(label: &str, action: Bind) -> MenuItem<Bind> {
        MenuItem::Control {
            label: label.to_owned(),
            action,
            slots: [R, None],
            note: None,
            chevron: None,
            mode: None,
        }
    }

    fn details(index: usize, slot: Slot, trigger: Trigger, modifiers: Modifiers) -> MenuItem<Bind> {
        MenuItem::Details {
            action: Bind::Row(index),
            slot,
            trigger,
            modifiers,
        }
    }

    fn picked(index: usize, slot: Slot, trigger: Trigger, modifiers: Modifiers) -> MenuEvent<Bind> {
        MenuEvent::Picked {
            action: Bind::Row(index),
            slot,
            trigger,
            modifiers,
        }
    }

    fn toggled(index: usize) -> MenuEvent<Bind> {
        MenuEvent::DetailsToggled {
            action: Bind::Row(index),
        }
    }

    fn mode_chosen(index: usize, mode: ActionMode) -> MenuEvent<Bind> {
        MenuEvent::ModeChosen {
            action: Bind::Row(index),
            mode,
        }
    }

    /// Enter on the focused column; a capture it starts is answered with Q. Returns the events.
    fn act(stack: &mut MenuStack<Bind>) -> Vec<MenuEvent<Bind>> {
        key_c(stack, Key::Enter);
        if stack.is_capturing() {
            key_c(stack, Key::Q);
        }
        stack.take_events()
    }

    /// A click at `point`; a capture it starts is answered with Q. Returns the events.
    fn click_and_settle(stack: &mut MenuStack<Bind>, point: [f32; 2]) -> Vec<MenuEvent<Bind>> {
        stack.click(point, SIZE);
        if stack.is_capturing() {
            key_c(stack, Key::Q);
        }
        stack.take_events()
    }

    fn press_all(stack: &mut MenuStack<Bind>, keys: &[Key]) {
        for &key in keys {
            key_c(stack, key);
        }
    }

    fn row_left(stack: &MenuStack<Bind>, row: usize) -> f32 {
        cell_point(stack, row, 0.0)[0]
    }

    /// The first row whose texts include the focus colour.
    fn focused_row(stack: &MenuStack<Bind>) -> Option<usize> {
        let rows = stack.top().expect("a screen is open").items.len();
        (0..rows).find(|&row| row_texts(stack, row).iter().any(|t| t.color == FOCUS))
    }

    /// The chevron cell's width, pinned independently of the production constant.
    const CHEVRON_WIDTH: f32 = 24.0;
    /// Where the checkbox column starts in a row, and a point inside it.
    const MODE_START: f32 = 640.0;
    const MODE_X: f32 = 700.0;

    #[test]
    fn enter_or_a_click_on_the_chevron_reports_details_toggled_and_moves_nothing() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Closed), None),
                row_with(1, [R, R], Some(Chevron::Closed), None),
            ]
        };
        let original = controls_screen(rows());
        let mut by_enter = controls_stack(rows());
        let mut by_click = controls_stack(rows());

        key_c(&mut by_enter, Key::ArrowLeft);
        key_c(&mut by_enter, Key::Enter);
        let after_enter = by_enter.take_events();
        let enter_capturing = by_enter.is_capturing();
        key_c(&mut by_enter, Key::Enter);
        let enter_again = by_enter.take_events();
        let point = cell_point(&by_click, 1, 10.0);
        by_click.click(point, SIZE);
        let after_click = by_click.take_events();
        let click_capturing = by_click.is_capturing();
        key_c(&mut by_click, Key::Q);

        assert_eq!(after_enter, [toggled(0)]);
        assert!(!enter_capturing, "the chevron starts no capture");
        assert_eq!(enter_again, [toggled(0)], "the focus stayed on the chevron");
        assert_eq!(by_enter.top(), Some(&original), "the shell moves no rows");
        assert_eq!(after_click, [toggled(1)]);
        assert!(!click_capturing);
        assert_eq!(by_click.take_events(), [], "no capture followed the click");
        assert_eq!(by_click.top(), Some(&original));
    }

    #[test]
    fn set_items_keeps_the_focus_on_the_same_row_when_rows_above_it_open_or_close() {
        let closed = || -> Vec<MenuItem<Bind>> {
            (0..30)
                .map(|index| row_with(index, [R, R], Some(Chevron::Closed), None))
                .collect()
        };
        let opened = || -> Vec<MenuItem<Bind>> {
            let mut items = closed();
            items.splice(
                3..3,
                [
                    details(2, Slot::Primary, Trigger::Press, Modifiers::NONE),
                    details(2, Slot::Secondary, Trigger::Press, Modifiers::NONE),
                ],
            );
            items
        };
        let mut stack = controls_stack(closed());
        press_all(&mut stack, &[Key::ArrowDown; 22]);
        key_c(&mut stack, Key::ArrowRight);

        stack.set_items(opened());
        let shown_open = row_shown(&stack, 22);
        let after_open = act(&mut stack);
        stack.set_items(closed());
        let shown_closed = row_shown(&stack, 22);
        let after_close = act(&mut stack);

        let captured_22 = [captured(Bind::Row(22), Slot::Secondary, Input::Key(Key::Q))];
        assert!(shown_open, "the row is scrolled back into view");
        assert_eq!(after_open, captured_22, "two rows opened above the focus");
        assert!(shown_closed);
        assert_eq!(after_close, captured_22, "and closed again");
    }

    #[test]
    fn set_items_keeps_the_focus_on_a_details_row_or_a_button_by_identity() {
        let items = || {
            vec![
                row_with(3, [R, R], Some(Chevron::Open), None),
                details(3, Slot::Primary, Trigger::Press, Modifiers::NONE),
                details(3, Slot::Secondary, Trigger::Press, Modifiers::NONE),
                bind_button("Reset", Bind::Reset),
            ]
        };
        let mut above = vec![control(8, [R, R]), control(9, [R, R])];
        above.extend(items());
        let mut on_details = controls_stack(items());
        let mut on_button = controls_stack(items());
        press_all(&mut on_details, &[Key::ArrowDown, Key::ArrowDown]);
        press_all(&mut on_button, &[Key::ArrowDown; 3]);

        on_details.set_items(above.clone());
        on_button.set_items(above);

        assert_eq!(
            act(&mut on_details),
            [picked(3, Slot::Secondary, Trigger::Hold, Modifiers::NONE)]
        );
        assert_eq!(act(&mut on_button), [MenuEvent::Chosen(Bind::Reset)]);
    }

    #[test]
    fn enter_on_the_kind_cell_steps_press_to_hold_and_back_and_reports_the_slot() {
        let rows = |trigger: Trigger| {
            vec![
                row_with(0, [R, R], Some(Chevron::Open), None),
                details(0, Slot::Primary, trigger, Modifiers::NONE),
                details(0, Slot::Secondary, Trigger::Hold, Modifiers::CTRL),
                bind_button("Reset", Bind::Reset),
            ]
        };
        for (from, to) in [
            (Trigger::Press, Trigger::Hold),
            (Trigger::Hold, Trigger::Double),
            (Trigger::Double, Trigger::Press),
        ] {
            let mut by_enter = controls_stack(rows(from));
            let mut by_click = controls_stack(rows(from));

            key_c(&mut by_enter, Key::ArrowDown);
            let first = act(&mut by_enter);
            let again = act(&mut by_enter);
            let point = cell_point(&by_click, 1, PRIMARY_X);
            let clicked = click_and_settle(&mut by_click, point);

            let step = [picked(0, Slot::Primary, to, Modifiers::NONE)];
            assert_eq!(first, step, "{from:?} steps to {to:?}");
            assert_eq!(again, step, "the game owns the result: nothing moved on");
            assert_eq!(clicked, step, "a click steps it too");
        }
        let mut second = controls_stack(rows(Trigger::Press));

        press_all(&mut second, &[Key::ArrowDown, Key::ArrowDown]);
        let events = act(&mut second);

        assert_eq!(
            events,
            [picked(0, Slot::Secondary, Trigger::Double, Modifiers::CTRL)],
            "the slot's own modifiers are reported with its new kind"
        );
    }

    #[test]
    fn the_modifier_cell_steps_none_ctrl_shift_alt_none() {
        let rows = |modifiers: Modifiers| {
            vec![
                row_with(0, [R, R], Some(Chevron::Open), None),
                details(0, Slot::Secondary, Trigger::Hold, modifiers),
            ]
        };
        for (from, to) in [
            (Modifiers::NONE, Modifiers::CTRL),
            (Modifiers::CTRL, Modifiers::SHIFT),
            (Modifiers::SHIFT, Modifiers::ALT),
            (Modifiers::ALT, Modifiers::NONE),
        ] {
            let mut by_enter = controls_stack(rows(from));
            let mut by_click = controls_stack(rows(from));

            press_all(&mut by_enter, &[Key::ArrowDown, Key::ArrowRight]);
            let entered = act(&mut by_enter);
            let point = cell_point(&by_click, 1, SECONDARY_X);
            let clicked = click_and_settle(&mut by_click, point);

            let step = [picked(0, Slot::Secondary, Trigger::Hold, to)];
            assert_eq!(entered, step, "{from:?} steps to {to:?}");
            assert_eq!(clicked, step, "{from:?} by click");
        }
    }

    #[test]
    fn a_multi_modifier_set_steps_to_none() {
        let sets = [
            Modifiers::CTRL.and(Modifiers::SHIFT),
            Modifiers::CTRL.and(Modifiers::ALT),
            Modifiers::SHIFT.and(Modifiers::ALT),
            Modifiers::CTRL.and(Modifiers::SHIFT).and(Modifiers::ALT),
        ];
        for set in sets {
            let mut stack = controls_stack(vec![
                row_with(0, [R, R], Some(Chevron::Open), None),
                details(0, Slot::Primary, Trigger::Press, set),
            ]);

            press_all(&mut stack, &[Key::ArrowDown, Key::ArrowRight]);
            let events = act(&mut stack);

            assert_eq!(
                events,
                [picked(0, Slot::Primary, Trigger::Press, Modifiers::NONE)],
                "{set:?}"
            );
        }
    }

    #[test]
    fn capturing_on_an_expanded_row_still_reports_the_input_only() {
        let original = controls_screen(vec![
            row_with(0, [R, R], Some(Chevron::Open), Some(ActionMode::Hold)),
            details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
            details(0, Slot::Secondary, Trigger::Hold, Modifiers::CTRL),
        ]);
        let mut stack = MenuStack::new();
        stack.push(original.clone());

        let primary = act(&mut stack);
        key_c(&mut stack, Key::ArrowRight);
        key_c(&mut stack, Key::Enter);
        key_c(&mut stack, Key::Ctrl);
        let secondary = stack.take_events();

        assert_eq!(
            primary,
            [captured(Bind::Row(0), Slot::Primary, Input::Key(Key::Q))]
        );
        assert_eq!(
            secondary,
            [captured(
                Bind::Row(0),
                Slot::Secondary,
                Input::Key(Key::Ctrl)
            )],
            "no kind or modifier rides along with the input"
        );
        assert_eq!(stack.top(), Some(&original));
    }

    #[test]
    fn a_button_among_detail_rows_is_chosen_as_any_button() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Open), None),
                details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
                details(0, Slot::Secondary, Trigger::Press, Modifiers::NONE),
                bind_button("Reset", Bind::Reset),
            ]
        };
        let mut by_key = controls_stack(rows());
        let mut by_click = controls_stack(rows());

        press_all(&mut by_key, &[Key::ArrowDown; 3]);
        key_c(&mut by_key, Key::Enter);
        let point = cell_point(&by_click, 3, 120.0);
        by_click.click(point, SIZE);

        assert_eq!(by_key.take_events(), [MenuEvent::Chosen(Bind::Reset)]);
        assert_eq!(by_click.take_events(), [MenuEvent::Chosen(Bind::Reset)]);
    }

    #[test]
    fn a_closed_chevron_hides_hold_and_chord_slots_and_marks_the_label() {
        let closed = Some(Chevron::Closed);
        let ctrl_r = Binding::key(Key::R).with(Modifiers::CTRL);
        let stack = controls_stack(vec![
            row_with(0, [R, Some(Binding::key(Key::R).held())], closed, None),
            row_with(1, [Some(ctrl_r), R], closed, None),
            row_with(2, [R, None], closed, None),
            row_with(3, [None, None], closed, None),
            row_with(
                4,
                [None, Some(Binding::key(Key::R).doubled())],
                closed,
                None,
            ),
        ]);

        let held = row_strings(&stack, 0);
        let chord = row_strings(&stack, 1);
        let plain = row_strings(&stack, 2);
        let unbound = row_strings(&stack, 3);
        let double = row_strings(&stack, 4);

        assert!(held.contains(&"Action 00 +".to_owned()), "{held:?}");
        assert!(held.contains(&"R".to_owned()), "{held:?}");
        assert!(held.contains(&"...".to_owned()), "{held:?}");
        assert!(!held.iter().any(|t| t.contains("Hold")), "{held:?}");
        assert!(chord.contains(&"Action 01 +".to_owned()), "{chord:?}");
        assert!(chord.contains(&"...".to_owned()), "{chord:?}");
        assert!(!chord.iter().any(|t| t.contains("Ctrl")), "{chord:?}");
        assert!(plain.contains(&"Action 02".to_owned()), "{plain:?}");
        assert!(!plain.iter().any(|t| t.contains('+') || t == "..."));
        assert!(unbound.contains(&"! Action 03".to_owned()), "{unbound:?}");
        assert!(double.contains(&"Action 04 +".to_owned()), "{double:?}");
        assert!(double.contains(&"Unbound".to_owned()), "{double:?}");
        assert!(!double.iter().any(|t| t.contains('!')), "{double:?}");
        let glyph = row_texts(&stack, 0)
            .into_iter()
            .find(|t| t.text == ">")
            .expect("a closed chevron is drawn as >");
        let label = row_texts(&stack, 0)
            .into_iter()
            .find(|t| t.text == "Action 00 +")
            .expect("the label is drawn");
        let left = row_left(&stack, 0);
        assert!(
            glyph.offset[0] >= left && glyph.offset[0] < left + CHEVRON_WIDTH,
            "the chevron sits in the first 24 points"
        );
        assert_eq!(label.offset[0], left + CHEVRON_WIDTH, "the label shifts");
    }

    #[test]
    fn an_open_or_absent_chevron_shows_every_binding() {
        let hold = Some(Binding::key(Key::R).held());
        let chord = Some(Binding::key(Key::R).with(Modifiers::CTRL));
        let stack = controls_stack(vec![
            row_with(0, [hold, chord], Some(Chevron::Open), None),
            row_with(1, [hold, chord], None, None),
        ]);

        let open = row_strings(&stack, 0);
        let absent = row_strings(&stack, 1);

        for (name, strings) in [("open", &open), ("absent", &absent)] {
            assert!(
                strings.contains(&"Hold R".to_owned()),
                "{name}: {strings:?}"
            );
            assert!(
                strings.contains(&"Ctrl+R".to_owned()),
                "{name}: {strings:?}"
            );
            assert!(!strings.iter().any(|t| t == "..."), "{name}: {strings:?}");
            assert!(
                !strings
                    .iter()
                    .any(|t| t.contains('+') && t.contains("Action"))
            );
        }
        assert!(open.contains(&"v".to_owned()), "an open chevron is v");
        assert!(open.contains(&"Action 00".to_owned()), "{open:?}");
        assert!(absent.contains(&"Action 01".to_owned()), "{absent:?}");
        assert!(
            !absent.iter().any(|t| t == "v" || t == ">"),
            "no chevron is drawn: {absent:?}"
        );
        let open_label = row_texts(&stack, 0)
            .into_iter()
            .find(|t| t.text == "Action 00")
            .expect("the label is drawn");
        let absent_label = row_texts(&stack, 1)
            .into_iter()
            .find(|t| t.text == "Action 01")
            .expect("the label is drawn");
        assert_eq!(open_label.offset[0], row_left(&stack, 0) + CHEVRON_WIDTH);
        assert_eq!(absent_label.offset[0], row_left(&stack, 1));
    }

    #[test]
    fn the_mode_column_shows_the_checkbox_and_reports_the_other_mode() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Closed), Some(ActionMode::Hold)),
                row_with(1, [R, R], None, Some(ActionMode::Toggle)),
            ]
        };
        let mut by_key = controls_stack(rows());
        let mut by_click = controls_stack(rows());

        let off = row_texts(&by_key, 0)
            .into_iter()
            .find(|t| t.text == "[ ] Toggle")
            .expect("a held action shows an empty box");
        let on = row_strings(&by_key, 1);
        press_all(&mut by_key, &[Key::ArrowRight, Key::ArrowRight]);
        let focused = row_texts(&by_key, 0)
            .into_iter()
            .find(|t| t.text == "[ ] Toggle")
            .expect("the box is still drawn");
        let secondary_focused = row_texts(&by_key, 0)
            .iter()
            .any(|t| t.text == "R" && t.color == FOCUS);
        let first = act(&mut by_key);
        press_all(&mut by_key, &[Key::ArrowDown]);
        let second = act(&mut by_key);
        let point = cell_point(&by_click, 1, MODE_X);
        let clicked = click_and_settle(&mut by_click, point);

        assert_eq!(off.offset[0], row_left(&by_key, 0) + MODE_START);
        assert!(on.contains(&"[x] Toggle".to_owned()), "{on:?}");
        assert_eq!(focused.color, FOCUS, "the mode column has the focus");
        assert!(!secondary_focused, "and the slot cell does not");
        assert_eq!(first, [mode_chosen(0, ActionMode::Toggle)]);
        assert_eq!(second, [mode_chosen(1, ActionMode::Hold)]);
        assert_eq!(clicked, [mode_chosen(1, ActionMode::Hold)]);
    }

    #[test]
    fn a_row_without_mode_has_no_checkbox_column() {
        let rows = || {
            vec![
                row_with(0, [R, R], None, Some(ActionMode::Hold)),
                row_with(1, [R, R], None, None),
            ]
        };
        let mut by_keys = controls_stack(rows());
        let mut by_click = controls_stack(rows());
        let plain = controls_stack(vec![row_with(0, [R, R], None, None)]);

        key_c(&mut by_keys, Key::ArrowDown);
        let drawn = row_strings(&by_keys, 1);
        press_all(&mut by_keys, &[Key::ArrowRight, Key::ArrowRight]);
        let by_keys_events = act(&mut by_keys);
        let point = cell_point(&by_click, 1, MODE_X);
        by_click.click(point, SIZE);
        let click_capturing = by_click.is_capturing();
        let by_click_events = by_click.take_events();
        let plain_point = cell_point(&plain, 0, MODE_X);
        let widths: Vec<f32> = layout(by_click.top().expect("a screen is open"), SIZE)
            .iter()
            .map(|r| r.max[0] - r.min[0])
            .collect();
        let mut with_mode = controls_stack(rows());
        press_all(&mut with_mode, &[Key::ArrowRight, Key::ArrowRight]);
        let mode_events = act(&mut with_mode);

        assert_eq!(widths, [768.0, 768.0], "the click below lands inside row 1");
        assert_eq!(
            mode_events,
            [mode_chosen(0, ActionMode::Toggle)],
            "the row with a mode has the column"
        );
        assert!(!drawn.iter().any(|t| t.contains("Toggle")), "{drawn:?}");
        assert_eq!(
            by_keys_events,
            [captured(Bind::Row(1), Slot::Secondary, Input::Key(Key::Q))],
            "Right stops at the secondary slot"
        );
        assert!(!click_capturing, "the empty column starts no capture");
        assert_eq!(by_click_events, []);
        let mut plain = plain;
        plain.click(plain_point, SIZE);
        assert!(!plain.is_capturing());
        assert_eq!(plain.take_events(), [], "outside the row of a plain screen");
    }

    #[test]
    fn left_and_right_walk_chevron_slots_and_mode_and_skip_missing_columns() {
        let both = row_with(0, [R, R], Some(Chevron::Closed), Some(ActionMode::Hold));
        let chevron_only = row_with(0, [R, R], Some(Chevron::Closed), None);
        let mode_only = row_with(0, [R, R], None, Some(ActionMode::Hold));
        let neither = row_with(0, [R, R], None, None);
        let to_toggle = mode_chosen(0, ActionMode::Toggle);
        let primary = captured(Bind::Row(0), Slot::Primary, Input::Key(Key::Q));
        let secondary = captured(Bind::Row(0), Slot::Secondary, Input::Key(Key::Q));
        let (left, right) = (Key::ArrowLeft, Key::ArrowRight);
        let cases = vec![
            ("both: Left", &both, vec![left], toggled(0)),
            ("both: Left Left", &both, vec![left, left], toggled(0)),
            (
                "both: Left Right",
                &both,
                vec![left, right],
                primary.clone(),
            ),
            ("both: Right", &both, vec![right], secondary.clone()),
            (
                "both: Right Right",
                &both,
                vec![right, right],
                to_toggle.clone(),
            ),
            (
                "both: Right x3",
                &both,
                vec![right, right, right],
                to_toggle.clone(),
            ),
            (
                "both: Right Right Left",
                &both,
                vec![right, right, left],
                secondary.clone(),
            ),
            (
                "chevron only: Right Right",
                &chevron_only,
                vec![right, right],
                secondary.clone(),
            ),
            ("chevron only: Left", &chevron_only, vec![left], toggled(0)),
            ("mode only: Left", &mode_only, vec![left], primary.clone()),
            (
                "mode only: Right Right",
                &mode_only,
                vec![right, right],
                to_toggle.clone(),
            ),
            (
                "mode only: Right x3",
                &mode_only,
                vec![right, right, right],
                to_toggle.clone(),
            ),
            ("neither: Left", &neither, vec![left], primary.clone()),
            ("neither: Right", &neither, vec![right], secondary.clone()),
            (
                "neither: Right Right",
                &neither,
                vec![right, right],
                secondary.clone(),
            ),
            (
                "neither: Right Left",
                &neither,
                vec![right, left],
                primary.clone(),
            ),
        ];

        for (name, row, keys, expected) in cases {
            let mut stack = controls_stack(vec![row.clone()]);

            press_all(&mut stack, &keys);
            let events = act(&mut stack);

            assert_eq!(events, [expected], "{name}");
        }
    }

    #[test]
    fn left_and_right_on_a_details_row_walk_kind_and_modifier_only() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Open), Some(ActionMode::Hold)),
                details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
            ]
        };
        let kind = picked(0, Slot::Primary, Trigger::Hold, Modifiers::NONE);
        let modifier = picked(0, Slot::Primary, Trigger::Press, Modifiers::CTRL);
        let (left, right) = (Key::ArrowLeft, Key::ArrowRight);
        let down = Key::ArrowDown;
        let cases = [
            ("Down", vec![down], kind.clone()),
            ("Down Left", vec![down, left], kind.clone()),
            ("Down Right", vec![down, right], modifier.clone()),
            ("Down Right Right", vec![down, right, right], modifier),
            ("Down Right Left", vec![down, right, left], kind),
        ];

        for (name, keys, expected) in cases {
            let mut stack = controls_stack(rows());

            press_all(&mut stack, &keys);
            let events = act(&mut stack);

            assert_eq!(events, [expected], "{name}");
        }
    }

    #[test]
    fn up_and_down_keep_the_column_clamped_to_the_new_row() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Closed), Some(ActionMode::Hold)),
                row_with(1, [R, R], Some(Chevron::Closed), None),
                row_with(2, [R, R], None, None),
                row_with(3, [R, R], None, Some(ActionMode::Hold)),
                details(4, Slot::Primary, Trigger::Press, Modifiers::NONE),
            ]
        };
        let kind = picked(4, Slot::Primary, Trigger::Hold, Modifiers::NONE);
        let modifier = picked(4, Slot::Primary, Trigger::Press, Modifiers::CTRL);
        let (left, right, down, up) = (
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::ArrowDown,
            Key::ArrowUp,
        );
        let cases = [
            (
                "Chevron kept on a row with one",
                vec![left, down],
                toggled(1),
            ),
            (
                "Chevron clamps to First",
                vec![left, down, down],
                captured(Bind::Row(2), Slot::Primary, Input::Key(Key::Q)),
            ),
            (
                "Mode clamps to Second",
                vec![right, right, down],
                captured(Bind::Row(1), Slot::Secondary, Input::Key(Key::Q)),
            ),
            (
                "Second kept down the page",
                vec![right, right, down, down, down],
                captured(Bind::Row(3), Slot::Secondary, Input::Key(Key::Q)),
            ),
            (
                "First kept across a row with a mode",
                vec![down, down, down],
                captured(Bind::Row(3), Slot::Primary, Input::Key(Key::Q)),
            ),
            ("Up wraps and Chevron clamps to First", vec![left, up], kind),
            (
                "Up wraps and Mode clamps to Second",
                vec![right, right, up],
                modifier,
            ),
        ];

        for (name, keys, expected) in cases {
            let mut stack = controls_stack(rows());

            press_all(&mut stack, &keys);
            let events = act(&mut stack);

            assert_eq!(events, [expected], "{name}");
        }
        let mut modes = controls_stack(vec![
            row_with(0, [R, R], Some(Chevron::Closed), Some(ActionMode::Hold)),
            row_with(1, [R, R], None, Some(ActionMode::Toggle)),
        ]);
        press_all(&mut modes, &[right, right, down]);
        assert_eq!(
            act(&mut modes),
            [mode_chosen(1, ActionMode::Hold)],
            "Mode kept on a row that has it"
        );
    }

    #[test]
    fn backspace_on_the_chevron_mode_or_a_picker_clears_nothing() {
        let right = Input::Mouse(MouseButton::Right);
        let fresh = || {
            controls_stack(vec![
                row_with(0, [R, R], Some(Chevron::Open), Some(ActionMode::Hold)),
                details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
                details(0, Slot::Secondary, Trigger::Press, Modifiers::NONE),
            ])
        };
        let columns: [(&str, Vec<Key>, usize, f32); 4] = [
            ("chevron", vec![Key::ArrowLeft], 0, 10.0),
            ("mode", vec![Key::ArrowRight, Key::ArrowRight], 0, MODE_X),
            ("kind", vec![Key::ArrowDown], 1, PRIMARY_X),
            (
                "modifier",
                vec![Key::ArrowDown, Key::ArrowRight],
                1,
                SECONDARY_X,
            ),
        ];

        for (name, keys, row, x) in columns {
            let mut by_key = fresh();
            let mut by_button = fresh();
            let mut by_click = fresh();
            press_all(&mut by_key, &keys);
            press_all(&mut by_button, &keys);
            press_all(&mut by_click, &keys);

            key_c(&mut by_key, Key::Backspace);
            by_button.on_key(right, true, false);
            let point = cell_point(&by_click, row, x);
            by_click.hover(point, SIZE);
            by_click.right_click(point, SIZE);

            assert_eq!(by_key.take_events(), [], "{name}: Backspace");
            assert_eq!(by_button.take_events(), [], "{name}: right button");
            assert_eq!(by_click.take_events(), [], "{name}: right click");
        }
        let mut first = fresh();
        let mut second = fresh();
        key_c(&mut first, Key::Backspace);
        press_all(&mut second, &[Key::ArrowRight, Key::Backspace]);
        assert_eq!(
            first.take_events(),
            [MenuEvent::Cleared {
                action: Bind::Row(0),
                slot: Slot::Primary
            }],
            "a slot column still clears"
        );
        assert_eq!(
            second.take_events(),
            [MenuEvent::Cleared {
                action: Bind::Row(0),
                slot: Slot::Secondary
            }]
        );
    }

    #[test]
    fn hover_and_click_pick_the_column_under_the_point() {
        let fresh = || {
            controls_stack(vec![
                row_with(0, [R, R], Some(Chevron::Closed), Some(ActionMode::Hold)),
                details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
            ])
        };
        let hovered = |x: f32| {
            let mut stack = fresh();
            let point = cell_point(&stack, 0, x);
            stack.hover(point, SIZE);
            act(&mut stack)
        };
        let clicked = |row: usize, x: f32| {
            let mut stack = fresh();
            let point = cell_point(&stack, row, x);
            click_and_settle(&mut stack, point)
        };

        assert_eq!(hovered(10.0), [toggled(0)]);
        assert_eq!(
            hovered(PRIMARY_X),
            [captured(Bind::Row(0), Slot::Primary, Input::Key(Key::Q))]
        );
        assert_eq!(
            hovered(SECONDARY_X),
            [captured(Bind::Row(0), Slot::Secondary, Input::Key(Key::Q))]
        );
        assert_eq!(hovered(MODE_X), [mode_chosen(0, ActionMode::Toggle)]);
        assert_eq!(clicked(0, 10.0), [toggled(0)]);
        assert_eq!(
            clicked(0, 639.5),
            [captured(Bind::Row(0), Slot::Secondary, Input::Key(Key::Q))],
            "the secondary cell ends at 640"
        );
        assert_eq!(
            clicked(0, 640.0),
            [mode_chosen(0, ActionMode::Toggle)],
            "and the checkbox column starts there"
        );
        assert_eq!(
            clicked(1, PRIMARY_X),
            [picked(0, Slot::Primary, Trigger::Hold, Modifiers::NONE)]
        );
        assert_eq!(
            clicked(1, SECONDARY_X),
            [picked(0, Slot::Primary, Trigger::Press, Modifiers::CTRL)]
        );
    }

    #[test]
    fn a_screen_with_a_checkbox_widens_every_control_and_details_row() {
        let items = |mode: Option<ActionMode>| {
            vec![
                row_with(0, [R, R], Some(Chevron::Open), mode),
                details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
                control(2, [R, R]),
                bind_button("Reset", Bind::Reset),
            ]
        };
        let narrow = layout(&controls_screen(items(None)), SIZE);
        let wide = layout(&controls_screen(items(Some(ActionMode::Hold))), SIZE);

        let widths =
            |rects: &[Rect]| -> Vec<f32> { rects.iter().map(|r| r.max[0] - r.min[0]).collect() };
        assert_eq!(widths(&narrow), [640.0, 640.0, 640.0, 240.0]);
        assert_eq!(widths(&wide), [768.0, 768.0, 768.0, 240.0]);
        for rects in [&narrow, &wide] {
            for rect in rects {
                assert_eq!(rect_centre(rect)[0], 640.0, "rows stay centred");
            }
            assert_eq!(rects[0].max[1] - rects[0].min[1], 32.0);
            assert_eq!(rects[1].max[1] - rects[1].min[1], 32.0, "same height");
        }
    }

    #[test]
    fn a_details_row_reads_its_slot_kind_and_modifiers() {
        let rows = vec![
            row_with(0, [R, R], Some(Chevron::Open), None),
            details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
            details(0, Slot::Secondary, Trigger::Hold, Modifiers::CTRL),
            details(0, Slot::Primary, Trigger::Double, Modifiers::SHIFT),
            details(0, Slot::Secondary, Trigger::Press, Modifiers::ALT),
            details(
                0,
                Slot::Primary,
                Trigger::Press,
                Modifiers::CTRL.and(Modifiers::SHIFT),
            ),
        ];
        let stack = controls_stack(rows);

        let expected = [
            (1, "Primary", "Kind: Press", "Mod: None"),
            (2, "Secondary", "Kind: Hold", "Mod: Ctrl"),
            (3, "Primary", "Kind: Double", "Mod: Shift"),
            (4, "Secondary", "Kind: Press", "Mod: Alt"),
            (5, "Primary", "Kind: Press", "Mod: Ctrl+Shift"),
        ];
        for (row, slot, kind, modifier) in expected {
            let strings = row_strings(&stack, row);
            assert!(strings.iter().any(|t| t.trim() == slot), "{strings:?}");
            assert!(strings.contains(&kind.to_owned()), "{strings:?}");
            assert!(strings.contains(&modifier.to_owned()), "{strings:?}");
            let label = row_texts(&stack, row)
                .into_iter()
                .find(|t| t.text.trim() == slot)
                .expect("the slot label is drawn");
            assert!(
                label.offset[0] > row_left(&stack, row),
                "the slot label is indented"
            );
        }
    }

    #[test]
    fn set_items_that_removes_the_capturing_row_ends_capture_without_an_event() {
        let answers: [(&str, Vec<MenuItem<Bind>>); 2] = [
            (
                "details",
                vec![details(5, Slot::Primary, Trigger::Press, Modifiers::NONE)],
            ),
            ("button", vec![bind_button("Reset", Bind::Reset)]),
        ];
        for (name, answer) in answers {
            let mut escaped = controls_stack(vec![control(0, [R, R])]);
            let mut entered = controls_stack(vec![control(0, [R, R])]);
            for stack in [&mut escaped, &mut entered] {
                key_c(stack, Key::ArrowRight);
                key_c(stack, Key::Enter);
                stack.set_items(answer.clone());
            }
            let still_capturing = escaped.is_capturing();

            key_c(&mut escaped, Key::Escape);
            key_c(&mut entered, Key::Enter);

            assert!(!still_capturing, "{name}: capture ended with the row");
            assert_eq!(
                escaped.take_events(),
                [MenuEvent::Chosen(Bind::Back)],
                "{name}: Escape is a normal key"
            );
            let expected = if name == "details" {
                vec![picked(5, Slot::Primary, Trigger::Press, Modifiers::CTRL)]
            } else {
                vec![MenuEvent::Chosen(Bind::Reset)]
            };
            assert_eq!(
                entered.take_events(),
                expected,
                "{name}: Enter acts as usual"
            );
        }
    }

    #[test]
    fn set_items_that_keeps_the_capturing_row_keeps_capture_and_its_slot_column() {
        let mut stack = controls_stack(vec![control(0, [R, R]), control(1, [R, R])]);
        key_c(&mut stack, Key::ArrowRight);
        key_c(&mut stack, Key::Enter);

        stack.set_items(vec![
            bind_button("Reset", Bind::Reset),
            control(0, [R, R]),
            control(1, [R, R]),
        ]);
        let capturing = stack.is_capturing();
        key_c(&mut stack, Key::Q);

        assert!(capturing);
        assert_eq!(
            stack.take_events(),
            [captured(Bind::Row(0), Slot::Secondary, Input::Key(Key::Q))]
        );
    }

    #[test]
    fn a_repeated_enter_on_a_picker_chevron_or_checkbox_emits_nothing() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Open), Some(ActionMode::Hold)),
                details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
            ]
        };
        let (left, right, down) = (Key::ArrowLeft, Key::ArrowRight, Key::ArrowDown);
        let columns = [
            ("chevron", vec![left]),
            ("mode", vec![right, right]),
            ("kind", vec![down]),
            ("modifier", vec![down, right]),
        ];
        for (name, keys) in columns {
            let mut stack = controls_stack(rows());
            press_all(&mut stack, &keys);

            stack.on_key(Input::Key(Key::Enter), true, true);
            let repeated = stack.take_events();
            let capturing = stack.is_capturing();
            key_c(&mut stack, Key::Enter);
            let first = stack.take_events();

            assert_eq!(repeated, [], "{name}: a repeat does nothing");
            assert!(!capturing, "{name}");
            assert_eq!(first.len(), 1, "{name}: the real press still acts");
        }
    }

    #[test]
    fn set_items_with_no_rows_and_then_rows_again_keeps_the_stack_working() {
        let mut stack = controls_stack(vec![
            row_with(0, [R, R], Some(Chevron::Open), Some(ActionMode::Hold)),
            details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
        ]);
        press_all(&mut stack, &[Key::ArrowDown, Key::ArrowRight]);

        stack.set_items(Vec::new());
        let point = [640.0, 360.0];
        press_all(
            &mut stack,
            &[
                Key::Enter,
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::ArrowUp,
                Key::ArrowDown,
                Key::Backspace,
            ],
        );
        stack.click(point, SIZE);
        stack.hover(point, SIZE);
        stack.right_click(point, SIZE);
        let capturing = stack.is_capturing();
        let drawn = stack.texts(SIZE);
        let on_empty = stack.take_events();
        stack.set_items(vec![control(0, [R, R]), control(1, [R, R])]);
        let events = act(&mut stack);

        assert_eq!(on_empty, [], "an empty screen reports nothing");
        assert!(!capturing);
        assert_eq!(drawn, []);
        assert_eq!(
            events,
            [captured(Bind::Row(0), Slot::Secondary, Input::Key(Key::Q))],
            "focus 0, and the column fits the row now there"
        );
    }

    #[test]
    fn a_click_on_a_label_area_emits_nothing_steps_nothing_and_starts_no_capture() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Open), None),
                details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
                details(0, Slot::Secondary, Trigger::Press, Modifiers::NONE),
            ]
        };
        let original = controls_screen(rows());
        for (row, offset) in [
            (0, 24.5),
            (0, 100.0),
            (0, 239.5),
            (1, 1.0),
            (1, 100.0),
            (1, 239.5),
            (2, 100.0),
        ] {
            let mut stack = controls_stack(rows());
            let point = cell_point(&stack, row, offset);

            stack.click(point, SIZE);
            let capturing = stack.is_capturing();
            key_c(&mut stack, Key::Q);

            assert!(!capturing, "row {row} at {offset}");
            assert_eq!(stack.take_events(), [], "row {row} at {offset}");
            assert_eq!(stack.top(), Some(&original), "row {row} at {offset}");
        }
    }

    #[test]
    fn hovering_a_details_label_keeps_the_column_and_the_chevron_cell_picks_the_chevron() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Open), None),
                details(0, Slot::Primary, Trigger::Press, Modifiers::NONE),
            ]
        };
        let kind = picked(0, Slot::Primary, Trigger::Hold, Modifiers::NONE);
        let modifier = picked(0, Slot::Primary, Trigger::Press, Modifiers::CTRL);
        let mut second = controls_stack(rows());
        let mut from_chevron = controls_stack(rows());
        let mut to_chevron = controls_stack(rows());
        press_all(&mut second, &[Key::ArrowDown, Key::ArrowRight]);
        key_c(&mut from_chevron, Key::ArrowLeft);
        press_all(&mut to_chevron, &[Key::ArrowDown, Key::ArrowRight]);

        let label = cell_point(&second, 1, 100.0);
        second.hover(label, SIZE);
        from_chevron.hover(label, SIZE);
        let chevron = cell_point(&to_chevron, 0, 10.0);
        to_chevron.hover(chevron, SIZE);

        assert_eq!(act(&mut second), [modifier], "Second is kept");
        assert_eq!(act(&mut from_chevron), [kind], "Chevron clamps to First");
        assert_eq!(act(&mut to_chevron), [toggled(0)]);
    }

    #[test]
    fn a_click_in_the_checkbox_column_reports_the_mode_only_on_a_row_that_has_one() {
        let rows = || {
            vec![
                row_with(0, [R, R], Some(Chevron::Closed), Some(ActionMode::Toggle)),
                row_with(1, [R, R], Some(Chevron::Closed), None),
            ]
        };
        for offset in [640.0, 700.0, 767.0] {
            let mut with_mode = controls_stack(rows());
            let mut without = controls_stack(rows());
            let on_mode = cell_point(&with_mode, 0, offset);
            let on_empty = cell_point(&without, 1, offset);

            let reported = click_and_settle(&mut with_mode, on_mode);
            without.click(on_empty, SIZE);
            let capturing = without.is_capturing();

            assert_eq!(
                reported,
                [mode_chosen(0, ActionMode::Hold)],
                "offset {offset}"
            );
            assert!(!capturing, "offset {offset}");
            assert_eq!(without.take_events(), [], "offset {offset}");
        }
    }

    #[test]
    fn set_items_prefers_the_same_index_when_actions_repeat() {
        let (a, b) = (Bind::Row(0), Bind::Row(1));
        let repeated = |labels: &[&str]| -> Vec<MenuItem<Bind>> {
            labels
                .iter()
                .enumerate()
                .map(|(at, label)| labelled(label, if at % 2 == 0 { a } else { b }))
                .collect()
        };
        let mut same = controls_stack(repeated(&["L0", "L1", "L2", "L3", "L4"]));
        press_all(&mut same, &[Key::ArrowDown, Key::ArrowDown]);
        let mut nearest = controls_stack(repeated(&["L0", "L1", "L2", "L3", "L4"]));
        press_all(&mut nearest, &[Key::ArrowDown, Key::ArrowDown]);

        same.set_items(repeated(&["M0", "M1", "M2", "M3", "M4"]));
        nearest.set_items(vec![
            labelled("N0", a),
            labelled("N1", a),
            labelled("N2", b),
            labelled("N3", b),
            labelled("N4", b),
            labelled("N5", b),
            labelled("N6", a),
        ]);

        assert_eq!(focused_row(&same), Some(2), "index 2 is still action A");
        assert_eq!(
            focused_row(&nearest),
            Some(1),
            "index 2 is B now; the nearest A is at 1, not the first or the last"
        );
    }

    mod set_items_properties {
        use proptest::prelude::prop_assert_eq;

        use super::*;

        proptest! {
            #[test]
            fn set_items_with_the_focused_row_moved_keeps_the_focus_on_it(
                count in 2_usize..40,
                focus_seed in 0_usize..40,
                removed_seed in 0_usize..40,
                inserted in 0_usize..4,
                at_seed in 0_usize..60,
            ) {
                let focus = focus_seed % count;
                let removed = removed_seed % (focus + 1);
                let mut moved: Vec<MenuItem<Bind>> =
                    controls_of(count).into_iter().skip(removed).collect();
                let at = at_seed % (moved.len() + 1);
                for extra in 0..inserted {
                    moved.insert(
                        at,
                        details(1000 + extra, Slot::Primary, Trigger::Press, Modifiers::NONE),
                    );
                }
                let mut stack = controls_stack(controls_of(count));
                for _ in 0..focus {
                    key_c(&mut stack, Key::ArrowDown);
                }

                stack.set_items(moved);
                let drawn = row_shown(&stack, focus);
                let events = act(&mut stack);

                prop_assert!(drawn, "the focused row is scrolled into view");
                prop_assert_eq!(
                    events,
                    vec![captured(Bind::Row(focus), Slot::Primary, Input::Key(Key::Q))]
                );
            }
        }
    }

    mod settings_rows {
        use moho_core::prefs::WindowMode;

        use super::*;

        /// Tall enough that the fourteen 56-point rows of `screen_of` need no scrolling.
        const SIZE: [f32; 2] = [1280.0, 1000.0];

        fn base() -> EngineSettings {
            EngineSettings {
                mouse_sensitivity: 2.5,
                mouse_smoothing: false,
                invert_mouse_y: false,
                master_volume: 10.0,
                sound_effect_volume: 8.0,
                music_volume: 6.0,
                ui_volume: 7.0,
                shadow_quality: 3,
                ssao_quality: 3,
                effects_quality: 3,
                window_mode: WindowMode::Windowed,
                resolution: (1920, 1080),
            }
        }

        /// The twelve rows, Apply, then Back: fourteen rows, so the whole column fits on this
        /// module's `SIZE`.
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
            step(&mut stack, EngineSetting::EffectsQuality, &[Key::ArrowLeft]);
            step(
                &mut stack,
                EngineSetting::MasterVolume,
                &[Key::ArrowLeft, Key::ArrowLeft],
            );
            let drawn: Vec<String> = stack.texts(SIZE).into_iter().map(|t| t.text).collect();

            stack.pop();
            stack.push(screen_of(base()));
            let applied = apply(&mut stack);

            assert!(drawn.contains(&"9.0".to_owned()), "{drawn:?}");
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
        fn the_effects_quality_row_steps_and_clamps_like_a_quality_choice() {
            let low = EngineSettings {
                effects_quality: 0,
                ..base()
            };
            let mut stack = stack_of(screen_of(low));

            step(
                &mut stack,
                EngineSetting::EffectsQuality,
                &[Key::ArrowLeft, Key::ArrowRight],
            );
            let applied = apply(&mut stack);

            assert_eq!(applied.effects_quality, 1);
            assert_eq!(applied.ssao_quality, base().ssao_quality);
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

    mod game_rows {
        use std::collections::BTreeMap;

        use moho_core::prefs::WindowMode;

        use super::*;

        fn engine_base() -> EngineSettings {
            EngineSettings {
                mouse_sensitivity: 2.5,
                mouse_smoothing: false,
                invert_mouse_y: false,
                master_volume: 10.0,
                sound_effect_volume: 8.0,
                music_volume: 6.0,
                ui_volume: 7.0,
                shadow_quality: 3,
                ssao_quality: 3,
                effects_quality: 3,
                window_mode: WindowMode::Windowed,
                resolution: (1920, 1080),
            }
        }

        fn slider(key: &str, label: &str, value: i32) -> GameSlider {
            GameSlider {
                key: key.to_owned(),
                label: label.to_owned(),
                min: 0,
                max: 100,
                step: 5,
                unit: "%".to_owned(),
                value,
            }
        }

        fn head_bob(value: i32) -> GameSlider {
            slider("head_bob", "Head bob", value)
        }

        /// The sliders, then Apply and Back; an engine volume row leads when `engine` is given.
        fn screen_of(sliders: Vec<GameSlider>, engine: Option<EngineSettings>) -> MenuScreen<Act> {
            let mut items: Vec<MenuItem<Act>> = Vec::new();
            if engine.is_some() {
                items.push(MenuItem::Setting(EngineSetting::MusicVolume));
            }
            items.extend(sliders.into_iter().map(MenuItem::GameSlider));
            items.push(MenuItem::Apply);
            items.push(button("Back", Act::Back));
            MenuScreen {
                title: Some("Settings".to_owned()),
                items,
                back: Some(Act::Back),
                settings: engine,
            }
        }

        fn game_values(pairs: &[(&str, i32)]) -> BTreeMap<String, i32> {
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), *value))
                .collect()
        }

        fn row_of(stack: &MenuStack<Act>, item: MenuItem<Act>) -> usize {
            let screen = stack.top().expect("a screen is open");
            screen
                .items
                .iter()
                .position(|candidate| *candidate == item)
                .expect("the screen lists the item")
        }

        fn game_row(stack: &MenuStack<Act>, key: &str) -> usize {
            let screen = stack.top().expect("a screen is open");
            screen
                .items
                .iter()
                .position(|item| matches!(item, MenuItem::GameSlider(s) if s.key == key))
                .expect("the screen lists the game slider")
        }

        fn rect_of(stack: &MenuStack<Act>, row: usize) -> Rect {
            layout(stack.top().expect("a screen is open"), SIZE)[row]
        }

        fn focus(stack: &mut MenuStack<Act>, row: usize) {
            let rect = rect_of(stack, row);
            stack.hover(
                [
                    (rect.min[0] + rect.max[0]) / 2.0,
                    (rect.min[1] + rect.max[1]) / 2.0,
                ],
                SIZE,
            );
        }

        fn step_game(stack: &mut MenuStack<Act>, key: &str, keys: &[Key]) {
            let row = game_row(stack, key);
            focus(stack, row);
            for &pressed in keys {
                press(stack, pressed);
            }
        }

        /// Presses the mouse `fraction` of the way along the band of the `key` slider.
        fn press_band(stack: &mut MenuStack<Act>, key: &str, fraction: f32) {
            let rect = rect_of(stack, game_row(stack, key));
            let x = rect.min[0] + fraction * (rect.max[0] - rect.min[0]);
            stack.click([x, (rect.min[1] + rect.max[1]) / 2.0], SIZE);
        }

        fn drag_band(stack: &mut MenuStack<Act>, key: &str, fraction: f32) {
            let rect = rect_of(stack, game_row(stack, key));
            let x = rect.min[0] + fraction * (rect.max[0] - rect.min[0]);
            stack.drag([x, (rect.min[1] + rect.max[1]) / 2.0], SIZE);
        }

        fn release(stack: &mut MenuStack<Act>) {
            stack.on_key(Input::Mouse(MouseButton::Left), false, false);
        }

        fn apply_events(stack: &mut MenuStack<Act>) -> Vec<MenuEvent<Act>> {
            let row = row_of(stack, MenuItem::Apply);
            focus(stack, row);
            press(stack, Key::Enter);
            stack.take_events()
        }

        fn applied_game(stack: &mut MenuStack<Act>) -> BTreeMap<String, i32> {
            let mut games: Vec<_> = apply_events(stack)
                .into_iter()
                .filter_map(|event| match event {
                    MenuEvent::GameSettingsApplied(values) => Some(values),
                    _ => None,
                })
                .collect();
            assert_eq!(
                games.len(),
                1,
                "Apply sends the game values once: {games:?}"
            );
            games.remove(0)
        }

        /// The texts on the `label` row: the label itself, then the value shown beside it.
        fn row_texts_of(stack: &MenuStack<Act>, label: &str) -> (ScreenText, Option<ScreenText>) {
            let texts = stack.texts(SIZE);
            let name = texts
                .iter()
                .find(|t| t.text == label)
                .unwrap_or_else(|| panic!("{label:?} is not drawn in {texts:?}"))
                .clone();
            let value = texts
                .iter()
                .find(|t| t.offset[1] == name.offset[1] && t.anchor == Anchor::Right)
                .cloned();
            (name, value)
        }

        fn shown_value(stack: &MenuStack<Act>, label: &str) -> Option<String> {
            row_texts_of(stack, label).1.map(|value| value.text)
        }

        #[test]
        fn apply_sends_game_values_beside_engine_values() {
            let mut stack = stack_of(screen_of(
                vec![head_bob(50), slider("sway", "Sway", 20)],
                Some(engine_base()),
            ));
            step_game(&mut stack, "head_bob", &[Key::ArrowLeft; 10]);
            let music = row_of(&stack, MenuItem::Setting(EngineSetting::MusicVolume));
            focus(&mut stack, music);
            press(&mut stack, Key::ArrowLeft);

            let events = apply_events(&mut stack);

            assert_eq!(
                events,
                [
                    MenuEvent::SettingsApplied(EngineSettings {
                        music_volume: 5.5,
                        ..engine_base()
                    }),
                    MenuEvent::GameSettingsApplied(game_values(&[("head_bob", 0), ("sway", 20)])),
                ]
            );
        }

        #[test]
        fn apply_on_a_screen_without_engine_rows_sends_only_game_values() {
            let mut stack = stack_of(screen_of(vec![head_bob(50)], None));
            step_game(&mut stack, "head_bob", &[Key::ArrowLeft; 10]);

            let events = apply_events(&mut stack);

            assert_eq!(
                events,
                [MenuEvent::GameSettingsApplied(game_values(&[(
                    "head_bob", 0
                )]))]
            );
        }

        #[test]
        fn pop_discards_staged_game_values() {
            let mut stack = stack_of(main_screen());
            stack.push(screen_of(vec![head_bob(50)], Some(engine_base())));
            step_game(&mut stack, "head_bob", &[Key::ArrowLeft; 10]);
            let staged = shown_value(&stack, "Head bob");

            stack.pop();
            let after_pop = stack.take_events();
            stack.push(screen_of(vec![head_bob(50)], Some(engine_base())));
            let reshown = shown_value(&stack, "Head bob");
            let events = apply_events(&mut stack);

            assert_eq!(staged.as_deref(), Some("0 %"));
            assert_eq!(after_pop, []);
            assert_eq!(reshown.as_deref(), Some("50 %"));
            assert_eq!(
                events,
                [
                    MenuEvent::SettingsApplied(engine_base()),
                    MenuEvent::GameSettingsApplied(game_values(&[("head_bob", 50)])),
                ]
            );
        }

        #[test]
        fn arrows_step_and_clamp_a_game_slider() {
            let mut stack = stack_of(screen_of(vec![head_bob(95)], None));

            step_game(&mut stack, "head_bob", &[Key::ArrowRight, Key::ArrowRight]);
            let at_top = shown_value(&stack, "Head bob");
            press(&mut stack, Key::Enter);
            let after_enter = shown_value(&stack, "Head bob");
            press_band(&mut stack, "head_bob", 0.25);
            let clicked = shown_value(&stack, "Head bob");
            release(&mut stack);
            step_game(&mut stack, "head_bob", &[Key::ArrowLeft]);
            let stepped_down = shown_value(&stack, "Head bob");
            let mut low = stack_of(screen_of(vec![head_bob(5)], None));
            step_game(&mut low, "head_bob", &[Key::ArrowLeft, Key::ArrowLeft]);
            let at_bottom = shown_value(&low, "Head bob");

            assert_eq!(at_bottom.as_deref(), Some("0 %"));
            assert_eq!(at_top.as_deref(), Some("100 %"));
            assert_eq!(after_enter.as_deref(), Some("100 %"));
            assert_eq!(clicked.as_deref(), Some("25 %"));
            assert_eq!(stepped_down.as_deref(), Some("20 %"));
            assert_eq!(stack.take_events(), [], "a slider chooses nothing");
        }

        #[test]
        fn click_and_drag_set_a_game_slider() {
            let mut stack = stack_of(screen_of(vec![head_bob(10)], None));
            let band = rect_of(&stack, game_row(&stack, "head_bob"));

            press_band(&mut stack, "head_bob", 0.3);
            let pressed = shown_value(&stack, "Head bob");
            let dragging = stack.is_dragging();
            drag_band(&mut stack, "head_bob", 0.7);
            let dragged = shown_value(&stack, "Head bob");
            let beyond = band.max[0] + 500.0;
            stack.drag([beyond, (band.min[1] + band.max[1]) / 2.0], SIZE);
            let past_the_end = shown_value(&stack, "Head bob");
            stack.drag(
                [band.min[0] - 500.0, (band.min[1] + band.max[1]) / 2.0],
                SIZE,
            );
            let before_the_start = shown_value(&stack, "Head bob");
            drag_band(&mut stack, "head_bob", 1.0);
            release(&mut stack);
            let still_dragging = stack.is_dragging();
            drag_band(&mut stack, "head_bob", 0.1);
            let after_release = shown_value(&stack, "Head bob");

            assert_eq!(pressed.as_deref(), Some("30 %"));
            assert!(dragging, "the press started a drag");
            assert_eq!(dragged.as_deref(), Some("70 %"));
            assert_eq!(past_the_end.as_deref(), Some("100 %"));
            assert_eq!(before_the_start.as_deref(), Some("0 %"));
            assert!(!still_dragging, "releasing the button ended the drag");
            assert_eq!(after_release.as_deref(), Some("100 %"));
            assert_eq!(applied_game(&mut stack), game_values(&[("head_bob", 100)]));
        }

        #[test]
        fn an_off_step_value_snaps_on_the_next_step() {
            let stepped = |value, min, key| {
                let row = GameSlider {
                    min,
                    max: 100,
                    ..head_bob(value)
                };
                let mut stack = stack_of(screen_of(vec![row], None));
                step_game(&mut stack, "head_bob", &[key]);
                applied_game(&mut stack)["head_bob"]
            };

            assert_eq!(stepped(52, 0, Key::ArrowRight), 55);
            assert_eq!(stepped(52, 0, Key::ArrowLeft), 45);
            assert_eq!(stepped(53, 0, Key::ArrowRight), 60);
            assert_eq!(stepped(4, 3, Key::ArrowRight), 8, "steps count from min");
        }

        #[test]
        fn extreme_ranges_stay_in_bounds() {
            let widest = GameSlider {
                min: i32::MIN,
                max: i32::MAX,
                step: 1,
                ..head_bob(i32::MAX)
            };
            let mut stack = stack_of(screen_of(vec![widest], None));

            step_game(&mut stack, "head_bob", &[Key::ArrowRight]);
            let right_at_max = shown_value(&stack, "Head bob");
            step_game(&mut stack, "head_bob", &[Key::ArrowLeft]);
            let left_once = shown_value(&stack, "Head bob");
            press_band(&mut stack, "head_bob", 0.5);
            drag_band(&mut stack, "head_bob", 1.0);
            let right_end = shown_value(&stack, "Head bob");
            drag_band(&mut stack, "head_bob", 0.0);
            let left_end = shown_value(&stack, "Head bob");
            release(&mut stack);
            let applied = applied_game(&mut stack);

            assert_eq!(right_at_max.as_deref(), Some("2147483647 %"));
            assert_eq!(left_once.as_deref(), Some("2147483646 %"));
            assert_eq!(right_end.as_deref(), Some("2147483647 %"));
            assert_eq!(left_end.as_deref(), Some("-2147483648 %"));
            assert_eq!(applied, game_values(&[("head_bob", i32::MIN)]));
        }

        #[test]
        fn a_negative_range_snaps_from_its_min() {
            let negative = |value| GameSlider {
                min: -10,
                max: 10,
                ..head_bob(value)
            };
            let mut stack = stack_of(screen_of(vec![negative(-3)], None));
            let mut fresh = stack_of(screen_of(vec![negative(-3)], None));

            step_game(&mut stack, "head_bob", &[Key::ArrowRight]);
            let first_right = shown_value(&stack, "Head bob");
            step_game(&mut stack, "head_bob", &[Key::ArrowRight]);
            let second_right = shown_value(&stack, "Head bob");
            step_game(&mut fresh, "head_bob", &[Key::ArrowLeft]);
            let left = shown_value(&fresh, "Head bob");

            assert_eq!(first_right.as_deref(), Some("0 %"));
            assert_eq!(second_right.as_deref(), Some("5 %"));
            assert_eq!(left.as_deref(), Some("-10 %"));
        }

        #[test]
        fn inverted_range_and_zero_step_do_not_panic() {
            let inverted = GameSlider {
                min: 10,
                max: 0,
                ..slider("inverted", "Inverted", 5)
            };
            let no_step = GameSlider {
                step: 0,
                ..slider("no_step", "No step", 40)
            };
            let mut stack = stack_of(screen_of(vec![inverted, no_step], None));

            step_game(&mut stack, "inverted", &[Key::ArrowLeft, Key::ArrowRight]);
            let after_arrows = shown_value(&stack, "Inverted");
            press_band(&mut stack, "inverted", 0.5);
            drag_band(&mut stack, "inverted", 0.2);
            release(&mut stack);
            step_game(&mut stack, "no_step", &[Key::ArrowRight]);
            let stepped = applied_game(&mut stack);
            press_band(&mut stack, "no_step", 0.5);
            release(&mut stack);
            let clicked = applied_game(&mut stack);

            assert_eq!(after_arrows.as_deref(), Some("10 %"));
            assert_eq!(stepped, game_values(&[("inverted", 10), ("no_step", 41)]));
            assert_eq!(clicked, game_values(&[("inverted", 10), ("no_step", 50)]));
        }

        #[test]
        fn rows_sharing_a_key_show_and_edit_one_value() {
            let first = head_bob(50);
            let second = slider("head_bob", "Head bob again", 50);
            let mut stack = stack_of(screen_of(vec![first, second], None));

            step_game(&mut stack, "head_bob", &[Key::ArrowRight]);
            let first_shown = shown_value(&stack, "Head bob");
            let second_shown = shown_value(&stack, "Head bob again");
            let applied = applied_game(&mut stack);

            assert_eq!(first_shown.as_deref(), Some("55 %"));
            assert_eq!(second_shown.as_deref(), Some("55 %"));
            assert_eq!(applied, game_values(&[("head_bob", 55)]));
        }

        #[test]
        fn a_game_slider_shows_label_and_value_with_unit() {
            let bare = GameSlider {
                unit: String::new(),
                ..slider("sway", "Sway", 7)
            };
            let stack = stack_of(screen_of(vec![head_bob(50), bare], None));

            let (label, value) = row_texts_of(&stack, "Head bob");
            let (bare_label, bare_value) = row_texts_of(&stack, "Sway");

            let rect = rect_of(&stack, game_row(&stack, "head_bob"));
            let value = value.expect("the row shows a value");
            assert_eq!(value.text, "50 %");
            assert_eq!(label.anchor, Anchor::Left);
            assert_eq!(label.offset[0], rect.min[0]);
            assert_eq!(value.anchor, Anchor::Right);
            assert_eq!(value.offset[0], rect.max[0] - SIZE[0]);
            assert_eq!(value.offset[1], label.offset[1]);
            assert_eq!(
                bare_value.map(|value| value.text).as_deref(),
                Some("7"),
                "no unit, no trailing space"
            );
            assert_eq!(bare_label.anchor, Anchor::Left);
        }

        #[test]
        fn a_second_apply_sends_the_applied_values() {
            let mut stack = stack_of(screen_of(vec![head_bob(50)], None));
            step_game(&mut stack, "head_bob", &[Key::ArrowLeft; 10]);

            let first = applied_game(&mut stack);
            let baseline = stack.top().map(|screen| screen.items.clone());
            let second = applied_game(&mut stack);
            step_game(&mut stack, "head_bob", &[Key::ArrowRight]);
            let third = applied_game(&mut stack);

            assert_eq!(first, game_values(&[("head_bob", 0)]));
            assert_eq!(
                baseline.and_then(|items| items.into_iter().find_map(|item| match item {
                    MenuItem::GameSlider(slider) => Some(slider.value),
                    _ => None,
                })),
                Some(0),
                "Apply makes the staged value the row's baseline"
            );
            assert_eq!(second, game_values(&[("head_bob", 0)]));
            assert_eq!(third, game_values(&[("head_bob", 5)]));
        }
    }

    fn lists_of_acts(rows: [usize; 2]) -> crate::lists::ItemLists<Act> {
        use crate::lists::{ItemList, ItemLists, ListRow};
        let list = |id: u32, title: &str, count: usize, item: Act| ItemList {
            id: ListId(id),
            title: title.to_owned(),
            rows: (0..count)
                .map(|i| ListRow {
                    label: format!("{title} {i}"),
                    count: Some(1),
                    tooltip: None,
                    item,
                    actions: vec![],
                })
                .collect(),
        };
        ItemLists {
            lists: vec![
                list(10, "Carried", rows[0], Act::Play),
                list(20, "Container", rows[1], Act::Quit),
            ],
            back: Some(Act::Back),
        }
    }

    fn move_requests(stack: &mut MenuStack<Act>) -> Vec<MenuEvent<Act>> {
        stack.take_events()
    }

    #[test]
    fn lists_push_over_a_menu_and_pop_back_to_its_focus() {
        let lists = lists_of_acts([2, 1]);
        let mut stack = stack_of(main_screen());
        press(&mut stack, Key::ArrowDown);

        stack.push_lists(lists.clone());
        let (menu_on_top, lists_on_top, open) = (
            stack.top().is_some(),
            stack.top_lists().cloned(),
            stack.is_open(),
        );
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::ArrowRight);
        press(&mut stack, Key::Enter);
        let first_row_was_focused = move_requests(&mut stack);
        stack.push(settings_screen());
        let menu_over_lists = (stack.top_lists().is_some(), stack.top().is_some());
        stack.pop();
        let lists_again = stack.top_lists().cloned();
        stack.pop();
        let menu_again = stack.top().cloned();
        press(&mut stack, Key::Enter);
        let chosen_after = chosen(&mut stack);

        assert!(!menu_on_top, "top() is None while a lists screen is on top");
        assert_eq!(lists_on_top, Some(lists.clone()));
        assert!(open);
        assert_eq!(
            first_row_was_focused,
            [MenuEvent::MoveRequested {
                item: Act::Play,
                from: ListId(10),
                to: ListId(20)
            }]
        );
        assert_eq!(menu_over_lists, (false, true));
        assert_eq!(lists_again, Some(lists));
        assert_eq!(menu_again, Some(main_screen()));
        assert_eq!(chosen_after, [Act::Settings], "the menu kept its focus");
        assert_eq!(stack.top_lists(), None);
    }

    #[test]
    fn escape_with_nothing_held_chooses_back() {
        let with_back = lists_of_acts([1, 1]);
        let without_back = crate::lists::ItemLists {
            back: None,
            ..lists_of_acts([1, 1])
        };
        let mut stack = MenuStack::new();
        stack.push_lists(with_back.clone());

        press(&mut stack, Key::Escape);
        let escaped = move_requests(&mut stack);
        stack.on_key(Input::Key(Key::Escape), true, true);
        stack.on_key(Input::Key(Key::Escape), false, false);
        let repeat_and_release = move_requests(&mut stack);
        stack.pop();
        stack.push_lists(without_back);
        press(&mut stack, Key::Escape);
        let no_back = move_requests(&mut stack);

        assert_eq!(escaped, [MenuEvent::Chosen(Act::Back)]);
        assert_eq!(repeat_and_release, []);
        assert_eq!(no_back, []);
        assert!(stack.top_lists().is_some(), "no hidden pop");
    }

    #[test]
    fn set_lists_with_a_menu_on_top_changes_nothing() {
        let mut stack = stack_of(main_screen());

        stack.set_lists(lists_of_acts([1, 1]));

        assert_eq!(stack.top(), Some(&main_screen()));
        assert_eq!(stack.top_lists(), None);
        stack.pop();
        assert!(!stack.is_open());
    }

    #[test]
    fn visible_rows_of_a_lists_screen_cover_every_title_and_visible_rows() {
        let mut stack = MenuStack::new();
        stack.push_lists(lists_of_acts([2, 40]));
        press(&mut stack, Key::ArrowRight);
        stack.scroll(5.0);

        let rects = stack.visible_rows(SIZE);

        let rows = |left: f32, count: usize| -> Vec<Rect> {
            (0..count)
                .map(|i| Rect {
                    min: [left, 56.0 + 32.0 * i as f32],
                    max: [left + 320.0, 88.0 + 32.0 * i as f32],
                })
                .collect()
        };
        let in_band = |left: f32| {
            rects
                .iter()
                .filter(|r| r.min[0] >= left && r.max[0] <= left + 320.0 && r.max[1] <= 56.0)
                .count()
        };
        let below_band: Vec<Rect> = rects.iter().copied().filter(|r| r.max[1] > 56.0).collect();
        assert_eq!(rects.len(), 2 + 2 + 19);
        assert_eq!(in_band(288.0), 1, "the first list's title");
        assert_eq!(in_band(672.0), 1, "the second list's title");
        let mut expected = rows(288.0, 2);
        expected.extend(rows(672.0, 19));
        assert_eq!(below_band, expected);
        assert_eq!(MenuStack::<Act>::new().visible_rows(SIZE), []);
    }

    #[test]
    fn visible_rows_of_a_lists_screen_include_the_open_action_list() {
        use crate::lists::{ActionTarget, ItemList, ItemLists, ListRow, RowAction};
        let action = |label: &str, action| RowAction {
            label: label.to_owned(),
            action,
            target: ActionTarget::Itself,
        };
        let mut stack = MenuStack::new();
        stack.push_lists(ItemLists {
            lists: vec![ItemList {
                id: ListId(10),
                title: "Rig".to_owned(),
                rows: vec![ListRow {
                    label: "Magazine".to_owned(),
                    count: Some(2),
                    tooltip: None,
                    item: Act::Play,
                    actions: vec![action("Unload", Act::Quit), action("Load", Act::Settings)],
                }],
            }],
            back: Some(Act::Back),
        });
        let closed = stack.visible_rows(SIZE);

        press(&mut stack, Key::Space);
        let open = stack.visible_rows(SIZE);
        press(&mut stack, Key::Escape);
        let closed_again = stack.visible_rows(SIZE);

        let extra: Vec<Rect> = open
            .iter()
            .copied()
            .filter(|rect| !closed.contains(rect))
            .collect();
        let first = Rect {
            min: [264.0, 56.0],
            max: [464.0, 88.0],
        };
        let second = Rect {
            min: [264.0, 88.0],
            max: [464.0, 120.0],
        };
        assert_eq!(closed.len(), 2, "the title and the one row");
        assert!(closed.iter().all(|rect| open.contains(rect)));
        for action_rect in [first, second] {
            assert!(
                extra.iter().any(|rect| rect.min[0] <= action_rect.min[0]
                    && rect.min[1] <= action_rect.min[1]
                    && rect.max[0] >= action_rect.max[0]
                    && rect.max[1] >= action_rect.max[1]),
                "{action_rect:?} is not covered by {extra:?}"
            );
        }
        assert!(
            extra.iter().all(|rect| rect.min[0] >= 264.0
                && rect.min[1] >= 56.0
                && rect.max[0] <= 464.0
                && rect.max[1] <= 120.0),
            "only the action list is added: {extra:?}"
        );
        assert_eq!(closed_again, closed);
    }
}
