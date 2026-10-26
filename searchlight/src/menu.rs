//! The main, pause and settings menus as data for the engine's menu screens: which items they
//! list and what choosing each one asks of the game. The shell draws and drives them.

use moho_ui_shell::{MenuItem, MenuScreen};

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
    }
}

/// Titled Settings, with one Back button; Escape is Back.
pub fn settings_screen() -> MenuScreen<MenuAction> {
    MenuScreen {
        title: Some("Settings".to_owned()),
        items: vec![MenuAction::Back.button()],
        back: Some(MenuAction::Back),
    }
}
