use std::fs;
use std::path::PathBuf;

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
    pub mouse_sensitivity: f32,
    pub input_filtering_enabled: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            key_w: Binding::new('W' as u32, 0),
            key_a: Binding::new('A' as u32, 0),
            key_s: Binding::new('S' as u32, 0),
            key_d: Binding::new('D' as u32, 0),
            mouse_sensitivity: 1.0,
            input_filtering_enabled: true,
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

                // Parse human-readable bindings like "Ctrl+W" or "ArrowUp".
                fn parse_binding(s: &str, fallback: Binding) -> Binding {
                    let s = s.trim();
                    if s.is_empty() {
                        return fallback;
                    }
                    if s.eq_ignore_ascii_case("unbound") {
                        return Binding::new(0, 0);
                    }
                    let mut mods: u8 = 0;
                    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
                    let mut key_part = "";
                    for p in &parts {
                        let up = p.to_ascii_uppercase();
                        match up.as_str() {
                            "CTRL" | "CONTROL" => {
                                mods |= 1;
                                continue;
                            }
                            "SHIFT" => {
                                mods |= 2;
                                continue;
                            }
                            "ALT" => {
                                mods |= 4;
                                continue;
                            }
                            _ => {
                                key_part = p.trim();
                            }
                        }
                    }
                    if key_part.is_empty() {
                        return fallback;
                    }

                    let code = match key_part {
                        "ARROWUP" | "UP" => 0x100,
                        "ARROWDOWN" | "DOWN" => 0x101,
                        "ARROWLEFT" | "LEFT" => 0x102,
                        "ARROWRIGHT" | "RIGHT" => 0x103,
                        "ESC" | "ESCAPE" => 0x200,
                        "TAB" => 0x201,
                        "BACKSPACE" => 0x202,
                        "ENTER" | "RETURN" => 0x203,
                        "SPACE" => ' ' as u32,
                        s if s.len() == 1 => s.chars().next().unwrap() as u32,
                        _ => {
                            // try parse numeric code as fallback
                            key_part.parse::<u32>().unwrap_or(fallback.code)
                        }
                    };
                    Binding::new(code, mods)
                }

                if let Some(s) = get_str("key_w") {
                    prefs.key_w = parse_binding(&s, prefs.key_w);
                }
                if let Some(s) = get_str("key_a") {
                    prefs.key_a = parse_binding(&s, prefs.key_a);
                }
                if let Some(s) = get_str("key_s") {
                    prefs.key_s = parse_binding(&s, prefs.key_s);
                }
                if let Some(s) = get_str("key_d") {
                    prefs.key_d = parse_binding(&s, prefs.key_d);
                }
                prefs.mouse_sensitivity = get_f32("mouse_sensitivity", prefs.mouse_sensitivity);

                if let Some(s) = get_str("input_filtering_enabled") {
                    prefs.input_filtering_enabled = s.to_lowercase() == "true" || s == "1";
                }
        }

        prefs
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = String::new();
        out.push_str("[prefs]\n");
        // store human-readable bindings, e.g. "Ctrl+W" or "ArrowUp"
        fn binding_to_string(b: &Binding) -> String {
            if b.code == 0 {
                return "Unbound".to_string();
            }
            let mut s = String::new();
            if b.mods & 1 != 0 {
                s.push_str("Ctrl+");
            }
            if b.mods & 2 != 0 {
                s.push_str("Shift+");
            }
            if b.mods & 4 != 0 {
                s.push_str("Alt+");
            }
            if let Some(ch) = std::char::from_u32(b.code)
                && ch.is_ascii_graphic()
            {
                s.push(ch.to_ascii_uppercase());
                return s;
            }
            match b.code {
                0x100 => s.push_str("ArrowUp"),
                0x101 => s.push_str("ArrowDown"),
                0x102 => s.push_str("ArrowLeft"),
                0x103 => s.push_str("ArrowRight"),
                0x200 => s.push_str("Escape"),
                0x201 => s.push_str("Tab"),
                0x202 => s.push_str("Backspace"),
                0x203 => s.push_str("Enter"),
                _ => s.push_str("Unknown"),
            }
            s
        }

        out.push_str(&format!("key_w={}\n", binding_to_string(&self.key_w)));
        out.push_str(&format!("key_a={}\n", binding_to_string(&self.key_a)));
        out.push_str(&format!("key_s={}\n", binding_to_string(&self.key_s)));
        out.push_str(&format!("key_d={}\n", binding_to_string(&self.key_d)));
        out.push_str(&format!("mouse_sensitivity={}\n", self.mouse_sensitivity));
        out.push_str(&format!(
            "input_filtering_enabled={}\n",
            self.input_filtering_enabled
        ));

        fs::write(path, out)?;
        Ok(())
    }
}
