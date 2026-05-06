//! INI parsing and binding serialization for preferences.
//!
//! This module handles parsing human-readable binding strings (e.g., "Ctrl", "ArrowUp")
//! into Binding structs and serializing them back to strings for INI files.
//!
//! Modifier keys (Ctrl, Shift, Alt) are treated as regular key bindings — they
//! produce a key code like any other key, not a modifier bitfield.
//!
//! Uses PHF (perfect hash functions) for O(1) key name lookups during parsing.

use super::Binding;
use super::key_names::parse_key_name;

/// Parse a human-readable binding string into a Binding struct.
///
/// Supports formats like:
/// - "ArrowUp" - Special keys
/// - "Ctrl" - Modifier keys (treated as regular key codes)
/// - "Shift" - Modifier keys (treated as regular key codes)
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
/// let binding = parse_binding("Ctrl", Binding::default());
/// assert_eq!(binding.code, 0x205);
/// assert_eq!(binding.mods, 0);
/// ```
pub fn parse_binding(s: &str, fallback: Binding) -> Binding {
    let s = s.trim();
    if s.is_empty() {
        return fallback;
    }
    if s.eq_ignore_ascii_case("unbound") {
        return Binding::new(0, 0);
    }

    let code = parse_key_name(s, 0);
    if code != 0 {
        return Binding::new(code, 0);
    }

    fallback
}

/// Convert a Binding to a human-readable string for INI serialization.
///
/// # Arguments
/// * `binding` - The binding to convert
///
/// # Returns
/// A human-readable string like "Ctrl" or "ArrowUp"
///
/// # Examples
/// ```ignore
/// let binding = Binding::new(0x205, 0); // Ctrl
/// assert_eq!(binding_to_string(&binding), "Ctrl");
/// ```
pub fn binding_to_string(binding: &Binding) -> String {
    if binding.code == 0 && binding.mods == 0 {
        return "Unbound".to_string();
    }

    // Handle Space specially
    if binding.code == ' ' as u32 {
        return "Spacebar".to_string();
    }

    // Handle regular ASCII characters
    if let Some(ch) = std::char::from_u32(binding.code)
        && ch.is_ascii_graphic()
    {
        return ch.to_ascii_uppercase().to_string();
    }

    // Handle special keys (including modifier keys as key codes)
    match binding.code {
        0x100 => "ArrowUp".to_string(),
        0x101 => "ArrowDown".to_string(),
        0x102 => "ArrowLeft".to_string(),
        0x103 => "ArrowRight".to_string(),
        0x200 => "Escape".to_string(),
        0x201 => "Tab".to_string(),
        0x202 => "Backspace".to_string(),
        0x203 => "Enter".to_string(),
        0x204 => "Shift".to_string(),
        0x205 => "Ctrl".to_string(),
        0x206 => "Alt".to_string(),
        _ => "Unknown".to_string(),
    }
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
    fn test_parse_modifier_as_key() {
        let fallback = Binding::default();

        let ctrl = parse_binding("Ctrl", fallback);
        assert_eq!(ctrl.code, 0x205);
        assert_eq!(ctrl.mods, 0);

        let shift = parse_binding("Shift", fallback);
        assert_eq!(shift.code, 0x204);
        assert_eq!(shift.mods, 0);

        let alt = parse_binding("Alt", fallback);
        assert_eq!(alt.code, 0x206);
        assert_eq!(alt.mods, 0);
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
    fn test_parse_unbound() {
        let fallback = Binding::new('W' as u32, 0);
        let binding = parse_binding("Unbound", fallback);
        assert_eq!(binding.code, 0);
        assert_eq!(binding.mods, 0);
    }

    #[test]
    fn test_parse_empty_returns_fallback() {
        let fallback = Binding::new('X' as u32, 0);
        let binding = parse_binding("", fallback);
        assert_eq!(binding.code, 'X' as u32);
    }

    #[test]
    fn test_parse_case_insensitive() {
        let fallback = Binding::default();

        let lower = parse_binding("w", fallback);
        assert_eq!(lower.code, 'W' as u32);

        let ctrl = parse_binding("ctrl", fallback);
        assert_eq!(ctrl.code, 0x205);

        let ctrl_upper = parse_binding("CTRL", fallback);
        assert_eq!(ctrl_upper.code, 0x205);
    }

    #[test]
    fn test_binding_to_string_simple() {
        let binding = Binding::new('W' as u32, 0);
        assert_eq!(binding_to_string(&binding), "W");
    }

    #[test]
    fn test_binding_to_string_modifier_keys() {
        let ctrl = Binding::new(0x205, 0);
        assert_eq!(binding_to_string(&ctrl), "Ctrl");

        let shift = Binding::new(0x204, 0);
        assert_eq!(binding_to_string(&shift), "Shift");

        let alt = Binding::new(0x206, 0);
        assert_eq!(binding_to_string(&alt), "Alt");
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
    fn test_binding_to_string_unbound() {
        let binding = Binding::new(0, 0);
        assert_eq!(binding_to_string(&binding), "Unbound");
    }

    #[test]
    fn test_roundtrip_parsing() {
        let keys = [
            Binding::new('W' as u32, 0),
            Binding::new(0x205, 0),
            Binding::new(0x204, 0),
            Binding::new(0x206, 0),
            Binding::new(0x100, 0),
            Binding::new(' ' as u32, 0),
            Binding::new(0x200, 0),
        ];
        for original in &keys {
            let string = binding_to_string(original);
            let parsed = parse_binding(&string, Binding::default());
            assert_eq!(parsed, *original, "Roundtrip failed for {string}");
        }
    }
}
