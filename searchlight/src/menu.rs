//! The main, pause and settings menus as data for the engine's menu screens: which items they
//! list and what choosing each one asks of the game. The shell draws and drives them.

use moho_ui_shell::{EngineSetting, EngineSettings, MenuItem, MenuScreen};

/// What choosing a menu item asks the game to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Play,
    Resume,
    Settings,
    Quit,
    /// Closes the top screen.
    Back,
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

/// Titled Settings, with the engine's eleven rows starting from `settings`, then Apply and a Back
/// button; Escape is Back.
pub fn settings_screen(settings: EngineSettings) -> MenuScreen<MenuAction> {
    let mut items: Vec<MenuItem<MenuAction>> = EngineSetting::ALL
        .into_iter()
        .map(MenuItem::Setting)
        .collect();
    items.push(MenuItem::Apply);
    items.push(MenuAction::Back.button());
    MenuScreen {
        title: Some("Settings".to_owned()),
        items,
        back: Some(MenuAction::Back),
        settings: Some(settings),
    }
}
