//! The main, pause and settings menus as data for the engine's menu screens: which items they
//! list and what choosing each one asks of the game. The shell draws and drives them.

use moho_input::Action;
use moho_ui_shell::{EngineSetting, EngineSettings, GameSlider, MenuItem, MenuScreen};

use crate::controls::{ControlsDraft, label};
use crate::head_bob::HeadBobLevel;
use crate::input::SearchlightAction;

/// What choosing a menu item asks the game to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Play,
    Resume,
    Settings,
    Quit,
    /// Closes the top screen.
    Back,
    /// Opens the Controls page.
    Controls,
    /// Makes the Controls page's drafted bindings live and saves them.
    ApplyControls,
    /// The Controls row of an action, which captures its bindings and is never chosen.
    Rebind(SearchlightAction),
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
            MenuAction::Controls => "Controls",
            MenuAction::ApplyControls => "Apply",
            MenuAction::Rebind(action) => label(action),
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
    items.push(MenuAction::Controls.button());
    items.push(MenuAction::Back.button());
    MenuScreen {
        title: Some("Settings".to_owned()),
        items,
        back: Some(MenuAction::Back),
        settings: Some(settings),
    }
}

/// The Controls page's rows for `draft`: one per action with its two slots, then Apply and Back.
/// `refusal` puts a note on one action's row.
pub fn controls_rows(
    draft: &ControlsDraft,
    refusal: Option<(SearchlightAction, &str)>,
) -> Vec<MenuItem<MenuAction>> {
    let mut items: Vec<MenuItem<MenuAction>> = SearchlightAction::ALL
        .iter()
        .map(|&action| MenuItem::Control {
            label: label(action).to_owned(),
            action: MenuAction::Rebind(action),
            slots: draft.slots(action),
            note: refusal
                .filter(|(refused, _)| *refused == action)
                .map(|(_, note)| note.to_owned()),
        })
        .collect();
    items.push(MenuAction::ApplyControls.button());
    items.push(MenuAction::Back.button());
    items
}

/// The titled Controls page for `draft`; Escape is Back.
pub fn controls_screen(draft: &ControlsDraft) -> MenuScreen<MenuAction> {
    MenuScreen {
        title: Some("Controls".to_owned()),
        items: controls_rows(draft, None),
        back: Some(MenuAction::Back),
        settings: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use moho_core::prefs::Prefs;
    use moho_input::bindings::{Action, ActionBindings, Binding, Input};
    use moho_input::key::{Key, MouseButton};
    use moho_ui_shell::GameSlider;

    use super::*;
    use crate::controls::label;
    use crate::input::SearchlightAction;

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
        expected.push(MenuAction::Controls.button());
        expected.push(MenuAction::Back.button());
        assert_eq!(screen.items, expected);
        assert_eq!(screen.title.as_deref(), Some("Settings"));
        assert_eq!(screen.back, Some(MenuAction::Back));
        assert_eq!(screen.settings, Some(settings));
    }

    type ControlRow = (String, [Option<Binding>; 2]);

    fn default_bindings() -> ActionBindings<SearchlightAction> {
        let (bindings, warnings) = ActionBindings::load(&BTreeMap::new());
        assert!(warnings.is_empty());
        bindings
    }

    fn control_rows(screen: &MenuScreen<MenuAction>) -> Vec<ControlRow> {
        screen
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Control { label, slots, .. } => Some((label.clone(), *slots)),
                _ => None,
            })
            .collect()
    }

    fn slots_of(
        bindings: &ActionBindings<SearchlightAction>,
        action: SearchlightAction,
    ) -> [Option<Binding>; 2] {
        let held = bindings.get(action);
        [held.first().copied(), held.get(1).copied()]
    }

    #[test]
    fn controls_lists_every_action_with_its_primary_label() {
        let bindings = default_bindings();
        let draft = ControlsDraft::new(&bindings);

        let screen = controls_screen(&draft);

        let rows = control_rows(&screen);
        let expected: Vec<ControlRow> = SearchlightAction::ALL
            .iter()
            .map(|&action| (label(action).to_owned(), slots_of(&bindings, action)))
            .collect();
        assert_eq!(rows, expected, "every action, in order, with its slots");
        let primary_labels: Vec<Option<String>> = rows
            .iter()
            .map(|(_, slots)| slots[0].map(Binding::label))
            .collect();
        let expected_labels: Vec<Option<String>> = [
            "W",
            "S",
            "A",
            "D",
            "Spacebar",
            "Mouse Left",
            "R",
            "C",
            "F8",
            "Q",
            "E",
            "F",
        ]
        .map(|text| Some(text.to_owned()))
        .to_vec();
        assert_eq!(primary_labels, expected_labels);
        assert_eq!(screen.back, Some(MenuAction::Back));
        assert!(
            screen.items.iter().any(|item| matches!(
                item,
                MenuItem::Button {
                    action: MenuAction::Back,
                    ..
                }
            )),
            "the page has a Back button"
        );
    }

    #[test]
    fn controls_shows_the_input_a_draft_holds() {
        let mut draft = ControlsDraft::new(&default_bindings());
        draft.capture(SearchlightAction::Reload, Input::Key(Key::T));
        draft.capture(SearchlightAction::Fire, Input::Mouse(MouseButton::Right));

        let rows = control_rows(&controls_screen(&draft));

        let slots = |action| {
            let at = SearchlightAction::ALL.iter().position(|&a| a == action);
            rows[at.expect("the action is listed")].1
        };
        assert_eq!(
            slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::T)), None]
        );
        assert_eq!(
            slots(SearchlightAction::Fire),
            [Some(Binding::mouse(MouseButton::Right)), None]
        );
        assert_eq!(
            slots(SearchlightAction::CheckMagazine),
            [
                Some(Binding::key(Key::C)),
                Some(Binding::key(Key::R).held())
            ]
        );
    }
}
