//! Preferences management for the game.
//!
//! This module handles loading, saving, and managing user preferences including
//! key bindings, mouse sensitivity, input filtering, audio volumes, and video settings.

mod key_names;
mod parser;
mod reader;

use std::fs;
use std::path::{Path, PathBuf};

pub use key_names::parse_key_name;
pub use parser::{binding_to_string, parse_binding};
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

/// A numeric key binding: key code and modifier bits.
/// mods bitflags: bit0 = ctrl, bit1 = shift, bit2 = alt
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub code: u32,
    pub mods: u8,
}

impl Binding {
    pub fn new(code: u32, mods: u8) -> Self {
        Self { code, mods }
    }
}

impl Default for Binding {
    fn default() -> Self {
        Binding::new('W' as u32, 0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    key_w: Binding,
    key_a: Binding,
    key_s: Binding,
    key_d: Binding,
    key_up: Binding,
    key_down: Binding,
    key_sprint: Binding,
    key_jump: Binding,
    mouse_sensitivity: f32,
    input_filtering_enabled: bool,
    // Audio settings (values 1.0 to 10.0)
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
}

const MAX_GRAPHICS_QUALITY: u32 = 4;

impl Prefs {
    // --- Binding accessors ---

    pub fn key_w(&self) -> Binding {
        self.key_w
    }
    pub fn key_a(&self) -> Binding {
        self.key_a
    }
    pub fn key_s(&self) -> Binding {
        self.key_s
    }
    pub fn key_d(&self) -> Binding {
        self.key_d
    }
    pub fn key_up(&self) -> Binding {
        self.key_up
    }
    pub fn key_down(&self) -> Binding {
        self.key_down
    }

    pub fn key_sprint(&self) -> Binding {
        self.key_sprint
    }

    pub fn key_jump(&self) -> Binding {
        self.key_jump
    }

    pub fn set_key_w(&mut self, b: Binding) {
        self.key_w = b;
    }
    pub fn set_key_a(&mut self, b: Binding) {
        self.key_a = b;
    }
    pub fn set_key_s(&mut self, b: Binding) {
        self.key_s = b;
    }
    pub fn set_key_d(&mut self, b: Binding) {
        self.key_d = b;
    }
    pub fn set_key_up(&mut self, b: Binding) {
        self.key_up = b;
    }
    pub fn set_key_down(&mut self, b: Binding) {
        self.key_down = b;
    }

    pub fn set_key_sprint(&mut self, b: Binding) {
        self.key_sprint = b;
    }

    pub fn set_key_jump(&mut self, b: Binding) {
        self.key_jump = b;
    }

    // --- Scalar getters ---

    pub fn mouse_sensitivity(&self) -> f32 {
        self.mouse_sensitivity
    }
    pub fn input_filtering_enabled(&self) -> bool {
        self.input_filtering_enabled
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
    pub fn input_filtering_enabled_mut(&mut self) -> &mut bool {
        &mut self.input_filtering_enabled
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

    pub fn with_key_w(mut self, b: Binding) -> Self {
        self.key_w = b;
        self
    }
    pub fn with_key_a(mut self, b: Binding) -> Self {
        self.key_a = b;
        self
    }
    pub fn with_mouse_sensitivity(mut self, v: f32) -> Self {
        self.mouse_sensitivity = v;
        self
    }
    pub fn with_input_filtering_enabled(mut self, v: bool) -> Self {
        self.input_filtering_enabled = v;
        self
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            key_w: Binding::new('W' as u32, 0),
            key_a: Binding::new('A' as u32, 0),
            key_s: Binding::new('S' as u32, 0),
            key_d: Binding::new('D' as u32, 0),
            key_up: Binding::new(' ' as u32, 0),   // Space
            key_down: Binding::new(0x205, 0),      // Ctrl
            key_sprint: Binding::new(0x204, 0),    // Shift
            key_jump: Binding::new(' ' as u32, 0), // Space
            mouse_sensitivity: 1.0,
            input_filtering_enabled: true,
            // Default audio volumes (mid-range)
            audio_sound_effect_volume: 7.0,
            audio_music_volume: 5.0,
            audio_ui_volume: 8.0,
            audio_voice_volume: 7.0,
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
            log::warn!("{w}");
        }
        prefs
    }

    pub fn to_ini_string(&self) -> String {
        let bind = parser::binding_to_string;
        format!(
            "[prefs]\n\
             key_w={key_w}\n\
             key_a={key_a}\n\
             key_s={key_s}\n\
             key_d={key_d}\n\
             key_up={key_up}\n\
             key_down={key_down}\n\
             key_sprint={key_sprint}\n\
             key_jump={key_jump}\n\
             mouse_sensitivity={mouse_sensitivity}\n\
             input_filtering_enabled={input_filtering_enabled}\n\
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
            key_w = bind(&self.key_w),
            key_a = bind(&self.key_a),
            key_s = bind(&self.key_s),
            key_d = bind(&self.key_d),
            key_up = bind(&self.key_up),
            key_down = bind(&self.key_down),
            key_sprint = bind(&self.key_sprint),
            key_jump = bind(&self.key_jump),
            mouse_sensitivity = self.mouse_sensitivity,
            input_filtering_enabled = self.input_filtering_enabled,
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
        )
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to(&Self::config_path())
    }

    fn save_to(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, self.to_ini_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_prefs() {
        let prefs = Prefs::default();
        assert_eq!(prefs.key_w().code, 'W' as u32);
        assert_eq!(prefs.mouse_sensitivity(), 1.0);
        assert!(prefs.input_filtering_enabled());
    }

    #[test]
    fn test_binding_default() {
        let binding = Binding::default();
        assert_eq!(binding.code, 'W' as u32);
        assert_eq!(binding.mods, 0);
    }

    #[test]
    fn test_default_video_settings() {
        let prefs = Prefs::default();
        assert_eq!(prefs.window_mode(), WindowMode::Windowed);
        assert_eq!(prefs.window_resolution(), (1920, 1080));
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
}
