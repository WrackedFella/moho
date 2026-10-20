//! The FPS's debug values, kept in the game's own prefs file under `[debug]`.

use std::io;
use std::path::PathBuf;

use moho_core::prefs::Prefs;

/// The prefs entry that holds god mode.
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
    fn a_save_that_cannot_be_written_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let mut settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));

        let saved = settings.set_god_mode(true);

        assert!(saved.is_err());
    }
}
