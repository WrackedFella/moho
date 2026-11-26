//! The main, pause and settings menus as data for the engine's menu screens: which items they
//! list and what choosing each one asks of the game. The shell draws and drives them.

use moho_input::Action;
use moho_ui_shell::{EngineSetting, EngineSettings, GameSlider, MenuItem, MenuScreen};

use crate::controls::{ControlsDraft, label};
use crate::debug_settings::GameRows;
use crate::head_bob::HeadBobLevel;
use crate::input::SearchlightAction;
use crate::item_lists::ItemAction;

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
    /// Puts the Controls page's draft back on every action's defaults; nothing is live until Apply.
    ResetControls,
    /// The Controls row of an action, which captures its bindings and is never chosen.
    Rebind(SearchlightAction),
    /// Answers the conflict dialog by moving the input to the capturing action.
    Replace,
    /// Answers the conflict dialog by keeping both actions as they were.
    Cancel,
    /// The item with this id in the item lists.
    Item(u32),
    /// An action a row of the item lists offers.
    ItemAction(ItemAction),
    /// Opens a Settings page.
    Page(SettingsPage),
}

/// A category of the Settings screen, in the order they are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsPage {
    Controls,
    Mouse,
    Audio,
    Graphics,
    Display,
    Game,
}

impl SettingsPage {
    pub const ALL: [SettingsPage; 6] = [
        SettingsPage::Controls,
        SettingsPage::Mouse,
        SettingsPage::Audio,
        SettingsPage::Graphics,
        SettingsPage::Display,
        SettingsPage::Game,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsPage::Controls => "Controls",
            SettingsPage::Mouse => "Mouse",
            SettingsPage::Audio => "Audio",
            SettingsPage::Graphics => "Graphics",
            SettingsPage::Display => "Display",
            SettingsPage::Game => "Game",
        }
    }
}

/// The page `setting` is shown on. The match has no wildcard arm, so an engine row added later
/// does not compile until it is placed on a page.
pub fn page_of(setting: EngineSetting) -> SettingsPage {
    match setting {
        EngineSetting::MouseSensitivity
        | EngineSetting::MouseSmoothing
        | EngineSetting::InvertMouseY => SettingsPage::Mouse,
        EngineSetting::MasterVolume
        | EngineSetting::SoundEffectVolume
        | EngineSetting::MusicVolume
        | EngineSetting::UiVolume => SettingsPage::Audio,
        EngineSetting::ShadowQuality
        | EngineSetting::SsaoQuality
        | EngineSetting::EffectsQuality => SettingsPage::Graphics,
        EngineSetting::WindowMode | EngineSetting::Resolution => SettingsPage::Display,
    }
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
            MenuAction::ResetControls => "Reset",
            MenuAction::Rebind(action) => label(action),
            MenuAction::Replace => "Replace",
            MenuAction::Cancel => "Cancel",
            MenuAction::Item(_) => "Item",
            MenuAction::ItemAction(_) => "Item action",
            MenuAction::Page(page) => page.label(),
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
    game_slider(
        HeadBobLevel::KEY,
        "Head bob",
        (HeadBobLevel::MIN, HeadBobLevel::MAX, HeadBobLevel::STEP),
        "%",
        level.percent(),
    )
}

/// A game slider row of whole numbers, starting at `value`.
fn game_slider(
    key: &str,
    label: &str,
    (min, max, step): (i32, i32, i32),
    unit: &str,
    value: i32,
) -> MenuItem<MenuAction> {
    MenuItem::GameSlider(GameSlider {
        key: key.to_owned(),
        label: label.to_owned(),
        min,
        max,
        step,
        unit: unit.to_owned(),
        value,
    })
}

/// The Field of view slider row, starting at `degrees`.
fn fov_row(degrees: i32) -> MenuItem<MenuAction> {
    game_slider(
        GameRows::FOV_KEY,
        "Field of view",
        (GameRows::FOV_MIN, GameRows::FOV_MAX, 1),
        "°",
        degrees,
    )
}

/// The Crosshair opacity slider row, starting at `percent`.
fn crosshair_opacity_row(percent: i32) -> MenuItem<MenuAction> {
    game_slider(
        GameRows::CROSSHAIR_OPACITY_KEY,
        "Crosshair opacity",
        (
            GameRows::CROSSHAIR_OPACITY_MIN,
            GameRows::CROSSHAIR_OPACITY_MAX,
            10,
        ),
        "%",
        percent,
    )
}

/// Titled Settings: a button for each page, in order, then Back; Escape is Back. The Controls
/// button opens the Controls page.
pub fn settings_categories_screen() -> MenuScreen<MenuAction> {
    let mut items: Vec<MenuItem<MenuAction>> = SettingsPage::ALL
        .into_iter()
        .map(|page| match page {
            SettingsPage::Controls => MenuAction::Controls.button(),
            other => MenuAction::Page(other).button(),
        })
        .collect();
    items.push(MenuAction::Back.button());
    MenuScreen {
        title: Some("Settings".to_owned()),
        items,
        back: Some(MenuAction::Back),
        settings: None,
    }
}

/// `page` titled with its label: the engine rows placed on it by [`page_of`] in listing order,
/// starting from `engine`, then its game rows starting from `game`, then Apply and a Back
/// button; Escape is Back.
pub fn settings_page(
    page: SettingsPage,
    engine: EngineSettings,
    game: &GameRows,
) -> MenuScreen<MenuAction> {
    let mut items: Vec<MenuItem<MenuAction>> = EngineSetting::ALL
        .into_iter()
        .filter(|&setting| page_of(setting) == page)
        .map(MenuItem::Setting)
        .collect();
    match page {
        SettingsPage::Display => items.push(fov_row(game.fov)),
        SettingsPage::Game => {
            items.push(crosshair_opacity_row(game.crosshair_opacity));
            items.push(head_bob_row(game.head_bob));
        }
        SettingsPage::Controls
        | SettingsPage::Mouse
        | SettingsPage::Audio
        | SettingsPage::Graphics => {}
    }
    items.push(MenuItem::Apply);
    items.push(MenuAction::Back.button());
    MenuScreen {
        title: Some(page.label().to_owned()),
        items,
        back: Some(MenuAction::Back),
        settings: Some(engine),
    }
}

/// The Controls page's rows for `draft`: one per action with its two slots, then Apply, Reset and Back.
pub fn controls_rows(draft: &ControlsDraft) -> Vec<MenuItem<MenuAction>> {
    let mut items: Vec<MenuItem<MenuAction>> = SearchlightAction::ALL
        .iter()
        .map(|&action| MenuItem::Control {
            label: label(action).to_owned(),
            action: MenuAction::Rebind(action),
            slots: draft.slots(action),
            note: None,
            chevron: None,
            mode: None,
        })
        .collect();
    items.push(MenuAction::ApplyControls.button());
    items.push(MenuAction::ResetControls.button());
    items.push(MenuAction::Back.button());
    items
}

/// The titled Controls page for `draft`; Escape is Back.
pub fn controls_screen(draft: &ControlsDraft) -> MenuScreen<MenuAction> {
    MenuScreen {
        title: Some("Controls".to_owned()),
        items: controls_rows(draft),
        back: Some(MenuAction::Back),
        settings: None,
    }
}

/// The dialog asking whether `input_label` moves from `by_label`'s action; Escape is Cancel.
pub fn conflict_screen(input_label: &str, by_label: &str) -> MenuScreen<MenuAction> {
    MenuScreen {
        title: Some(format!("{input_label} is used by {by_label}")),
        items: vec![MenuAction::Replace.button(), MenuAction::Cancel.button()],
        back: Some(MenuAction::Cancel),
        settings: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use moho_core::prefs::Prefs;
    use moho_input::bindings::{Action, ActionBindings, Binding, Input};
    use moho_input::key::{Key, MouseButton};
    use moho_ui_shell::{GameSlider, Slot};

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

        let rows = GameRows {
            head_bob: level(30),
            ..GameRows::default()
        };

        let screen = settings_page(SettingsPage::Game, settings, &rows);

        let expected = vec![
            crosshair_opacity_row(100),
            head_bob_row(level(30)),
            MenuItem::Apply,
            MenuAction::Back.button(),
        ];
        assert_eq!(screen.items, expected);
        assert_eq!(screen.title.as_deref(), Some("Game"));
        assert_eq!(screen.back, Some(MenuAction::Back));
        assert_eq!(screen.settings, Some(settings));
    }

    fn page(page: SettingsPage) -> MenuScreen<MenuAction> {
        let settings = EngineSettings::from_prefs(&Prefs::default());
        settings_page(page, settings, &GameRows::default())
    }

    fn slider(
        key: &str,
        label: &str,
        range: (i32, i32, i32),
        unit: &str,
        value: i32,
    ) -> MenuItem<MenuAction> {
        MenuItem::GameSlider(GameSlider {
            key: key.to_owned(),
            label: label.to_owned(),
            min: range.0,
            max: range.1,
            step: range.2,
            unit: unit.to_owned(),
            value,
        })
    }

    #[test]
    fn the_categories_screen_lists_the_six_pages_in_order_then_back() {
        let screen = settings_categories_screen();

        let labels: Vec<&str> = screen
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Button { label, .. } => label.as_str(),
                other => panic!("the categories screen lists only buttons, not {other:?}"),
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Controls", "Mouse", "Audio", "Graphics", "Display", "Game", "Back"
            ]
        );
        assert_eq!(screen.title.as_deref(), Some("Settings"));
        assert_eq!(screen.back, Some(MenuAction::Back));
        assert_eq!(
            screen.settings, None,
            "no setting is staged on the categories"
        );
        assert_eq!(
            SettingsPage::ALL.map(SettingsPage::label),
            ["Controls", "Mouse", "Audio", "Graphics", "Display", "Game"]
        );
    }

    #[test]
    fn page_of_places_each_engine_row_on_its_page() {
        let placed = EngineSetting::ALL.map(page_of);

        assert_eq!(
            placed,
            [
                SettingsPage::Mouse,
                SettingsPage::Mouse,
                SettingsPage::Mouse,
                SettingsPage::Audio,
                SettingsPage::Audio,
                SettingsPage::Audio,
                SettingsPage::Audio,
                SettingsPage::Graphics,
                SettingsPage::Graphics,
                SettingsPage::Display,
                SettingsPage::Display,
                SettingsPage::Graphics,
            ]
        );
    }

    #[test]
    fn audio_page_lists_only_the_audio_rows() {
        let settings = EngineSettings::from_prefs(&Prefs::default());

        let screen = settings_page(SettingsPage::Audio, settings, &GameRows::default());

        assert_eq!(
            screen.items,
            vec![
                MenuItem::Setting(EngineSetting::MasterVolume),
                MenuItem::Setting(EngineSetting::SoundEffectVolume),
                MenuItem::Setting(EngineSetting::MusicVolume),
                MenuItem::Setting(EngineSetting::UiVolume),
                MenuItem::Apply,
                MenuAction::Back.button(),
            ],
            "master above the other volumes, then Apply and Back"
        );
        assert_eq!(screen.title.as_deref(), Some("Audio"));
        assert_eq!(screen.back, Some(MenuAction::Back));
        assert_eq!(screen.settings, Some(settings));
    }

    #[test]
    fn the_mouse_graphics_and_display_pages_list_only_their_rows_in_listing_order() {
        let mouse = page(SettingsPage::Mouse);
        let graphics = page(SettingsPage::Graphics);
        let display = page(SettingsPage::Display);

        assert_eq!(
            mouse.items,
            vec![
                MenuItem::Setting(EngineSetting::MouseSensitivity),
                MenuItem::Setting(EngineSetting::MouseSmoothing),
                MenuItem::Setting(EngineSetting::InvertMouseY),
                MenuItem::Apply,
                MenuAction::Back.button(),
            ]
        );
        assert_eq!(
            graphics.items,
            vec![
                MenuItem::Setting(EngineSetting::ShadowQuality),
                MenuItem::Setting(EngineSetting::SsaoQuality),
                MenuItem::Setting(EngineSetting::EffectsQuality),
                MenuItem::Apply,
                MenuAction::Back.button(),
            ]
        );
        assert_eq!(
            display.items,
            vec![
                MenuItem::Setting(EngineSetting::WindowMode),
                MenuItem::Setting(EngineSetting::Resolution),
                slider("fov", "Field of view", (50, 90, 1), "°", 75),
                MenuItem::Apply,
                MenuAction::Back.button(),
            ]
        );
        assert_eq!(
            [mouse.title, graphics.title, display.title].map(Option::unwrap_or_default),
            ["Mouse", "Graphics", "Display"]
        );
    }

    #[test]
    fn the_game_page_lists_crosshair_opacity_then_head_bob_then_apply_and_back() {
        let rows = GameRows {
            fov: 60,
            crosshair_opacity: 30,
            head_bob: level(25),
        };
        let settings = EngineSettings::from_prefs(&Prefs::default());

        let screen = settings_page(SettingsPage::Game, settings, &rows);

        assert_eq!(
            screen.items,
            vec![
                slider(
                    "crosshair_opacity",
                    "Crosshair opacity",
                    (0, 100, 10),
                    "%",
                    30
                ),
                head_bob_row(level(25)),
                MenuItem::Apply,
                MenuAction::Back.button(),
            ]
        );
        assert_eq!(screen.title.as_deref(), Some("Game"));
        assert_eq!(screen.back, Some(MenuAction::Back));
    }

    #[test]
    fn the_display_page_starts_the_field_of_view_slider_at_the_saved_value() {
        let rows = GameRows {
            fov: 90,
            ..GameRows::default()
        };
        let settings = EngineSettings::from_prefs(&Prefs::default());

        let screen = settings_page(SettingsPage::Display, settings, &rows);

        assert!(
            screen
                .items
                .contains(&slider("fov", "Field of view", (50, 90, 1), "°", 90))
        );
    }

    #[test]
    fn every_engine_setting_is_on_exactly_one_page() {
        let pages = [
            SettingsPage::Mouse,
            SettingsPage::Audio,
            SettingsPage::Graphics,
            SettingsPage::Display,
            SettingsPage::Game,
        ];

        for setting in EngineSetting::ALL {
            let homes: Vec<SettingsPage> = pages
                .into_iter()
                .filter(|&at| page(at).items.contains(&MenuItem::Setting(setting)))
                .collect();

            assert_eq!(homes, vec![page_of(setting)], "{}", setting.label());
        }
        for key in ["fov", "crosshair_opacity", HeadBobLevel::KEY] {
            let homes = pages
                .into_iter()
                .filter(|&at| {
                    page(at).items.iter().any(
                        |item| matches!(item, MenuItem::GameSlider(slider) if slider.key == key),
                    )
                })
                .count();

            assert_eq!(homes, 1, "the {key} row");
        }
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
            "Hold C",
            "F8",
            "Q",
            "E",
            "F",
            "Mouse Right",
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
    fn an_action_with_no_binding_is_marked_unbound() {
        let mut bindings = default_bindings();
        bindings.set(SearchlightAction::LeanLeft, vec![]);
        let draft = ControlsDraft::new(&bindings);

        let rows = control_rows(&controls_screen(&draft));

        let slots = |action| {
            let at = SearchlightAction::ALL.iter().position(|&a| a == action);
            rows[at.expect("the action is listed")].1
        };
        assert_eq!(slots(SearchlightAction::LeanLeft), [None, None]);
        assert!(draft.is_unbound(SearchlightAction::LeanLeft));
        assert!(!draft.is_unbound(SearchlightAction::LeanRight));
        assert_ne!(slots(SearchlightAction::LeanRight), [None, None]);
    }

    #[test]
    fn the_conflict_dialog_names_both_and_offers_replace_or_cancel() {
        let screen = conflict_screen("Q", "Lean left");

        assert_eq!(screen.title.as_deref(), Some("Q is used by Lean left"));
        assert_eq!(
            screen.items,
            vec![MenuAction::Replace.button(), MenuAction::Cancel.button()]
        );
        assert_eq!(screen.back, Some(MenuAction::Cancel), "Escape cancels");
    }

    #[test]
    fn check_magazine_shows_hold_r_as_its_secondary() {
        let draft = ControlsDraft::new(&default_bindings());

        let rows = control_rows(&controls_screen(&draft));

        let at = SearchlightAction::ALL
            .iter()
            .position(|&a| a == SearchlightAction::CheckMagazine)
            .expect("the action is listed");
        assert_eq!(
            rows[at].1,
            [
                Some(Binding::key(Key::C)),
                Some(Binding::key(Key::R).held())
            ]
        );
    }

    #[test]
    fn controls_shows_the_input_a_draft_holds() {
        let mut draft = ControlsDraft::new(&default_bindings());
        draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::T));
        draft.capture(
            SearchlightAction::Fire,
            Slot::Primary,
            Input::Mouse(MouseButton::Middle),
        );

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
            [Some(Binding::mouse(MouseButton::Middle)), None]
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
