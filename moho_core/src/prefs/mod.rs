//! Preferences management for the game.
//!
//! This module handles loading, saving, and managing user preferences including
//! key bindings, mouse sensitivity, input filtering, audio volumes, and video settings.

mod reader;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use reader::{PrefsIssue, PrefsWarning};

/// Window display mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WindowMode {
    #[default]
    Windowed,
    Fullscreen,
    Borderless,
}

impl WindowMode {
    fn as_str(self) -> &'static str {
        match self {
            WindowMode::Windowed => "Windowed",
            WindowMode::Fullscreen => "Fullscreen",
            WindowMode::Borderless => "Borderless",
        }
    }

    /// Inverse of `as_str`: exact names only.
    fn parse(s: &str) -> Option<Self> {
        [
            WindowMode::Windowed,
            WindowMode::Fullscreen,
            WindowMode::Borderless,
        ]
        .into_iter()
        .find(|mode| mode.as_str() == s)
    }
}

/// Standard resolution presets.
pub const RESOLUTION_PRESETS: &[(&str, (u32, u32))] = &[
    ("1280×720 (HD)", (1280, 720)),
    ("1920×1080 (FHD)", (1920, 1080)),
    ("2560×1440 (QHD)", (2560, 1440)),
    ("3840×2160 (4K)", (3840, 2160)),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    mouse_sensitivity: f32,
    mouse_smoothing: bool,
    // Audio settings (values 0.0 to 10.0)
    audio_sound_effect_volume: f32,
    audio_music_volume: f32,
    audio_ui_volume: f32,
    audio_voice_volume: f32,
    // Graphics settings
    graphics_shadow_quality: u32, // 0=Off, 1=Low, 2=Medium, 3=High, 4=Ultra
    graphics_ssao_quality: u32,   // 0=Off, 1=Low, 2=Medium, 3=High, 4=Ultra
    // Video settings
    window_mode: WindowMode,
    window_resolution: (u32, u32),
    // World / streaming settings
    world_load_radius: u32,
    world_unload_radius: u32,
    world_chunks_per_frame: u32,
    bindings: BTreeMap<String, String>,
    debug: BTreeMap<String, String>,
}

const MAX_GRAPHICS_QUALITY: u32 = 4;

/// Why a debug name or value was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DebugValueError {
    #[error("debug name {0:?} must be non-empty lowercase letters, digits or underscores")]
    InvalidName(String),
    #[error("debug value {0:?} must be non-empty, unpadded and free of ';', '#' and line breaks")]
    InvalidValue(String),
}

impl Prefs {
    /// The value a game stored under `name` in `[debug]`, if any.
    pub fn debug_value(&self, name: &str) -> Option<&str> {
        self.debug.get(name).map(String::as_str)
    }

    /// Stores a game's debug value.
    ///
    /// # Errors
    /// Refuses a name or value that would not read back exactly; prefs stay unchanged.
    pub fn set_debug_value(&mut self, name: &str, value: &str) -> Result<(), DebugValueError> {
        let name_ok = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !name_ok {
            return Err(DebugValueError::InvalidName(name.to_owned()));
        }
        // The INI reader trims values and treats `;` and `#` as comments.
        let value_ok =
            !value.is_empty() && value.trim() == value && !value.contains([';', '#', '\r', '\n']);
        if !value_ok {
            return Err(DebugValueError::InvalidValue(value.to_owned()));
        }
        self.debug.insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    /// The raw `[bindings]` section: action name to unparsed value.
    pub fn bindings(&self) -> &BTreeMap<String, String> {
        &self.bindings
    }

    pub fn set_bindings(&mut self, section: BTreeMap<String, String>) {
        self.bindings = section;
    }

    // --- Scalar getters ---

    pub fn mouse_sensitivity(&self) -> f32 {
        self.mouse_sensitivity
    }
    pub fn mouse_smoothing(&self) -> bool {
        self.mouse_smoothing
    }
    pub fn sound_effect_volume(&self) -> f32 {
        self.audio_sound_effect_volume
    }
    pub fn music_volume(&self) -> f32 {
        self.audio_music_volume
    }
    pub fn ui_volume(&self) -> f32 {
        self.audio_ui_volume
    }
    pub fn voice_volume(&self) -> f32 {
        self.audio_voice_volume
    }
    pub fn shadow_quality(&self) -> u32 {
        self.graphics_shadow_quality
    }
    pub fn ssao_quality(&self) -> u32 {
        self.graphics_ssao_quality
    }
    pub fn world_load_radius(&self) -> u32 {
        self.world_load_radius
    }
    pub fn world_unload_radius(&self) -> u32 {
        self.world_unload_radius
    }
    pub fn world_chunks_per_frame(&self) -> u32 {
        self.world_chunks_per_frame
    }
    pub fn set_world_load_radius(&mut self, v: u32) {
        self.world_load_radius = v;
    }
    pub fn set_world_unload_radius(&mut self, v: u32) {
        self.world_unload_radius = v;
    }
    pub fn set_world_chunks_per_frame(&mut self, v: u32) {
        self.world_chunks_per_frame = v.max(1);
    }
    pub fn window_mode(&self) -> WindowMode {
        self.window_mode
    }
    pub fn window_resolution(&self) -> (u32, u32) {
        self.window_resolution
    }

    // --- Mutable accessors (for egui widget binding) ---

    pub fn mouse_sensitivity_mut(&mut self) -> &mut f32 {
        &mut self.mouse_sensitivity
    }
    pub fn mouse_smoothing_mut(&mut self) -> &mut bool {
        &mut self.mouse_smoothing
    }
    pub fn sound_effect_volume_mut(&mut self) -> &mut f32 {
        &mut self.audio_sound_effect_volume
    }
    pub fn music_volume_mut(&mut self) -> &mut f32 {
        &mut self.audio_music_volume
    }
    pub fn ui_volume_mut(&mut self) -> &mut f32 {
        &mut self.audio_ui_volume
    }
    pub fn voice_volume_mut(&mut self) -> &mut f32 {
        &mut self.audio_voice_volume
    }

    // --- Validated setters ---

    pub fn set_shadow_quality(&mut self, quality: u32) {
        self.graphics_shadow_quality = quality.min(MAX_GRAPHICS_QUALITY);
    }

    pub fn set_ssao_quality(&mut self, quality: u32) {
        self.graphics_ssao_quality = quality.min(MAX_GRAPHICS_QUALITY);
    }

    pub fn set_window_mode(&mut self, mode: WindowMode) {
        self.window_mode = mode;
    }

    pub fn set_window_resolution(&mut self, width: u32, height: u32) {
        self.window_resolution = (width, height);
    }

    // --- Builder methods (for construction in tests) ---

    pub fn with_mouse_sensitivity(mut self, v: f32) -> Self {
        self.mouse_sensitivity = v;
        self
    }
    pub fn with_mouse_smoothing(mut self, v: bool) -> Self {
        self.mouse_smoothing = v;
        self
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.0,
            mouse_smoothing: false,
            // Audio volumes use a 0 to 10 scale
            audio_sound_effect_volume: 8.0,
            audio_music_volume: 6.0,
            audio_ui_volume: 7.0,
            audio_voice_volume: 9.0,
            // Default graphics settings (High)
            graphics_shadow_quality: 3,
            graphics_ssao_quality: 3,
            // Default video settings
            window_mode: WindowMode::Windowed,
            window_resolution: (1920, 1080),
            // Default streaming settings
            world_load_radius: 8,
            world_unload_radius: 12,
            world_chunks_per_frame: 4,
            bindings: BTreeMap::new(),
            debug: BTreeMap::new(),
        }
    }
}

impl Prefs {
    pub fn config_path() -> PathBuf {
        PathBuf::from("config/prefs.ini")
    }

    /// Load preferences from config/prefs.ini, logging a warning for each fallback.
    pub fn load() -> Self {
        let (prefs, warnings) = Self::load_from(&Self::config_path());
        for w in warnings {
            tracing::warn!(warning = %w, "Preferences load fallback");
        }
        prefs
    }

    pub fn to_ini_string(&self) -> String {
        let mut text = format!(
            "[prefs]\n\
             mouse_sensitivity={mouse_sensitivity}\n\
             mouse_smoothing={mouse_smoothing}\n\
             \n[audio]\n\
             sound_effect_volume={sfx:.1}\n\
             music_volume={music:.1}\n\
             ui_volume={ui:.1}\n\
             voice_volume={voice:.1}\n\
             \n[graphics]\n\
             shadow_quality={shadow}\n\
             ssao_quality={ssao}\n\
             \n[video]\n\
             window_mode={window_mode}\n\
             window_width={width}\n\
             window_height={height}\n\
             \n[world]\n\
             load_radius={load}\n\
             unload_radius={unload}\n\
             chunks_per_frame={chunks}\n",
            mouse_sensitivity = self.mouse_sensitivity,
            mouse_smoothing = self.mouse_smoothing,
            sfx = self.audio_sound_effect_volume,
            music = self.audio_music_volume,
            ui = self.audio_ui_volume,
            voice = self.audio_voice_volume,
            shadow = self.graphics_shadow_quality,
            ssao = self.graphics_ssao_quality,
            window_mode = self.window_mode.as_str(),
            width = self.window_resolution.0,
            height = self.window_resolution.1,
            load = self.world_load_radius,
            unload = self.world_unload_radius,
            chunks = self.world_chunks_per_frame,
        );
        if !self.bindings.is_empty() {
            text.push_str("\n[bindings]\n");
            text.extend(
                self.bindings
                    .iter()
                    .map(|(action, value)| format!("{action}={value}\n")),
            );
        }
        if !self.debug.is_empty() {
            text.push_str("\n[debug]\n");
            text.extend(
                self.debug
                    .iter()
                    .map(|(name, value)| format!("{name}={value}\n")),
            );
        }
        text
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to(&Self::config_path())
    }

    /// Write the preferences to `path`, creating its parent directory.
    ///
    /// # Errors
    /// Returns the I/O error if the directory or file cannot be written.
    pub fn save_to(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, self.to_ini_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Restores the working directory when dropped, even if the test panics.
    struct CwdGuard(PathBuf);

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    /// `load` and `save` read and write the working-directory-relative `config_path()`.
    /// No other test depends on the working directory, and nextest runs each test in
    /// its own process.
    #[test]
    fn save_then_load_round_trips_through_config_path() {
        let dir = tempfile::tempdir().unwrap();
        let _restore = CwdGuard(std::env::current_dir().unwrap());
        std::env::set_current_dir(dir.path()).unwrap();
        let mut prefs = Prefs::default();
        *prefs.mouse_sensitivity_mut() = 2.5;
        prefs.set_window_mode(WindowMode::Fullscreen);

        prefs.save().unwrap();
        let loaded = Prefs::load();

        assert!(dir.path().join("config/prefs.ini").is_file());
        assert_eq!(loaded, prefs);
    }

    #[test]
    fn default_prefs_have_mouse_smoothing_off() {
        assert!(!Prefs::default().mouse_smoothing());
    }

    #[test]
    fn with_mouse_sensitivity_sets_the_value() {
        let prefs = Prefs::default().with_mouse_sensitivity(2.5);

        assert_eq!(prefs.mouse_sensitivity(), 2.5);
    }

    #[test]
    fn with_mouse_smoothing_sets_the_flag() {
        assert!(
            Prefs::default()
                .with_mouse_smoothing(true)
                .mouse_smoothing()
        );
        assert!(
            !Prefs::default()
                .with_mouse_smoothing(true)
                .with_mouse_smoothing(false)
                .mouse_smoothing()
        );
    }

    #[test]
    fn window_mode_parse_accepts_only_exact_names() {
        let cases = [
            ("Windowed", Some(WindowMode::Windowed)),
            ("Fullscreen", Some(WindowMode::Fullscreen)),
            ("Borderless", Some(WindowMode::Borderless)),
            ("windowed", None),
            ("fullscreen", None),
            ("unknown", None),
            ("", None),
        ];

        for (input, expected) in cases {
            assert_eq!(WindowMode::parse(input), expected, "input {input:?}");
        }
    }

    #[test]
    fn bindings_section_round_trips() {
        let raw: BTreeMap<String, String> = [
            ("move_forward", "Z"),
            ("jump", "Unbound"),
            ("sprint", "Shift, Ctrl"),
            ("not_an_action", "Banana"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let mut prefs = Prefs::default();
        prefs.set_bindings(raw.clone());

        let (parsed, issues) = Prefs::parse(&prefs.to_ini_string());

        assert_eq!(issues, vec![]);
        assert_eq!(parsed.bindings(), &raw);
    }

    #[test]
    fn a_debug_value_survives_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("game.ini");
        let mut prefs = Prefs::default();
        prefs.set_debug_value("god", "1").unwrap();

        prefs.save_to(&path).unwrap();
        let (loaded, warnings) = Prefs::load_from(&path);

        assert_eq!(warnings, vec![]);
        assert_eq!(loaded.debug_value("god"), Some("1"));
        assert_eq!(loaded, prefs);
    }

    #[test]
    fn storing_a_debug_value_again_overwrites_it() {
        let mut prefs = Prefs::default();
        prefs.set_debug_value("god", "0").unwrap();
        prefs.set_debug_value("tgm", "1").unwrap();

        prefs.set_debug_value("god", "1").unwrap();
        assert_eq!(prefs.debug_value("god"), Some("1"));

        prefs.set_debug_value("tgm", "0").unwrap();
        assert_eq!(prefs.debug_value("tgm"), Some("0"));
        assert_eq!(prefs.debug_value("god"), Some("1"));
    }

    #[test]
    fn an_unstored_debug_value_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.ini");
        let mut prefs = Prefs::default();
        assert_eq!(prefs.debug_value("god"), None);
        prefs.set_debug_value("tgm", "on").unwrap();
        prefs.save_to(&path).unwrap();

        let (loaded, _) = Prefs::load_from(&path);

        assert_eq!(loaded.debug_value("god"), None);
        assert_eq!(loaded.debug_value("tgm"), Some("on"));
    }

    #[test]
    fn invalid_debug_value_is_refused() {
        let mut prefs = Prefs::default();
        prefs.set_debug_value("kept", "yes").unwrap();
        let before = prefs.clone();
        let bad_names = ["God", "a b", "", "a-b", "a=b", "a;b", "é", "a\n"];
        let bad_values = [
            "", "a;b", "a#b", "a\nb", "a\rb", " 1", "1 ", "\t1", "1\n", "  ",
        ];

        for name in bad_names {
            let result = prefs.set_debug_value(name, "1");

            assert!(
                matches!(result, Err(DebugValueError::InvalidName(_))),
                "name {name:?}: {result:?}"
            );
            assert_eq!(prefs, before, "name {name:?} changed prefs");
        }
        for value in bad_values {
            let result = prefs.set_debug_value("kept", value);

            assert!(
                matches!(result, Err(DebugValueError::InvalidValue(_))),
                "value {value:?}: {result:?}"
            );
            assert_eq!(prefs, before, "value {value:?} changed prefs");
        }
    }

    #[test]
    fn no_game_section_is_written_when_empty() {
        let text = Prefs::default().to_ini_string();

        assert!(!text.contains("[debug]"), "{text}");
    }

    proptest::proptest! {
        #[test]
        fn accepted_debug_values_round_trip(
            name in "[a-z0-9_]{1,16}",
            value in "[A-Za-z0-9_.,:/=+-]([A-Za-z0-9_.,:/=+ -]{0,12}[A-Za-z0-9_.,:/=+-])?",
        ) {
            let mut prefs = Prefs::default();
            prefs.set_debug_value(&name, &value).unwrap();

            let (parsed, issues) = Prefs::parse(&prefs.to_ini_string());

            proptest::prop_assert_eq!(issues, vec![]);
            proptest::prop_assert_eq!(parsed.debug_value(&name), Some(value.as_str()));
            proptest::prop_assert_eq!(parsed, prefs);
        }
    }

    #[test]
    fn hand_written_bindings_section_is_read_verbatim() {
        let (prefs, issues) = Prefs::parse("[bindings]\njump = F\nsprint = Shift, Ctrl\n");

        let expected: BTreeMap<String, String> = [("jump", "F"), ("sprint", "Shift, Ctrl")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(issues, vec![]);
        assert_eq!(prefs.bindings(), &expected);
    }
}
