//! Preferences management for the game.
//!
//! This module handles loading, saving, and managing user preferences including
//! key bindings, mouse sensitivity, input filtering, audio volumes, and video settings.

mod key_names;
mod parser;

use std::fs;
use std::path::PathBuf;

pub use key_names::parse_key_name;
pub use parser::{binding_to_string, parse_binding};

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

    fn from_str(s: &str) -> Self {
        match s {
            "Fullscreen" => WindowMode::Fullscreen,
            "Borderless" => WindowMode::Borderless,
            _ => WindowMode::Windowed,
        }
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

    pub fn with_window_mode(mut self, mode: WindowMode) -> Self {
        self.window_mode = mode;
        self
    }

    pub fn with_window_resolution(mut self, width: u32, height: u32) -> Self {
        self.window_resolution = (width, height);
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

    /// Load preferences from config/prefs.ini. If the file doesn't exist,
    /// create it with defaults and return defaults.
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let def = Prefs::default();
            let _ = def.save();
            return def;
        }

        let content = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return Prefs::default(),
        };

        let mut prefs = Prefs::default();
        if let Ok(map) = ini::macro_safe_read(&content)
            && let Some(section) = map.get("prefs").or_else(|| map.get("default"))
        {
            let get_str = |k: &str| section.get(k).and_then(|o| o.clone());
            let get_f32 = |k: &str, def: f32| {
                section
                    .get(k)
                    .and_then(|o| o.clone())
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(def)
            };

            // Use parser module for binding parsing
            if let Some(s) = get_str("key_w") {
                prefs.key_w = parser::parse_binding(&s, prefs.key_w);
            }
            if let Some(s) = get_str("key_a") {
                prefs.key_a = parser::parse_binding(&s, prefs.key_a);
            }
            if let Some(s) = get_str("key_s") {
                prefs.key_s = parser::parse_binding(&s, prefs.key_s);
            }
            if let Some(s) = get_str("key_d") {
                prefs.key_d = parser::parse_binding(&s, prefs.key_d);
            }
            if let Some(s) = get_str("key_up") {
                prefs.key_up = parser::parse_binding(&s, prefs.key_up);
            }
            if let Some(s) = get_str("key_down") {
                prefs.key_down = parser::parse_binding(&s, prefs.key_down);
            }
            if let Some(s) = get_str("key_sprint") {
                prefs.key_sprint = parser::parse_binding(&s, prefs.key_sprint);
            }
            if let Some(s) = get_str("key_jump") {
                prefs.key_jump = parser::parse_binding(&s, prefs.key_jump);
            }
            prefs.mouse_sensitivity = get_f32("mouse_sensitivity", prefs.mouse_sensitivity);

            if let Some(s) = get_str("input_filtering_enabled") {
                prefs.input_filtering_enabled = s.to_lowercase() == "true" || s == "1";
            }
        }

        // Load audio settings from [audio] section if present
        if let Ok(map) = ini::macro_safe_read(&content)
            && let Some(audio_section) = map.get("audio")
        {
            let get_f32 = |k: &str, def: f32| {
                audio_section
                    .get(k)
                    .and_then(|o| o.clone())
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(def)
            };

            prefs.audio_sound_effect_volume =
                get_f32("sound_effect_volume", prefs.audio_sound_effect_volume);
            prefs.audio_music_volume = get_f32("music_volume", prefs.audio_music_volume);
            prefs.audio_ui_volume = get_f32("ui_volume", prefs.audio_ui_volume);
            prefs.audio_voice_volume = get_f32("voice_volume", prefs.audio_voice_volume);
        }

        // Load graphics settings from [graphics] section if present
        if let Ok(map) = ini::macro_safe_read(&content)
            && let Some(graphics_section) = map.get("graphics")
        {
            let get_u32 = |k: &str, def: u32| {
                graphics_section
                    .get(k)
                    .and_then(|o| o.clone())
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(def)
            };

            prefs.graphics_shadow_quality =
                get_u32("shadow_quality", prefs.graphics_shadow_quality);
            prefs.graphics_ssao_quality = get_u32("ssao_quality", prefs.graphics_ssao_quality);
        }

        // Load video settings from [video] section if present
        if let Ok(map) = ini::macro_safe_read(&content)
            && let Some(video_section) = map.get("video")
        {
            let get_str = |k: &str| video_section.get(k).and_then(|o| o.clone());
            let get_u32 = |k: &str, def: u32| {
                video_section
                    .get(k)
                    .and_then(|o| o.clone())
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(def)
            };

            if let Some(s) = get_str("window_mode") {
                prefs.window_mode = WindowMode::from_str(&s);
            }
            let w = get_u32("window_width", prefs.window_resolution.0);
            let h = get_u32("window_height", prefs.window_resolution.1);
            prefs.window_resolution = (w, h);
        }

        // Load world / streaming settings from [world] section if present
        if let Ok(map) = ini::macro_safe_read(&content)
            && let Some(world_section) = map.get("world")
        {
            let get_u32 = |k: &str, def: u32| {
                world_section
                    .get(k)
                    .and_then(|o| o.clone())
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(def)
            };
            prefs.world_load_radius = get_u32("load_radius", prefs.world_load_radius);
            prefs.world_unload_radius = get_u32("unload_radius", prefs.world_unload_radius);
            prefs.world_chunks_per_frame =
                get_u32("chunks_per_frame", prefs.world_chunks_per_frame).max(1);
        }

        prefs
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = String::new();

        // Controls section
        out.push_str("[prefs]\n");

        // Use parser module for binding serialization
        out.push_str(&format!(
            "key_w={}\n",
            parser::binding_to_string(&self.key_w)
        ));
        out.push_str(&format!(
            "key_a={}\n",
            parser::binding_to_string(&self.key_a)
        ));
        out.push_str(&format!(
            "key_s={}\n",
            parser::binding_to_string(&self.key_s)
        ));
        out.push_str(&format!(
            "key_d={}\n",
            parser::binding_to_string(&self.key_d)
        ));
        out.push_str(&format!(
            "key_up={}\n",
            parser::binding_to_string(&self.key_up)
        ));
        out.push_str(&format!(
            "key_down={}\n",
            parser::binding_to_string(&self.key_down)
        ));
        out.push_str(&format!(
            "key_sprint={}\n",
            parser::binding_to_string(&self.key_sprint)
        ));
        out.push_str(&format!(
            "key_jump={}\n",
            parser::binding_to_string(&self.key_jump)
        ));
        out.push_str(&format!("mouse_sensitivity={}\n", self.mouse_sensitivity));
        out.push_str(&format!(
            "input_filtering_enabled={}\n",
            self.input_filtering_enabled
        ));

        // Audio section
        out.push_str("\n[audio]\n");
        out.push_str(&format!(
            "sound_effect_volume={:.1}\n",
            self.audio_sound_effect_volume
        ));
        out.push_str(&format!("music_volume={:.1}\n", self.audio_music_volume));
        out.push_str(&format!("ui_volume={:.1}\n", self.audio_ui_volume));
        out.push_str(&format!("voice_volume={:.1}\n", self.audio_voice_volume));

        // Graphics section
        out.push_str("\n[graphics]\n");
        out.push_str(&format!(
            "shadow_quality={}\n",
            self.graphics_shadow_quality
        ));
        out.push_str(&format!("ssao_quality={}\n", self.graphics_ssao_quality));

        // Video section
        out.push_str("\n[video]\n");
        out.push_str(&format!("window_mode={}\n", self.window_mode.as_str()));
        out.push_str(&format!("window_width={}\n", self.window_resolution.0));
        out.push_str(&format!("window_height={}\n", self.window_resolution.1));

        // World / streaming section
        out.push_str("\n[world]\n");
        out.push_str(&format!("load_radius={}\n", self.world_load_radius));
        out.push_str(&format!("unload_radius={}\n", self.world_unload_radius));
        out.push_str(&format!(
            "chunks_per_frame={}\n",
            self.world_chunks_per_frame
        ));

        fs::write(path, out)?;
        Ok(())
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
    fn test_binding_new() {
        let binding = Binding::new('A' as u32, 1);
        assert_eq!(binding.code, 'A' as u32);
        assert_eq!(binding.mods, 1);
    }

    #[test]
    fn test_binding_default() {
        let binding = Binding::default();
        assert_eq!(binding.code, 'W' as u32);
        assert_eq!(binding.mods, 0);
    }

    #[test]
    fn test_config_path() {
        let path = Prefs::config_path();
        assert_eq!(path.to_str().unwrap(), "config/prefs.ini");
    }

    #[test]
    fn test_default_video_settings() {
        let prefs = Prefs::default();
        assert_eq!(prefs.window_mode(), WindowMode::Windowed);
        assert_eq!(prefs.window_resolution(), (1920, 1080));
    }

    #[test]
    fn test_video_round_trip_windowed() {
        // Build a prefs with custom video settings
        let prefs = Prefs::default()
            .with_window_mode(WindowMode::Windowed)
            .with_window_resolution(2560, 1440);

        // Serialize to INI string directly (without touching disk)
        let mut out = String::new();
        out.push_str("[video]\n");
        out.push_str(&format!("window_mode={}\n", prefs.window_mode().as_str()));
        out.push_str(&format!("window_width={}\n", prefs.window_resolution().0));
        out.push_str(&format!("window_height={}\n", prefs.window_resolution().1));

        // Parse back
        if let Ok(map) = ini::macro_safe_read(&out)
            && let Some(section) = map.get("video")
        {
            let mode_str = section
                .get("window_mode")
                .and_then(|o| o.clone())
                .unwrap_or_default();
            let w: u32 = section
                .get("window_width")
                .and_then(|o| o.clone())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let h: u32 = section
                .get("window_height")
                .and_then(|o| o.clone())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            assert_eq!(WindowMode::from_str(&mode_str), WindowMode::Windowed);
            assert_eq!((w, h), (2560, 1440));
        } else {
            panic!("Failed to parse video section");
        }
    }

    #[test]
    fn test_video_round_trip_fullscreen() {
        let prefs = Prefs::default()
            .with_window_mode(WindowMode::Fullscreen)
            .with_window_resolution(1920, 1080);

        let mut out = String::new();
        out.push_str("[video]\n");
        out.push_str(&format!("window_mode={}\n", prefs.window_mode().as_str()));
        out.push_str(&format!("window_width={}\n", prefs.window_resolution().0));
        out.push_str(&format!("window_height={}\n", prefs.window_resolution().1));

        if let Ok(map) = ini::macro_safe_read(&out)
            && let Some(section) = map.get("video")
        {
            let mode_str = section
                .get("window_mode")
                .and_then(|o| o.clone())
                .unwrap_or_default();
            assert_eq!(WindowMode::from_str(&mode_str), WindowMode::Fullscreen);
        } else {
            panic!("Failed to parse video section");
        }
    }

    #[test]
    fn test_window_mode_from_str() {
        assert_eq!(WindowMode::from_str("Windowed"), WindowMode::Windowed);
        assert_eq!(WindowMode::from_str("Fullscreen"), WindowMode::Fullscreen);
        assert_eq!(WindowMode::from_str("Borderless"), WindowMode::Borderless);
        assert_eq!(WindowMode::from_str("unknown"), WindowMode::Windowed); // default
    }
}
