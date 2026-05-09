//! Key name to key code mappings for binding parsing.
//!
//! This module provides compile-time perfect hash maps for parsing key names
//! from INI files into key codes. Uses PHF for O(1) lookup performance.

use phf::phf_map;

/// Map key names (case-insensitive) to their corresponding key codes.
///
/// This map supports:
/// - Special keys (Arrow keys, Escape, Tab, etc.)
/// - Single ASCII characters (A-Z, 0-9, punctuation)
/// - Modifier keys (Ctrl, Shift, Alt)
/// - Common aliases (e.g., "UP" for "ARROWUP")
///
/// All keys are stored in UPPERCASE for case-insensitive matching.
pub static KEY_NAME_TO_CODE: phf::Map<&'static str, u32> = phf_map! {
    // Arrow keys
    "ARROWUP" => 0x100,
    "UP" => 0x100,
    "ARROWDOWN" => 0x101,
    "DOWN" => 0x101,
    "ARROWLEFT" => 0x102,
    "LEFT" => 0x102,
    "ARROWRIGHT" => 0x103,
    "RIGHT" => 0x103,

    // Special keys
    "ESCAPE" => 0x200,
    "ESC" => 0x200,
    "TAB" => 0x201,
    "BACKSPACE" => 0x202,
    "ENTER" => 0x203,
    "RETURN" => 0x203,
    "SPACE" => ' ' as u32,
    "SPACEBAR" => ' ' as u32,

    // Modifier keys (as key codes, not as modifiers)
    "SHIFT" => 0x204,
    "CTRL" => 0x205,
    "CONTROL" => 0x205,
    "ALT" => 0x206,

    // Letters (A-Z)
    "A" => 'A' as u32,
    "B" => 'B' as u32,
    "C" => 'C' as u32,
    "D" => 'D' as u32,
    "E" => 'E' as u32,
    "F" => 'F' as u32,
    "G" => 'G' as u32,
    "H" => 'H' as u32,
    "I" => 'I' as u32,
    "J" => 'J' as u32,
    "K" => 'K' as u32,
    "L" => 'L' as u32,
    "M" => 'M' as u32,
    "N" => 'N' as u32,
    "O" => 'O' as u32,
    "P" => 'P' as u32,
    "Q" => 'Q' as u32,
    "R" => 'R' as u32,
    "S" => 'S' as u32,
    "T" => 'T' as u32,
    "U" => 'U' as u32,
    "V" => 'V' as u32,
    "W" => 'W' as u32,
    "X" => 'X' as u32,
    "Y" => 'Y' as u32,
    "Z" => 'Z' as u32,

    // Numbers (0-9)
    "0" => '0' as u32,
    "1" => '1' as u32,
    "2" => '2' as u32,
    "3" => '3' as u32,
    "4" => '4' as u32,
    "5" => '5' as u32,
    "6" => '6' as u32,
    "7" => '7' as u32,
    "8" => '8' as u32,
    "9" => '9' as u32,

    // Common punctuation
    "." => '.' as u32,
    "," => ',' as u32,
    "/" => '/' as u32,
    "\\" => '\\' as u32,
    ";" => ';' as u32,
    "'" => '\'' as u32,
    "[" => '[' as u32,
    "]" => ']' as u32,
    "-" => '-' as u32,
    "=" => '=' as u32,
    "`" => '`' as u32,
};

/// Parse a key name string into a key code using PHF lookup.
///
/// This function provides O(1) lookup time for known key names.
/// Falls back to single-character parsing or numeric code parsing
/// for keys not in the map.
///
/// # Arguments
/// * `key_part` - The key name (e.g., "ArrowUp", "W", "Escape")
/// * `fallback` - The key code to return if parsing fails
///
/// # Returns
/// The key code as u32, or fallback on error
///
/// # Examples
/// ```ignore
/// assert_eq!(parse_key_name("ArrowUp", 0), 0x100);
/// assert_eq!(parse_key_name("W", 0), 'W' as u32);
/// assert_eq!(parse_key_name("unknown", 42), 42);
/// ```
pub fn parse_key_name(key_part: &str, fallback: u32) -> u32 {
    // Try PHF lookup first (O(1))
    let uppercase = key_part.to_ascii_uppercase();
    if let Some(&code) = KEY_NAME_TO_CODE.get(uppercase.as_str()) {
        return code;
    }

    // Fallback: Try single character
    if key_part.len() == 1
        && let Some(ch) = key_part.chars().next()
    {
        return ch.to_ascii_uppercase() as u32;
    }

    // Fallback: Try parse as numeric code
    key_part.parse::<u32>().unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_arrow_keys() {
        assert_eq!(parse_key_name("ArrowUp", 0), 0x100);
        assert_eq!(parse_key_name("UP", 0), 0x100);
        assert_eq!(parse_key_name("ArrowDown", 0), 0x101);
        assert_eq!(parse_key_name("DOWN", 0), 0x101);
        assert_eq!(parse_key_name("ArrowLeft", 0), 0x102);
        assert_eq!(parse_key_name("LEFT", 0), 0x102);
        assert_eq!(parse_key_name("ArrowRight", 0), 0x103);
        assert_eq!(parse_key_name("RIGHT", 0), 0x103);
    }

    #[test]
    fn test_parse_special_keys() {
        assert_eq!(parse_key_name("Escape", 0), 0x200);
        assert_eq!(parse_key_name("ESC", 0), 0x200);
        assert_eq!(parse_key_name("Tab", 0), 0x201);
        assert_eq!(parse_key_name("Backspace", 0), 0x202);
        assert_eq!(parse_key_name("Enter", 0), 0x203);
        assert_eq!(parse_key_name("RETURN", 0), 0x203);
        assert_eq!(parse_key_name("Spacebar", 0), ' ' as u32);
        assert_eq!(parse_key_name("SPACE", 0), ' ' as u32);
    }

    #[test]
    fn test_parse_letters() {
        assert_eq!(parse_key_name("A", 0), 'A' as u32);
        assert_eq!(parse_key_name("W", 0), 'W' as u32);
        assert_eq!(parse_key_name("Z", 0), 'Z' as u32);
        // Case insensitive
        assert_eq!(parse_key_name("w", 0), 'W' as u32);
        assert_eq!(parse_key_name("a", 0), 'A' as u32);
    }

    #[test]
    fn test_parse_numbers() {
        assert_eq!(parse_key_name("0", 0), '0' as u32);
        assert_eq!(parse_key_name("5", 0), '5' as u32);
        assert_eq!(parse_key_name("9", 0), '9' as u32);
    }

    #[test]
    fn test_parse_punctuation() {
        assert_eq!(parse_key_name(".", 0), '.' as u32);
        assert_eq!(parse_key_name(",", 0), ',' as u32);
        assert_eq!(parse_key_name("/", 0), '/' as u32);
    }

    #[test]
    fn test_parse_modifier_keys() {
        assert_eq!(parse_key_name("Shift", 0), 0x204);
        assert_eq!(parse_key_name("Ctrl", 0), 0x205);
        assert_eq!(parse_key_name("CONTROL", 0), 0x205);
        assert_eq!(parse_key_name("Alt", 0), 0x206);
    }

    #[test]
    fn test_parse_unknown_returns_fallback() {
        assert_eq!(parse_key_name("unknown_key", 42), 42);
        assert_eq!(parse_key_name("", 99), 99);
    }

    #[test]
    fn test_parse_numeric_string() {
        assert_eq!(parse_key_name("256", 0), 256);
        assert_eq!(parse_key_name("0x100", 0), 0); // Hex not supported, uses fallback
    }

    #[test]
    fn test_case_insensitivity() {
        assert_eq!(parse_key_name("arrowup", 0), 0x100);
        assert_eq!(parse_key_name("ARROWUP", 0), 0x100);
        assert_eq!(parse_key_name("ArrowUp", 0), 0x100);
        assert_eq!(parse_key_name("escape", 0), 0x200);
        assert_eq!(parse_key_name("ESCAPE", 0), 0x200);
    }
}
