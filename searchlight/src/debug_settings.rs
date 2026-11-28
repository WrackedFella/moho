//! The FPS's debug values, kept in the game's own prefs file under `[debug]`.

use std::io;
use std::path::PathBuf;

use moho_app::FULLSCREEN_BINDING;
use moho_core::prefs::{Prefs, WindowMode};
use moho_input::bindings::ActionBindings;
use moho_ui_shell::{CONSOLE_BINDING, EngineSettings};

use crate::aim::Magnification;
use crate::head_bob::HeadBobLevel;
use crate::input::SearchlightAction;

const GOD_MODE: &str = "god_mode";
const SIMPLE: &str = "simple";
const ZOOM: &str = "zoom";

/// The game's own settings rows, kept under `[game]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameRows {
    /// Vertical field of view in degrees, 50 to 90.
    pub fov: i32,
    /// Crosshair opacity in percent, 0 to 100.
    pub crosshair_opacity: i32,
    pub head_bob: HeadBobLevel,
}

impl GameRows {
    pub const FOV_KEY: &'static str = "fov";
    pub const FOV_MIN: i32 = 50;
    pub const FOV_MAX: i32 = 90;
    pub const CROSSHAIR_OPACITY_KEY: &'static str = "crosshair_opacity";
    pub const CROSSHAIR_OPACITY_MIN: i32 = 0;
    pub const CROSSHAIR_OPACITY_MAX: i32 = 100;
}

impl Default for GameRows {
    fn default() -> Self {
        Self {
            fov: 75,
            crosshair_opacity: 100,
            head_bob: HeadBobLevel::DEFAULT,
        }
    }
}

/// The whole loaded prefs and where to save them, so a save keeps every other value.
#[derive(Clone, Debug)]
pub struct DebugSettings {
    prefs: Prefs,
    path: PathBuf,
}

impl DebugSettings {
    pub fn new(prefs: Prefs, path: PathBuf) -> Self {
        Self { prefs, path }
    }

    /// The loaded prefs, for settings the game only reads.
    pub fn prefs(&self) -> &Prefs {
        &self.prefs
    }

    /// Whether god mode is stored as on; an absent or unrecognised value reads as off.
    pub fn god_mode(&self) -> bool {
        self.prefs.debug_value(GOD_MODE) == Some("1")
    }

    /// Stores god mode and saves the prefs file.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_god_mode(&mut self, on: bool) -> io::Result<()> {
        self.prefs
            .set_debug_value(GOD_MODE, if on { "1" } else { "0" })
            .expect("\"god_mode\" with \"1\" or \"0\" is a name and value prefs always accepts");
        self.prefs.save_to(&self.path)
    }

    /// Whether Simple is stored as on; an absent or unrecognised value reads as off.
    pub fn simple(&self) -> bool {
        self.prefs.debug_value(SIMPLE) == Some("1")
    }

    /// Stores Simple and saves the prefs file.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_simple(&mut self, on: bool) -> io::Result<()> {
        self.prefs
            .set_debug_value(SIMPLE, if on { "1" } else { "0" })
            .expect("\"simple\" with \"1\" or \"0\" is a name and value prefs always accepts");
        self.prefs.save_to(&self.path)
    }

    /// The magnification the prefs hold under `[debug]`; 10 when absent or invalid.
    pub fn zoom(&self) -> Magnification {
        self.prefs
            .debug_value(ZOOM)
            .and_then(Magnification::parse)
            .unwrap_or(Magnification::DEFAULT)
    }

    /// Stores the magnification under `[debug]` and saves the prefs file; the value is kept if
    /// the save fails.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_zoom(&mut self, zoom: Magnification) -> io::Result<()> {
        self.prefs
            .set_debug_value(ZOOM, &zoom.get().to_string())
            .expect("\"zoom\" with a number is a name and value prefs always accepts");
        self.prefs.save_to(&self.path)
    }

    /// The head bob the prefs hold under `[game]`: 50 % when absent or unparsable, and a stored
    /// value out of range or off the grid snaps to the nearest step.
    pub fn head_bob(&self) -> HeadBobLevel {
        self.prefs
            .game_value(HeadBobLevel::KEY)
            .and_then(|text| text.parse::<i32>().ok())
            .map_or(HeadBobLevel::DEFAULT, HeadBobLevel::nearest)
    }

    /// Stores the head bob under `[game]` and saves the prefs file; the value is kept if the save
    /// fails.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_head_bob(&mut self, level: HeadBobLevel) -> io::Result<()> {
        self.prefs
            .set_game_value(HeadBobLevel::KEY, &level.percent().to_string())
            .expect("\"head_bob\" with a number is a name and value prefs always accepts");
        self.prefs.save_to(&self.path)
    }

    /// The game rows the prefs hold under `[game]`: an absent or unparsable value reads as its
    /// default and an out-of-range one is clamped.
    pub fn game_rows(&self) -> GameRows {
        let number = |key: &str, default: i32, min: i32, max: i32| {
            self.prefs
                .game_value(key)
                .and_then(|text| text.parse::<i32>().ok())
                .map_or(default, |value| value.clamp(min, max))
        };
        let defaults = GameRows::default();
        GameRows {
            fov: number(
                GameRows::FOV_KEY,
                defaults.fov,
                GameRows::FOV_MIN,
                GameRows::FOV_MAX,
            ),
            crosshair_opacity: number(
                GameRows::CROSSHAIR_OPACITY_KEY,
                defaults.crosshair_opacity,
                GameRows::CROSSHAIR_OPACITY_MIN,
                GameRows::CROSSHAIR_OPACITY_MAX,
            ),
            head_bob: self.head_bob(),
        }
    }

    /// Stores the game rows under `[game]` and saves the prefs file; the values are kept if the
    /// save fails.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_game_rows(&mut self, rows: GameRows) -> io::Result<()> {
        for (key, value) in [
            (GameRows::FOV_KEY, rows.fov),
            (GameRows::CROSSHAIR_OPACITY_KEY, rows.crosshair_opacity),
            (HeadBobLevel::KEY, rows.head_bob.percent()),
        ] {
            self.prefs
                .set_game_value(key, &value.to_string())
                .expect("a game row name with a number is a name and value prefs always accepts");
        }
        self.prefs.save_to(&self.path)
    }

    /// Stores the engine's settings rows and saves the prefs file; every other value in the
    /// file is kept, and the rows are kept for the session if the save fails.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_engine_settings(&mut self, settings: EngineSettings) -> io::Result<()> {
        settings.write_to(&mut self.prefs);
        self.prefs.save_to(&self.path)
    }

    /// Stores the game's action bindings under `[bindings]` and their hold/toggle modes under
    /// `[action_modes]`, and saves the prefs file; the `console` and `fullscreen` entries the
    /// file holds are kept.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_bindings(&mut self, bindings: &ActionBindings<SearchlightAction>) -> io::Result<()> {
        let mut section = bindings.to_section();
        for reserved in [CONSOLE_BINDING, FULLSCREEN_BINDING] {
            if let Some(value) = self.prefs.bindings().get(reserved) {
                section.insert(reserved.to_owned(), value.clone());
            }
        }
        self.prefs.set_bindings(section);
        self.prefs.set_action_modes(bindings.modes_to_section());
        self.prefs.save_to(&self.path)
    }

    /// The window mode the prefs hold.
    pub fn window_mode(&self) -> WindowMode {
        self.prefs.window_mode()
    }

    /// Stores the window mode and saves the prefs file; the value is kept if the save fails.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_window_mode(&mut self, mode: WindowMode) -> io::Result<()> {
        self.prefs.set_window_mode(mode);
        self.prefs.save_to(&self.path)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use moho_app::split_fullscreen_binding;
    use moho_input::bindings::{ActionMode, Binding, Input, Modifiers};
    use moho_input::key::{Key, MouseButton};

    use super::*;
    use crate::split_console_binding;

    fn settings_at(path: &Path) -> DebugSettings {
        let (prefs, warnings) = Prefs::load_from(path);
        assert_eq!(warnings, vec![]);
        DebugSettings::new(prefs, path.to_path_buf())
    }

    #[test]
    fn a_fresh_prefs_file_has_god_mode_off() {
        let dir = tempfile::tempdir().unwrap();

        let settings = settings_at(&dir.path().join("searchlight.ini"));

        assert!(!settings.god_mode());
    }

    #[test]
    fn an_unrecognised_value_reads_as_off() {
        for value in ["0", "2", "yes", "true", "on", "11"] {
            let mut prefs = Prefs::default();
            prefs.set_debug_value("god_mode", value).unwrap();

            let settings = DebugSettings::new(prefs, PathBuf::from("unused.ini"));

            assert!(!settings.god_mode(), "{value:?}");
        }
    }

    #[test]
    fn the_stored_one_reads_as_on() {
        let mut prefs = Prefs::default();
        prefs.set_debug_value("god_mode", "1").unwrap();

        let settings = DebugSettings::new(prefs, PathBuf::from("unused.ini"));

        assert!(settings.god_mode());
    }

    #[test]
    fn god_mode_survives_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/searchlight.ini");
        let mut settings = settings_at(&path);

        settings.set_god_mode(true).unwrap();
        let after_on = settings_at(&path).god_mode();
        settings.set_god_mode(false).unwrap();
        let after_off = settings_at(&path).god_mode();

        assert!(after_on);
        assert!(!after_off);
        assert!(!settings.god_mode());
    }

    #[test]
    fn simple_survives_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/searchlight.ini");
        let mut settings = settings_at(&path);
        assert!(!settings.simple(), "a fresh file has Simple off");

        settings.set_simple(true).unwrap();
        let after_on = settings_at(&path).simple();
        settings.set_simple(false).unwrap();
        let after_off = settings_at(&path).simple();

        assert!(after_on);
        assert!(!after_off);
        assert!(!settings.simple());
    }

    fn zoom_of(factor: f32) -> Magnification {
        Magnification::new(factor).expect("a factor from 1 to 12")
    }

    #[test]
    fn zoom_reads_ten_when_absent_or_invalid() {
        let read = |value: Option<&str>| {
            let mut prefs = Prefs::default();
            if let Some(value) = value {
                prefs.set_debug_value("zoom", value).unwrap();
            }
            DebugSettings::new(prefs, PathBuf::from("unused.ini")).zoom()
        };

        assert_eq!(read(None), zoom_of(10.0));
        for invalid in ["x", "0", "0.5", "13", "-5", "nan", "inf", "5x"] {
            assert_eq!(read(Some(invalid)), zoom_of(10.0), "{invalid:?}");
        }
        for (stored, expected) in [("1", 1.0), ("5", 5.0), ("2.5", 2.5), ("12", 12.0)] {
            assert_eq!(read(Some(stored)), zoom_of(expected), "{stored:?}");
        }
    }

    #[test]
    fn zoom_survives_a_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/searchlight.ini");
        let mut settings = settings_at(&path);
        settings.set_god_mode(true).unwrap();

        settings.set_zoom(zoom_of(5.0)).unwrap();
        let after_five = settings_at(&path);
        settings.set_zoom(zoom_of(2.5)).unwrap();
        let after_two_and_a_half = settings_at(&path);

        assert_eq!(after_five.zoom(), zoom_of(5.0));
        assert!(after_five.prefs().debug_value("zoom").is_some());
        assert_eq!(after_two_and_a_half.zoom(), zoom_of(2.5));
        assert_eq!(settings.zoom(), zoom_of(2.5));
        assert!(after_five.god_mode(), "saving zoom keeps other values");
    }

    #[test]
    fn a_failed_save_keeps_the_zoom_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let mut settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));

        let saved = settings.set_zoom(zoom_of(4.0));

        assert!(saved.is_err());
        assert_eq!(settings.zoom(), zoom_of(4.0));
    }

    #[test]
    fn simple_and_god_mode_are_stored_apart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("searchlight.ini");
        let mut settings = settings_at(&path);

        settings.set_simple(true).unwrap();
        let only_simple = settings_at(&path);
        settings.set_god_mode(true).unwrap();
        settings.set_simple(false).unwrap();
        let only_god = settings_at(&path);

        assert!(only_simple.simple() && !only_simple.god_mode());
        assert!(only_god.god_mode() && !only_god.simple());
    }

    #[test]
    fn setting_god_mode_reads_back_in_the_same_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings_at(&dir.path().join("searchlight.ini"));

        settings.set_god_mode(true).unwrap();

        assert!(settings.god_mode());
    }

    #[test]
    fn saving_keeps_bindings_and_other_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("searchlight.ini");
        let mut prefs = Prefs::default().with_mouse_sensitivity(2.5);
        prefs.set_bindings(std::collections::BTreeMap::from([(
            "jump".to_owned(),
            "F".to_owned(),
        )]));
        prefs.set_debug_value("other_value", "kept").unwrap();
        let mut settings = DebugSettings::new(prefs, path.clone());

        settings.set_god_mode(true).unwrap();

        let (saved, warnings) = Prefs::load_from(&path);
        assert_eq!(warnings, vec![]);
        assert_eq!(saved.bindings().get("jump").map(String::as_str), Some("F"));
        assert_eq!(saved.debug_value("other_value"), Some("kept"));
        assert_eq!(saved.debug_value("god_mode"), Some("1"));
        assert_eq!(saved.mouse_sensitivity(), 2.5);
    }

    #[test]
    fn window_mode_round_trips_through_the_prefs_file_and_keeps_other_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("searchlight.ini");
        let mut prefs = Prefs::default().with_mouse_sensitivity(2.5);
        prefs.set_bindings(std::collections::BTreeMap::from([(
            "jump".to_owned(),
            "F".to_owned(),
        )]));
        prefs.set_debug_value("other_value", "kept").unwrap();
        let mut settings = DebugSettings::new(prefs, path.clone());
        settings.set_god_mode(true).unwrap();

        settings.set_window_mode(WindowMode::Borderless).unwrap();
        let borderless = settings_at(&path);
        settings.set_window_mode(WindowMode::Windowed).unwrap();
        let windowed = settings_at(&path);

        assert_eq!(borderless.window_mode(), WindowMode::Borderless);
        assert_eq!(windowed.window_mode(), WindowMode::Windowed);
        assert_eq!(settings.window_mode(), WindowMode::Windowed);
        assert_eq!(
            windowed.prefs.bindings().get("jump").map(String::as_str),
            Some("F")
        );
        assert_eq!(windowed.prefs.debug_value("other_value"), Some("kept"));
        assert!(windowed.god_mode());
        assert_eq!(windowed.prefs.mouse_sensitivity(), 2.5);
    }

    #[test]
    fn applied_engine_settings_survive_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/searchlight.ini");
        let mut prefs = Prefs::default().with_mouse_sensitivity(2.5);
        prefs.set_bindings(std::collections::BTreeMap::from([(
            "jump".to_owned(),
            "F".to_owned(),
        )]));
        prefs.set_debug_value("other_value", "kept").unwrap();
        prefs.set_brightness(1.5);
        *prefs.voice_volume_mut() = 3.0;
        let mut settings = DebugSettings::new(prefs, path.clone());
        settings.set_god_mode(true).unwrap();
        let applied = EngineSettings {
            mouse_sensitivity: 6.5,
            mouse_smoothing: true,
            invert_mouse_y: true,
            master_volume: 3.5,
            sound_effect_volume: 1.5,
            music_volume: 0.0,
            ui_volume: 9.5,
            shadow_quality: 1,
            ssao_quality: 4,
            effects_quality: 3,
            window_mode: WindowMode::Borderless,
            resolution: (2560, 1440),
        };

        settings.set_engine_settings(applied).unwrap();
        let reloaded = settings_at(&path);

        assert_eq!(EngineSettings::from_prefs(reloaded.prefs()), applied);
        assert_eq!(EngineSettings::from_prefs(settings.prefs()), applied);
        assert_eq!(
            reloaded.prefs.bindings().get("jump").map(String::as_str),
            Some("F")
        );
        assert_eq!(reloaded.prefs.debug_value("other_value"), Some("kept"));
        assert!(reloaded.god_mode());
        assert_eq!(reloaded.prefs.brightness(), 1.5);
        assert_eq!(reloaded.prefs.voice_volume(), 3.0);
    }

    #[test]
    fn a_failed_engine_settings_save_keeps_the_values_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let mut settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));
        let applied = EngineSettings {
            mouse_sensitivity: 3.0,
            mouse_smoothing: false,
            invert_mouse_y: false,
            master_volume: 10.0,
            sound_effect_volume: 8.0,
            music_volume: 6.0,
            ui_volume: 1.0,
            shadow_quality: 3,
            ssao_quality: 3,
            effects_quality: 3,
            window_mode: WindowMode::Windowed,
            resolution: (1920, 1080),
        };

        let saved = settings.set_engine_settings(applied);

        assert!(saved.is_err());
        assert_eq!(settings.prefs().ui_volume(), 1.0);
    }

    #[test]
    fn a_failed_save_keeps_the_window_mode_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let mut settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));

        let saved = settings.set_window_mode(WindowMode::Borderless);

        assert!(saved.is_err());
        assert_eq!(settings.window_mode(), WindowMode::Borderless);
    }

    fn level(percent: i32) -> HeadBobLevel {
        HeadBobLevel::new(percent).expect("a percentage on the grid")
    }

    #[test]
    fn head_bob_reads_fifty_when_absent_or_unparsable_and_snaps_bad_values() {
        let read = |value: Option<&str>| {
            let mut prefs = Prefs::default();
            if let Some(value) = value {
                prefs.set_game_value("head_bob", value).unwrap();
            }
            DebugSettings::new(prefs, PathBuf::from("unused.ini")).head_bob()
        };

        assert_eq!(read(None), level(50));
        for unparsable in ["x", "fifty", "2.5", "5%", "--5"] {
            assert_eq!(read(Some(unparsable)), level(50), "{unparsable:?}");
        }
        for (stored, expected) in [
            ("0", 0),
            ("25", 25),
            ("100", 100),
            ("23", 25),
            ("22", 20),
            ("150", 100),
            ("-20", 0),
        ] {
            assert_eq!(read(Some(stored)), level(expected), "{stored:?}");
        }
    }

    #[test]
    fn head_bob_is_stored_under_game_and_survives_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/searchlight.ini");
        let mut settings = settings_at(&path);
        settings.set_god_mode(true).unwrap();

        settings.set_head_bob(level(25)).unwrap();
        let after_25 = settings_at(&path);
        settings.set_head_bob(level(0)).unwrap();
        let after_0 = settings_at(&path);

        assert_eq!(after_25.head_bob(), level(25));
        assert_eq!(after_25.prefs().game_value("head_bob"), Some("25"));
        assert_eq!(after_25.prefs().debug_value("head_bob"), None);
        assert_eq!(after_0.head_bob(), level(0));
        assert_eq!(settings.head_bob(), level(0));
        assert!(after_0.god_mode(), "saving head bob keeps other values");
    }

    #[test]
    fn a_failed_save_keeps_the_head_bob_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let mut settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));

        let saved = settings.set_head_bob(level(15));

        assert!(saved.is_err());
        assert_eq!(settings.head_bob(), level(15));
    }

    #[test]
    fn game_rows_round_trip_through_the_prefs_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/searchlight.ini");
        let mut settings = settings_at(&path);
        settings.set_god_mode(true).unwrap();
        let fresh = settings.game_rows();
        let rows = GameRows {
            fov: 60,
            crosshair_opacity: 40,
            head_bob: level(25),
        };

        settings.set_game_rows(rows).unwrap();
        let reloaded = settings_at(&path);

        assert_eq!((fresh.fov, fresh.crosshair_opacity), (75, 100));
        assert_eq!(reloaded.game_rows(), rows);
        assert_eq!(settings.game_rows(), rows);
        assert_eq!(reloaded.prefs().game_value("fov"), Some("60"));
        assert_eq!(reloaded.prefs().game_value("crosshair_opacity"), Some("40"));
        assert_eq!(reloaded.prefs().game_value("head_bob"), Some("25"));
        assert_eq!(reloaded.prefs().debug_value("fov"), None);
        assert!(reloaded.god_mode(), "saving game rows keeps other values");
    }

    #[test]
    fn an_unparsable_or_out_of_range_game_value_reads_as_default_or_clamped() {
        let read = |fov: Option<&str>, opacity: Option<&str>| {
            let mut prefs = Prefs::default();
            if let Some(fov) = fov {
                prefs.set_game_value("fov", fov).unwrap();
            }
            if let Some(opacity) = opacity {
                prefs.set_game_value("crosshair_opacity", opacity).unwrap();
            }
            let rows = DebugSettings::new(prefs, PathBuf::from("unused.ini")).game_rows();
            (rows.fov, rows.crosshair_opacity)
        };

        assert_eq!(read(None, None), (75, 100));
        for unparsable in ["x", "fifty", "7.5", "5%", "--5"] {
            assert_eq!(read(Some(unparsable), None).0, 75, "fov {unparsable:?}");
            assert_eq!(
                read(None, Some(unparsable)).1,
                100,
                "opacity {unparsable:?}"
            );
        }
        for (stored, expected) in [("10", 50), ("-5", 50), ("49", 50), ("50", 50), ("90", 90)] {
            assert_eq!(read(Some(stored), None).0, expected, "fov {stored:?}");
        }
        for (stored, expected) in [("91", 90), ("200", 90), ("62", 62)] {
            assert_eq!(read(Some(stored), None).0, expected, "fov {stored:?}");
        }
        for (stored, expected) in [("-10", 0), ("0", 0), ("100", 100), ("150", 100)] {
            assert_eq!(read(None, Some(stored)).1, expected, "opacity {stored:?}");
        }
        assert_eq!(
            read(Some("200"), Some("x")),
            (90, 100),
            "each value is read on its own"
        );
    }

    fn rebound() -> ActionBindings<SearchlightAction> {
        let (mut bindings, warnings) = ActionBindings::load(&std::collections::BTreeMap::new());
        assert!(warnings.is_empty());
        bindings.set(SearchlightAction::Reload, vec![Binding::key(Key::T)]);
        bindings.set(
            SearchlightAction::Fire,
            vec![Binding::mouse(MouseButton::Right)],
        );
        bindings.set(
            SearchlightAction::Jump,
            vec![
                Binding::key(Key::Space),
                Binding::key(Key::X).with(Modifiers::CTRL),
            ],
        );
        bindings
    }

    #[test]
    fn applied_bindings_load_back_as_main_loads_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/searchlight.ini");
        let mut settings = settings_at(&path);
        let mut bindings = rebound();
        bindings.set(
            SearchlightAction::Reload,
            vec![Binding::key(Key::T), Binding::key(Key::Y)],
        );

        settings.set_bindings(&bindings).unwrap();
        let (prefs, warnings) = Prefs::load_from(&path);
        let (_toggle_keys, without_fullscreen) = split_fullscreen_binding(prefs.bindings());
        let (_console, game_section) = split_console_binding(&without_fullscreen);
        let (loaded, game_warnings) = ActionBindings::<SearchlightAction>::load(&game_section);

        assert_eq!(warnings, vec![]);
        assert_eq!(game_warnings, vec![]);
        assert_eq!(loaded, bindings);
    }

    #[test]
    fn applied_modes_load_back_as_main_loads_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/searchlight.ini");
        let mut settings = settings_at(&path);
        let mut bindings = rebound();
        bindings.set_mode(SearchlightAction::LeanLeft, ActionMode::Toggle);

        settings.set_bindings(&bindings).unwrap();
        let (prefs, warnings) = Prefs::load_from(&path);
        let (_toggle_keys, without_fullscreen) = split_fullscreen_binding(prefs.bindings());
        let (_console, game_section) = split_console_binding(&without_fullscreen);
        let (mut loaded, game_warnings) = ActionBindings::<SearchlightAction>::load(&game_section);
        let mode_warnings = loaded.load_modes(prefs.action_modes());

        assert_eq!(warnings, vec![]);
        assert_eq!(game_warnings, vec![]);
        assert_eq!(mode_warnings, vec![]);
        assert_eq!(loaded, bindings);
        assert_eq!(loaded.mode(SearchlightAction::LeanLeft), ActionMode::Toggle);
        assert_eq!(loaded.mode(SearchlightAction::LeanRight), ActionMode::Hold);
    }

    #[test]
    fn the_startup_load_reads_back_applied_modes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("searchlight.ini");
        let mut settings = settings_at(&path);
        let mut bindings = rebound();
        bindings.set_mode(SearchlightAction::LeanLeft, ActionMode::Toggle);
        settings.set_bindings(&bindings).unwrap();

        let (prefs, _warnings) = Prefs::load_from(&path);
        let (_toggle_keys, without_fullscreen) = split_fullscreen_binding(prefs.bindings());
        let (_console, game_section) = split_console_binding(&without_fullscreen);
        let loaded = crate::input::load_bindings(&game_section, prefs.action_modes());

        assert_eq!(loaded, bindings);
        assert_eq!(loaded.mode(SearchlightAction::LeanLeft), ActionMode::Toggle);
        assert_eq!(loaded.mode(SearchlightAction::LeanRight), ActionMode::Hold);
    }

    #[test]
    fn set_bindings_keeps_the_console_and_fullscreen_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("searchlight.ini");
        let mut prefs = Prefs::default().with_mouse_sensitivity(2.5);
        prefs.set_bindings(std::collections::BTreeMap::from([
            ("console".to_owned(), "Backslash".to_owned()),
            ("fullscreen".to_owned(), "F10".to_owned()),
            ("jump".to_owned(), "F".to_owned()),
        ]));
        prefs.set_debug_value("other_value", "kept").unwrap();
        let mut settings = DebugSettings::new(prefs, path.clone());
        let bindings = rebound();

        settings.set_bindings(&bindings).unwrap();
        let (saved, warnings) = Prefs::load_from(&path);
        let (toggle_keys, without_fullscreen) = split_fullscreen_binding(saved.bindings());
        let (console_section, game_section) = split_console_binding(&without_fullscreen);
        let (loaded, game_warnings) = ActionBindings::<SearchlightAction>::load(&game_section);

        assert_eq!(warnings, vec![]);
        assert_eq!(toggle_keys, Some(vec![Input::Key(Key::F10)]));
        assert_eq!(
            console_section.get("console").map(String::as_str),
            Some("Backslash")
        );
        assert_eq!(game_warnings, vec![]);
        assert_eq!(loaded, bindings, "the old jump line is replaced");
        assert_eq!(saved.debug_value("other_value"), Some("kept"));
        assert_eq!(saved.mouse_sensitivity(), 2.5);
        assert_eq!(
            settings
                .prefs()
                .bindings()
                .get("console")
                .map(String::as_str),
            Some("Backslash"),
            "the session keeps the console key too"
        );
    }

    #[test]
    fn a_save_that_cannot_be_written_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let mut settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));

        let saved = settings.set_god_mode(true);

        assert!(saved.is_err());
    }
}
