//! The main menu as a pure value: which item is selected, what a key or a click on it
//! chooses, and the text that draws it. It owns no window and no input state.

use moho_ui_shell::ScreenText;

/// What choosing a menu item asks the game to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Play,
    Resume,
    Settings,
    Quit,
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
            items: Vec::new(),
            selected: 0,
        }
    }

    /// The selected item's action.
    pub fn selected(&self) -> Option<MenuAction> {
        self.items.get(self.selected).copied()
    }

    /// Selects the previous item, wrapping from the first to the last.
    pub fn up(&mut self) {}

    /// Selects the next item, wrapping from the last to the first.
    pub fn down(&mut self) {}

    /// The selected item's action.
    pub fn activate(&self) -> MenuAction {
        MenuAction::Resume
    }

    /// Selects the item under `point`, in logical points from the top left of a window
    /// `screen` points across; false, selecting nothing, when the point is over no item.
    pub fn hover(&mut self, _point: [f32; 2], _screen: [f32; 2]) -> bool {
        false
    }

    /// The action of the item under `point` (see [`Menu::hover`]), if any.
    pub fn click(&self, _point: [f32; 2], _screen: [f32; 2]) -> Option<MenuAction> {
        None
    }

    /// One centre-anchored text per item, top to bottom; the selected one in its own colour.
    pub fn texts(&self) -> Vec<ScreenText> {
        Vec::new()
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
