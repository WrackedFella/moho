//! INI parsing and binding serialization for preferences.
//!
//! This module handles parsing human-readable binding strings (e.g., "Ctrl+W", "ArrowUp")
//! into Binding structs and serializing them back to strings for INI files.
//!
//! Uses PHF (perfect hash functions) for O(1) key name lookups during parsing.

use super::key_names::parse_key_name;
use super::Binding;

/// Parse a human-readable binding string into a Binding struct.
///
/// Supports formats like:
/// - "Ctrl+W" - Key with modifiers
/// - "ArrowUp" - Special keys
/// - "Shift" - Modifier-only bindings
/// - "Unbound" - No binding (0, 0)
/// - "W" - Single character keys
///
/// # Arguments
/// * `s` - The binding string to parse
/// * `fallback` - The binding to return if parsing fails
///
/// # Returns
/// A Binding representing the parsed string, or the fallback on error
///
/// # Examples
/// ```ignore
/// let binding = parse_binding("Ctrl+W", Binding::default());
/// assert_eq!(binding.code, 'W' as u32);
/// assert_eq!(binding.mods, 1); // Ctrl bit
/// ```
pub fn parse_binding(s: &str, fallback: Binding) -> Binding {
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

    // If only modifiers (no key part), return modifier-only binding
    if key_part.is_empty() {
        if mods != 0 {
            return Binding::new(0, mods);
        }
        return fallback;
    }

    let code = parse_key_name(key_part, fallback.code);
    Binding::new(code, mods)
}

/// Convert a Binding to a human-readable string for INI serialization.
///
/// # Arguments
/// * `binding` - The binding to convert
///
/// # Returns
/// A human-readable string like "Ctrl+W" or "ArrowUp"
///
/// # Examples
/// ```ignore
/// let binding = Binding::new('W' as u32, 1); // Ctrl+W
/// assert_eq!(binding_to_string(&binding), "Ctrl+W");
/// ```
pub fn binding_to_string(binding: &Binding) -> String {
    if binding.code == 0 && binding.mods == 0 {
        return "Unbound".to_string();
    }

    let mut s = String::new();

    // If only modifiers are set (no key code), show just the modifier
    if binding.code == 0 {
        if binding.mods & 1 != 0 {
            s.push_str("Ctrl");
        }
        if binding.mods & 2 != 0 {
            if !s.is_empty() {
                s.push('+');
            }
            s.push_str("Shift");
        }
        if binding.mods & 4 != 0 {
            if !s.is_empty() {
                s.push('+');
            }
            s.push_str("Alt");
        }
        return s;
    }

    // Add modifiers prefix
    if binding.mods & 1 != 0 {
        s.push_str("Ctrl+");
    }
    if binding.mods & 2 != 0 {
        s.push_str("Shift+");
    }
    if binding.mods & 4 != 0 {
        s.push_str("Alt+");
    }

    // Handle Space specially
    if binding.code == ' ' as u32 {
        s.push_str("Spacebar");
        return s;
    }

    // Handle regular ASCII characters
    if let Some(ch) = std::char::from_u32(binding.code)
        && ch.is_ascii_graphic()
    {
        s.push(ch.to_ascii_uppercase());
        return s;
    }

    // Handle special keys
    match binding.code {
        0x100 => s.push_str("ArrowUp"),
        0x101 => s.push_str("ArrowDown"),
        0x102 => s.push_str("ArrowLeft"),
        0x103 => s.push_str("ArrowRight"),
        0x200 => s.push_str("Escape"),
        0x201 => s.push_str("Tab"),
        0x202 => s.push_str("Backspace"),
        0x203 => s.push_str("Enter"),
        0x204 => s.push_str("Shift"),
        0x205 => s.push_str("Ctrl"),
        0x206 => s.push_str("Alt"),
        _ => s.push_str("Unknown"),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_key() {
        let fallback = Binding::default();
        let binding = parse_binding("W", fallback);
        assert_eq!(binding.code, 'W' as u32);
        assert_eq!(binding.mods, 0);
    }

    #[test]
    fn test_parse_ctrl_key() {
        let fallback = Binding::default();
        let binding = parse_binding("Ctrl+W", fallback);
        assert_eq!(binding.code, 'W' as u32);
        assert_eq!(binding.mods, 1);
    }

    #[test]
    fn test_parse_shift_key() {
        let fallback = Binding::default();
        let binding = parse_binding("Shift+A", fallback);
        assert_eq!(binding.code, 'A' as u32);
        assert_eq!(binding.mods, 2);
    }

    #[test]
    fn test_parse_alt_key() {
        let fallback = Binding::default();
        let binding = parse_binding("Alt+D", fallback);
        assert_eq!(binding.code, 'D' as u32);
        assert_eq!(binding.mods, 4);
    }

    #[test]
    fn test_parse_multiple_modifiers() {
        let fallback = Binding::default();
        let binding = parse_binding("Ctrl+Shift+W", fallback);
        assert_eq!(binding.code, 'W' as u32);
        assert_eq!(binding.mods, 3); // Ctrl (1) + Shift (2)
    }

    #[test]
    fn test_parse_special_keys() {
        let fallback = Binding::default();

        let up = parse_binding("ArrowUp", fallback);
        assert_eq!(up.code, 0x100);

        let down = parse_binding("ArrowDown", fallback);
        assert_eq!(down.code, 0x101);

        let space = parse_binding("Spacebar", fallback);
        assert_eq!(space.code, ' ' as u32);

        let esc = parse_binding("Escape", fallback);
        assert_eq!(esc.code, 0x200);
    }

    #[test]
    fn test_parse_modifier_only() {
        let fallback = Binding::default();
        let binding = parse_binding("Shift", fallback);
        assert_eq!(binding.code, 0);
        assert_eq!(binding.mods, 2);
    }

    #[test]
    fn test_parse_unbound() {
        let fallback = Binding::new('W' as u32, 0);
        let binding = parse_binding("Unbound", fallback);
        assert_eq!(binding.code, 0);
        assert_eq!(binding.mods, 0);
    }

    #[test]
    fn test_parse_empty_returns_fallback() {
        let fallback = Binding::new('X' as u32, 1);
        let binding = parse_binding("", fallback);
        assert_eq!(binding.code, 'X' as u32);
        assert_eq!(binding.mods, 1);
    }

    #[test]
    fn test_parse_case_insensitive() {
        let fallback = Binding::default();

        let ctrl = parse_binding("ctrl+w", fallback);
        assert_eq!(ctrl.code, 'W' as u32);
        assert_eq!(ctrl.mods, 1);

        let alt = parse_binding("ALT+D", fallback);
        assert_eq!(alt.code, 'D' as u32);
        assert_eq!(alt.mods, 4);
    }

    #[test]
    fn test_binding_to_string_simple() {
        let binding = Binding::new('W' as u32, 0);
        assert_eq!(binding_to_string(&binding), "W");
    }

    #[test]
    fn test_binding_to_string_ctrl() {
        let binding = Binding::new('W' as u32, 1);
        assert_eq!(binding_to_string(&binding), "Ctrl+W");
    }

    #[test]
    fn test_binding_to_string_shift() {
        let binding = Binding::new('A' as u32, 2);
        assert_eq!(binding_to_string(&binding), "Shift+A");
    }

    #[test]
    fn test_binding_to_string_alt() {
        let binding = Binding::new('D' as u32, 4);
        assert_eq!(binding_to_string(&binding), "Alt+D");
    }

    #[test]
    fn test_binding_to_string_multiple_mods() {
        let binding = Binding::new('W' as u32, 3); // Ctrl+Shift
        assert_eq!(binding_to_string(&binding), "Ctrl+Shift+W");
    }

    #[test]
    fn test_binding_to_string_special_keys() {
        let up = Binding::new(0x100, 0);
        assert_eq!(binding_to_string(&up), "ArrowUp");

        let space = Binding::new(' ' as u32, 0);
        assert_eq!(binding_to_string(&space), "Spacebar");

        let esc = Binding::new(0x200, 0);
        assert_eq!(binding_to_string(&esc), "Escape");
    }

    #[test]
    fn test_binding_to_string_modifier_only() {
        let shift = Binding::new(0, 2);
        assert_eq!(binding_to_string(&shift), "Shift");

        let ctrl_alt = Binding::new(0, 5); // Ctrl (1) + Alt (4)
        assert_eq!(binding_to_string(&ctrl_alt), "Ctrl+Alt");
    }

    #[test]
    fn test_binding_to_string_unbound() {
        let binding = Binding::new(0, 0);
        assert_eq!(binding_to_string(&binding), "Unbound");
    }

    #[test]
    fn test_roundtrip_parsing() {
        let original = Binding::new('W' as u32, 1);
        let string = binding_to_string(&original);
        let parsed = parse_binding(&string, Binding::default());
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_roundtrip_special_keys() {
        let original = Binding::new(0x100, 3); // Ctrl+Shift+ArrowUp
        let string = binding_to_string(&original);
        let parsed = parse_binding(&string, Binding::default());
        assert_eq!(parsed, original);
    }
}
