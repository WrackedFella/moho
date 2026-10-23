//! The main and pause menus as a pure value: which item is selected, what a key or a click on it
//! chooses, and the text that draws it. It owns no window and no input state.

use moho_ui_shell::{Anchor, ScreenText};

/// Font size of every row.
const ROW_SIZE: f32 = 36.0;
/// Distance between row centres, in logical points; at least [`ROW_SIZE`] so rows do not overlap.
const ROW_PITCH: f32 = 56.0;
/// Width of a row's clickable band, centred on the screen.
const ROW_WIDTH: f32 = 240.0;
const SELECTED_COLOR: [u8; 4] = [255, 220, 80, 255];
const COLOR: [u8; 4] = [220, 220, 220, 255];

/// What choosing a menu item asks the game to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Play,
    Resume,
    Settings,
    Quit,
}

impl MenuAction {
    fn label(self) -> &'static str {
        match self {
            MenuAction::Play => "Play",
            MenuAction::Resume => "Resume",
            MenuAction::Settings => "Settings",
            MenuAction::Quit => "Quit",
        }
    }
}

/// A column of items, one of them selected, laid out around the screen centre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    items: Vec<MenuAction>,
    selected: usize,
}

impl Menu {
    /// Play, Settings and Quit, in that order, with Play selected.
    pub fn main() -> Self {
        Self {
            items: vec![MenuAction::Play, MenuAction::Settings, MenuAction::Quit],
            selected: 0,
        }
    }

    pub fn pause() -> Self {
        Self {
            items: vec![MenuAction::Resume, MenuAction::Settings, MenuAction::Quit],
            selected: 0,
        }
    }

    /// The selected item's action.
    pub fn selected(&self) -> Option<MenuAction> {
        self.items.get(self.selected).copied()
    }

    /// Selects the previous item, wrapping from the first to the last.
    pub fn up(&mut self) {
        self.selected = (self.selected + self.items.len() - 1) % self.items.len();
    }

    /// Selects the next item, wrapping from the last to the first.
    pub fn down(&mut self) {
        self.selected = (self.selected + 1) % self.items.len();
    }

    /// The selected item's action.
    pub fn activate(&self) -> MenuAction {
        self.items[self.selected]
    }

    /// Selects the item under `point`, in logical points from the top left of a window
    /// `screen` points across; false, selecting nothing, when the point is over no item.
    pub fn hover(&mut self, point: [f32; 2], screen: [f32; 2]) -> bool {
        let Some(row) = self.row_at(point, screen) else {
            return false;
        };
        self.selected = row;
        true
    }

    /// The action of the item under `point` (see [`Menu::hover`]), if any.
    pub fn click(&self, point: [f32; 2], screen: [f32; 2]) -> Option<MenuAction> {
        self.row_at(point, screen).map(|row| self.items[row])
    }

    /// One centre-anchored text per item, top to bottom; the selected one in its own colour.
    pub fn texts(&self) -> Vec<ScreenText> {
        self.items
            .iter()
            .enumerate()
            .map(|(row, item)| ScreenText {
                text: item.label().to_owned(),
                anchor: Anchor::Center,
                offset: [0.0, self.row_offset(row)],
                size: ROW_SIZE,
                color: if row == self.selected {
                    SELECTED_COLOR
                } else {
                    COLOR
                },
            })
            .collect()
    }

    /// Vertical offset of row `row`'s centre from the screen centre.
    fn row_offset(&self, row: usize) -> f32 {
        (row as f32 - (self.items.len() - 1) as f32 / 2.0) * ROW_PITCH
    }

    fn row_at(&self, point: [f32; 2], screen: [f32; 2]) -> Option<usize> {
        if (point[0] - screen[0] / 2.0).abs() >= ROW_WIDTH / 2.0 {
            return None;
        }
        (0..self.items.len()).find(|&row| {
            (point[1] - screen[1] / 2.0 - self.row_offset(row)).abs() < ROW_PITCH / 2.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: [f32; 2] = [1280.0, 720.0];

    /// Where row `index` is drawn on a window `screen` points across.
    fn row_centre(menu: &Menu, index: usize, screen: [f32; 2]) -> [f32; 2] {
        let texts = menu.texts();
        assert!(index < texts.len(), "no row {index} in {texts:?}");
        let offset = texts[index].offset;
        [screen[0] / 2.0 + offset[0], screen[1] / 2.0 + offset[1]]
    }

    /// The actions reached by pressing down from the first item, the first included.
    fn actions_in_order(menu: &mut Menu) -> Vec<MenuAction> {
        let mut actions = Vec::new();
        for _ in 0..3 {
            actions.push(menu.activate());
            menu.down();
        }
        actions
    }

    #[test]
    fn the_main_menu_lists_play_settings_quit_in_order() {
        let mut menu = Menu::main();

        let labels: Vec<String> = menu.texts().into_iter().map(|t| t.text).collect();
        let actions = actions_in_order(&mut menu);

        assert_eq!(labels, ["Play", "Settings", "Quit"]);
        assert_eq!(
            actions,
            [MenuAction::Play, MenuAction::Settings, MenuAction::Quit]
        );
        assert_eq!(Menu::main().selected(), Some(MenuAction::Play));
    }

    #[test]
    fn the_pause_menu_lists_resume_settings_quit_in_order() {
        let mut menu = Menu::pause();

        let labels: Vec<String> = menu.texts().into_iter().map(|t| t.text).collect();
        let actions = actions_in_order(&mut menu);

        assert_eq!(labels, ["Resume", "Settings", "Quit"]);
        assert_eq!(
            actions,
            [MenuAction::Resume, MenuAction::Settings, MenuAction::Quit]
        );
        assert_eq!(Menu::pause().selected(), Some(MenuAction::Resume));
    }

    #[test]
    fn down_wraps_from_the_last_item_and_up_from_the_first() {
        let mut menu = Menu::main();

        menu.up();
        let up_from_first = menu.selected();
        menu.down();
        let down_from_last = menu.selected();
        menu.down();
        menu.down();
        menu.down();
        let a_full_turn = menu.selected();

        assert_eq!(up_from_first, Some(MenuAction::Quit));
        assert_eq!(down_from_last, Some(MenuAction::Play));
        assert_eq!(a_full_turn, Some(MenuAction::Play));
    }

    #[test]
    fn activate_returns_the_selected_action() {
        let mut menu = Menu::main();

        let first = menu.activate();
        menu.down();
        let second = menu.activate();
        menu.down();
        let third = menu.activate();

        assert_eq!(first, MenuAction::Play);
        assert_eq!(second, MenuAction::Settings);
        assert_eq!(third, MenuAction::Quit);
    }

    #[test]
    fn hovering_an_item_selects_it_and_a_point_outside_every_row_hits_nothing() {
        let mut menu = Menu::main();
        let settings = row_centre(&menu, 1, SCREEN);
        let pitch = row_centre(&menu, 1, SCREEN)[1] - row_centre(&menu, 0, SCREEN)[1];
        let below_last = [settings[0], row_centre(&menu, 2, SCREEN)[1] + pitch];
        let above_first = [settings[0], row_centre(&menu, 0, SCREEN)[1] - pitch];

        let over_settings = menu.hover(settings, SCREEN);
        let selected = menu.selected();
        let misses = [
            menu.hover([0.0, 0.0], SCREEN),
            menu.hover([SCREEN[0] - 1.0, settings[1]], SCREEN),
            menu.hover(below_last, SCREEN),
            menu.hover(above_first, SCREEN),
        ];

        assert!(over_settings);
        assert_eq!(selected, Some(MenuAction::Settings));
        assert_eq!(misses, [false; 4]);
        assert_eq!(
            menu.selected(),
            Some(MenuAction::Settings),
            "a miss keeps the selection"
        );
    }

    #[test]
    fn a_click_on_an_item_returns_its_action_and_a_click_outside_returns_none() {
        let menu = Menu::main();
        let rows: Vec<Option<MenuAction>> = (0..3)
            .map(|row| menu.click(row_centre(&menu, row, SCREEN), SCREEN))
            .collect();
        let small = [800.0, 600.0];

        let on_a_smaller_screen = menu.click(row_centre(&menu, 2, small), small);
        let outside = menu.click([0.0, 0.0], SCREEN);

        assert_eq!(
            rows,
            [
                Some(MenuAction::Play),
                Some(MenuAction::Settings),
                Some(MenuAction::Quit)
            ]
        );
        assert_eq!(on_a_smaller_screen, Some(MenuAction::Quit));
        assert_eq!(outside, None);
    }

    #[test]
    fn rows_are_centred_on_the_screen_one_pitch_apart() {
        let menu = Menu::main();

        let offsets: Vec<[f32; 2]> = menu.texts().iter().map(|text| text.offset).collect();

        assert_eq!(offsets, [[0.0, -ROW_PITCH], [0.0, 0.0], [0.0, ROW_PITCH]]);
    }

    #[test]
    fn only_the_selected_row_carries_the_selected_colour() {
        let mut menu = Menu::main();
        let colours =
            |menu: &Menu| -> Vec<[u8; 4]> { menu.texts().iter().map(|text| text.color).collect() };

        let first = colours(&menu);
        menu.down();
        let second = colours(&menu);

        assert_eq!(first, [SELECTED_COLOR, COLOR, COLOR]);
        assert_eq!(second, [COLOR, SELECTED_COLOR, COLOR]);
    }

    #[test]
    fn a_click_is_a_hit_just_inside_a_row_band_and_a_miss_on_its_edge() {
        let menu = Menu::main();
        let [cx, cy] = [SCREEN[0] / 2.0, SCREEN[1] / 2.0];
        let half_width = ROW_WIDTH / 2.0;
        let half_pitch = ROW_PITCH / 2.0;
        let inside = 0.5;

        let just_inside_right = menu.click([cx + half_width - inside, cy], SCREEN);
        let on_right_edge = menu.click([cx + half_width, cy], SCREEN);
        let just_inside_left = menu.click([cx - half_width + inside, cy], SCREEN);
        let on_left_edge = menu.click([cx - half_width, cy], SCREEN);
        let below_middle_inside = menu.click([cx, cy + half_pitch - inside], SCREEN);
        let below_middle_edge = menu.click([cx, cy + half_pitch], SCREEN);
        let above_middle_inside = menu.click([cx, cy - half_pitch + inside], SCREEN);
        let above_middle_edge = menu.click([cx, cy - half_pitch], SCREEN);
        let last_row_bottom_inside = menu.click([cx, cy + ROW_PITCH + half_pitch - inside], SCREEN);
        let last_row_bottom_edge = menu.click([cx, cy + ROW_PITCH + half_pitch], SCREEN);
        let first_row_top_inside = menu.click([cx, cy - ROW_PITCH - half_pitch + inside], SCREEN);
        let first_row_top_edge = menu.click([cx, cy - ROW_PITCH - half_pitch], SCREEN);

        assert_eq!(just_inside_right, Some(MenuAction::Settings));
        assert_eq!(on_right_edge, None);
        assert_eq!(just_inside_left, Some(MenuAction::Settings));
        assert_eq!(on_left_edge, None);
        assert_eq!(below_middle_inside, Some(MenuAction::Settings));
        assert_eq!(below_middle_edge, None);
        assert_eq!(above_middle_inside, Some(MenuAction::Settings));
        assert_eq!(above_middle_edge, None);
        assert_eq!(last_row_bottom_inside, Some(MenuAction::Quit));
        assert_eq!(last_row_bottom_edge, None);
        assert_eq!(first_row_top_inside, Some(MenuAction::Play));
        assert_eq!(first_row_top_edge, None);
    }

    #[test]
    fn hit_testing_follows_the_screen_centre_of_a_non_default_window() {
        let menu = Menu::main();
        let screen = [1000.0, 400.0];

        let at_its_centre = menu.click([500.0, 200.0], screen);
        let at_the_default_centre = menu.click([SCREEN[0] / 2.0, SCREEN[1] / 2.0], screen);

        assert_eq!(at_its_centre, Some(MenuAction::Settings));
        assert_eq!(at_the_default_centre, None);
    }

    #[test]
    fn the_selected_item_is_drawn_in_a_different_colour_and_rows_do_not_overlap() {
        let mut menu = Menu::main();
        let colours =
            |menu: &Menu| -> Vec<[u8; 4]> { menu.texts().iter().map(|text| text.color).collect() };

        let first_selected = colours(&menu);
        menu.down();
        let second_selected = colours(&menu);
        let texts = menu.texts();

        assert_eq!(first_selected.len(), 3);
        assert_ne!(first_selected[0], first_selected[1]);
        assert_eq!(first_selected[1], first_selected[2]);
        assert_ne!(second_selected[1], second_selected[0]);
        assert_eq!(second_selected[0], second_selected[2]);
        assert_eq!(second_selected[1], first_selected[0]);
        assert!(texts.iter().all(|t| t.size == texts[0].size));
        for pair in texts.windows(2) {
            assert!(
                pair[1].offset[1] - pair[0].offset[1] >= pair[0].size,
                "rows overlap: {pair:?}"
            );
        }
    }
}
