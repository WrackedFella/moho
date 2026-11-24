//! The main, pause and settings menus as data for the engine's menu screens: which items they
//! list and what choosing each one asks of the game. The shell draws and drives them.

use moho_ui_shell::{EngineSetting, EngineSettings, GameSlider, MenuItem, MenuScreen};

use crate::head_bob::HeadBobLevel;

/// What choosing a menu item asks the game to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Play,
    Resume,
    Settings,
    Quit,
    /// Closes the top screen.
    Back,
    /// The item with this id in the item lists.
    Item(u32),
}

impl MenuAction {
    fn button(self) -> MenuItem<MenuAction> {
        MenuItem::Button {
            label: self.label().to_owned(),
            action: self,
        }
    }

    fn label(self) -> &'static str {
        match self {
            MenuAction::Play => "Play",
            MenuAction::Resume => "Resume",
            MenuAction::Settings => "Settings",
            MenuAction::Quit => "Quit",
            MenuAction::Back => "Back",
            MenuAction::Item(_) => "Item",
        }
    }
}

/// Play, Settings and Quit, in that order; Escape does nothing.
pub fn main_screen() -> MenuScreen<MenuAction> {
    MenuScreen {
        title: None,
        items: vec![
            MenuAction::Play.button(),
            MenuAction::Settings.button(),
            MenuAction::Quit.button(),
        ],
        back: None,
        settings: None,
    }
}

/// Resume, Settings and Quit, in that order; Escape resumes.
pub fn pause_screen() -> MenuScreen<MenuAction> {
    MenuScreen {
        title: None,
        items: vec![
            MenuAction::Resume.button(),
            MenuAction::Settings.button(),
            MenuAction::Quit.button(),
        ],
        back: Some(MenuAction::Resume),
        settings: None,
    }
}

/// The Head bob slider row, starting at `level`.
pub fn head_bob_row(level: HeadBobLevel) -> MenuItem<MenuAction> {
    MenuItem::GameSlider(GameSlider {
        key: HeadBobLevel::KEY.to_owned(),
        label: "Head bob".to_owned(),
        min: HeadBobLevel::MIN,
        max: HeadBobLevel::MAX,
        step: HeadBobLevel::STEP,
        unit: "%".to_owned(),
        value: level.percent(),
    })
}

/// Titled Settings, with the engine's rows starting from `settings`, the Head bob row starting
/// at `head_bob`, then Apply and a Back button; Escape is Back.
pub fn settings_screen(settings: EngineSettings, head_bob: HeadBobLevel) -> MenuScreen<MenuAction> {
    let mut items: Vec<MenuItem<MenuAction>> = EngineSetting::ALL
        .into_iter()
        .map(MenuItem::Setting)
        .collect();
    items.push(head_bob_row(head_bob));
    items.push(MenuItem::Apply);
    items.push(MenuAction::Back.button());
    MenuScreen {
        title: Some("Settings".to_owned()),
        items,
        back: Some(MenuAction::Back),
        settings: Some(settings),
    }
}

#[cfg(test)]
mod tests {
    use moho_core::prefs::Prefs;
    use moho_ui_shell::GameSlider;

    use super::*;

    fn level(percent: i32) -> HeadBobLevel {
        HeadBobLevel::new(percent).expect("a percentage on the grid")
    }

    #[test]
    fn the_head_bob_row_is_a_zero_to_a_hundred_percent_slider_in_steps_of_five() {
        let row = head_bob_row(level(35));

        assert_eq!(
            row,
            MenuItem::GameSlider(GameSlider {
                key: "head_bob".to_owned(),
                label: "Head bob".to_owned(),
                min: 0,
                max: 100,
                step: 5,
                unit: "%".to_owned(),
                value: 35,
            })
        );
    }

    #[test]
    fn the_head_bob_row_starts_at_each_level_it_is_given() {
        for percent in [0, 5, 50, 100] {
            let MenuItem::GameSlider(slider) = head_bob_row(level(percent)) else {
                panic!("the Head bob row is a game slider");
            };

            assert_eq!(slider.value, percent);
            assert_eq!(slider.key, HeadBobLevel::KEY);
        }
    }

    #[test]
    fn settings_puts_the_head_bob_row_after_the_engine_rows_and_before_apply() {
        let settings = EngineSettings::from_prefs(&Prefs::default());

        let screen = settings_screen(settings, level(30));

        let mut expected: Vec<MenuItem<MenuAction>> = EngineSetting::ALL
            .into_iter()
            .map(MenuItem::Setting)
            .collect();
        expected.push(head_bob_row(level(30)));
        expected.push(MenuItem::Apply);
        expected.push(MenuAction::Back.button());
        assert_eq!(screen.items, expected);
        assert_eq!(screen.title.as_deref(), Some("Settings"));
        assert_eq!(screen.back, Some(MenuAction::Back));
        assert_eq!(screen.settings, Some(settings));
    }
}
