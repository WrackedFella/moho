//! The FPS's debug values, kept in the game's own prefs file under `[debug]`.

use std::io;
use std::path::PathBuf;

use moho_core::prefs::{Prefs, WindowMode};
use moho_ui_shell::EngineSettings;

const GOD_MODE: &str = "god_mode";

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
        false
    }

    /// Stores Simple and saves the prefs file.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_simple(&mut self, _on: bool) -> io::Result<()> {
        todo!("DebugSettings::set_simple")
    }

    /// Stores the engine's ten settings rows and saves the prefs file; every other value in the
    /// file is kept, and the rows are kept for the session if the save fails.
    ///
    /// # Errors
    /// The I/O error if the file cannot be written.
    pub fn set_engine_settings(&mut self, settings: EngineSettings) -> io::Result<()> {
        settings.write_to(&mut self.prefs);
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

    use super::*;

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
            sound_effect_volume: 1.5,
            music_volume: 0.0,
            ui_volume: 9.5,
            shadow_quality: 1,
            ssao_quality: 4,
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
            sound_effect_volume: 8.0,
            music_volume: 6.0,
            ui_volume: 1.0,
            shadow_quality: 3,
            ssao_quality: 3,
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
