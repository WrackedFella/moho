use std::path::PathBuf;
use std::fs;

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub key_w: String,
    pub key_a: String,
    pub key_s: String,
    pub key_d: String,
    pub mouse_sensitivity: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            key_w: "W".to_string(),
            key_a: "A".to_string(),
            key_s: "S".to_string(),
            key_d: "D".to_string(),
            mouse_sensitivity: 1.0,
        }
    }
}

impl Prefs {
    pub fn config_path() -> PathBuf {
        PathBuf::from("config/prefs.ini")
    }

    /// Load prefs from config/prefs.ini. If file doesn't exist, create it
    /// with default values and return defaults.
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

        // Use inistr! macro from ini crate to parse a string into nested HashMaps
        // The `safe` variant returns a Result so we can handle parse errors gracefully.
        let mut key_w = "W".to_string();
        let mut key_a = "A".to_string();
        let mut key_s = "S".to_string();
        let mut key_d = "D".to_string();
        let mut mouse_sensitivity = 1.0f32;

        // Use the crate-provided safe parsing function to avoid macro issues here.
        let parsed_res = ini::macro_safe_read(&content);
        if let Ok(map) = parsed_res {
            // map: HashMap<String, HashMap<String, Option<String>>>
            // section names and keys are stored lowercase by the crate
            if let Some(section) = map.get("prefs").or_else(|| map.get("default")) {
                if let Some(opt) = section.get("key_w") {
                    if let Some(v) = opt.clone() { key_w = v; }
                }
                if let Some(opt) = section.get("key_a") {
                    if let Some(v) = opt.clone() { key_a = v; }
                }
                if let Some(opt) = section.get("key_s") {
                    if let Some(v) = opt.clone() { key_s = v; }
                }
                if let Some(opt) = section.get("key_d") {
                    if let Some(v) = opt.clone() { key_d = v; }
                }
                if let Some(opt) = section.get("mouse_sensitivity") {
                    if let Some(v) = opt.clone() {
                        if let Ok(f) = v.parse::<f32>() { mouse_sensitivity = f; }
                    }
                }
            }
        }

        Prefs { key_w, key_a, key_s, key_d, mouse_sensitivity }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = String::new();
        out.push_str("[prefs]\n");
        out.push_str(&format!("key_w={}\n", self.key_w));
        out.push_str(&format!("key_a={}\n", self.key_a));
        out.push_str(&format!("key_s={}\n", self.key_s));
        out.push_str(&format!("key_d={}\n", self.key_d));
        out.push_str(&format!("mouse_sensitivity={}\n", self.mouse_sensitivity));
        fs::write(path, out)?;
        Ok(())
    }
}
