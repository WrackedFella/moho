//! The FPS prototype: arena, shot resolution and the fixed-tick simulation, adapted from
//! the engine crates and the platform-free rules in `searchlight_rules`.

pub mod arena;
pub mod brain;
pub mod console_commands;
pub mod cover;
pub mod debug_settings;
pub mod game;
pub mod hud;
pub mod input;
pub mod mode;
pub mod shot;
pub mod sim;
pub mod tag;
pub mod wave;

use std::collections::BTreeMap;
use std::path::PathBuf;

use moho_ui_shell::CONSOLE_BINDING;

/// Ticks per second the prototype runs at.
pub const TICK_HZ: u32 = 60;

/// Where the FPS keeps its preferences.
pub fn prefs_path() -> PathBuf {
    PathBuf::from("config/searchlight.ini")
}

/// Splits a `[bindings]` section into the `console` entry and the game's own actions,
/// so neither loader warns about the other's name.
pub fn split_console_binding(
    section: &BTreeMap<String, String>,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut game = section.clone();
    let console = game
        .remove(CONSOLE_BINDING)
        .map(|value| (CONSOLE_BINDING.to_owned(), value))
        .into_iter()
        .collect();
    (console, game)
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console_commands;
    use crate::input::SearchlightAction;
    use moho_core::prefs::Prefs;
    use moho_input::bindings::{ActionBindings, Binding};
    use moho_input::key::Key;
    use moho_ui_shell::console::Routing;

    #[test]
    fn the_fps_prefs_file_is_not_strategys() {
        assert_eq!(prefs_path(), std::path::Path::new("config/searchlight.ini"));
        assert_ne!(prefs_path(), Prefs::config_path());
    }

    #[test]
    fn a_fresh_fps_prefs_file_has_the_default_console_key() {
        let dir = tempfile::tempdir().unwrap();
        let mut strategy = Prefs::default();
        strategy.set_bindings(BTreeMap::from([(
            "console".to_owned(),
            "Backslash".to_owned(),
        )]));
        strategy
            .save_to(&dir.path().join(Prefs::config_path()))
            .unwrap();

        let (fps, warnings) = Prefs::load_from(&dir.path().join(prefs_path()));
        let (console_section, _) = split_console_binding(fps.bindings());
        let (mut console, console_warnings) = console_commands::console(&console_section);

        assert_eq!(warnings, vec![]);
        assert_eq!(console_warnings, vec![]);
        assert_eq!(
            console.on_key(Binding::Key(Key::Backslash), true, false),
            Routing::Game
        );
        assert_eq!(
            console.on_key(Binding::Key(Key::Backtick), true, false),
            Routing::Opened
        );
    }

    #[test]
    fn a_bindings_section_with_console_and_jump_loads_both_without_warnings() {
        let section = BTreeMap::from([
            ("console".to_owned(), "Backslash".to_owned()),
            ("jump".to_owned(), "F".to_owned()),
        ]);

        let (console_section, game_section) = split_console_binding(&section);
        let (mut console, console_warnings) = console_commands::console(&console_section);
        let (bindings, game_warnings) = ActionBindings::<SearchlightAction>::load(&game_section);

        assert_eq!(console_warnings, vec![]);
        assert_eq!(game_warnings, vec![]);
        assert_eq!(
            console.on_key(Binding::Key(Key::Backslash), true, false),
            Routing::Opened
        );
        assert_eq!(
            bindings.get(SearchlightAction::Jump),
            [Binding::Key(Key::F)]
        );
    }
}
