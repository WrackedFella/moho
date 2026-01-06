//! Preferences management for the game.
//!
//! This module handles loading, saving, and managing user preferences including
//! key bindings, mouse sensitivity, input filtering, and audio volumes.

mod key_names;
mod parser;

use std::fs;
use std::path::PathBuf;

pub use key_names::parse_key_name;
pub use parser::{binding_to_string, parse_binding};

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
    pub key_w: Binding,
    pub key_a: Binding,
    pub key_s: Binding,
    pub key_d: Binding,
    pub key_up: Binding,
    pub key_down: Binding,
    pub mouse_sensitivity: f32,
    pub input_filtering_enabled: bool,
    // Audio settings (values 1.0 to 10.0)
    pub audio_sound_effect_volume: f32,
    pub audio_music_volume: f32,
    pub audio_ui_volume: f32,
    pub audio_voice_volume: f32,
    // Graphics settings
    pub graphics_shadow_quality: u32, // 0=Off, 1=Low, 2=Medium, 3=High, 4=Ultra
    pub graphics_ssao_quality: u32,   // 0=Off, 1=Low, 2=Medium, 3=High, 4=Ultra
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            key_w: Binding::new('W' as u32, 0),
            key_a: Binding::new('A' as u32, 0),
            key_s: Binding::new('S' as u32, 0),
            key_d: Binding::new('D' as u32, 0),
            key_up: Binding::new(' ' as u32, 0), // Space
            key_down: Binding::new(0x204, 0),    // Shift
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
        assert_eq!(prefs.key_w.code, 'W' as u32);
        assert_eq!(prefs.mouse_sensitivity, 1.0);
        assert!(prefs.input_filtering_enabled);
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
}
