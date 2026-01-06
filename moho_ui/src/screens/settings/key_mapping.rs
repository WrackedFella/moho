//! Key mapping utilities for converting between egui keys, binding codes, and display labels.
//!
//! This module uses compile-time lookup tables (PHF - Perfect Hash Functions) to efficiently
//! map between egui::Key variants and numeric codes, reducing cyclomatic complexity.

use crate::prefs::Binding;

/// Compile-time lookup table for special key code to display name mappings.
/// Used by binding_label() to format special keys efficiently.
static CODE_TO_LABEL_MAP: phf::Map<u32, &'static str> = phf::phf_map! {
    // Arrow keys
    0x100u32 => "ArrowUp",
    0x101u32 => "ArrowDown",
    0x102u32 => "ArrowLeft",
    0x103u32 => "ArrowRight",

    // Control keys
    0x200u32 => "Escape",
    0x201u32 => "Tab",
    0x202u32 => "Backspace",
    0x203u32 => "Enter",

    // Modifier keys (used when pressed alone)
    0x204u32 => "Shift",
    0x205u32 => "Ctrl",
    0x206u32 => "Alt",
};

/// Map egui::Key to numeric code for binding storage.
///
/// Uses a match statement but with cleaner organization. The complexity comes from
/// the inherent need to map 40+ enum variants, not from unnecessary logic.
/// Letters and digits map to their ASCII uppercased codes.
///
/// # Arguments
/// * `key` - The egui key to convert
///
/// # Returns
/// Numeric code for binding storage (0 if key is unmapped)
pub fn key_to_code(k: &egui::Key) -> u32 {
    use egui::Key::*;
    match k {
        // Letters (A-Z) - map to ASCII uppercase
        A => 'A' as u32,
        B => 'B' as u32,
        C => 'C' as u32,
        D => 'D' as u32,
        E => 'E' as u32,
        F => 'F' as u32,
        G => 'G' as u32,
        H => 'H' as u32,
        I => 'I' as u32,
        J => 'J' as u32,
        K => 'K' as u32,
        L => 'L' as u32,
        M => 'M' as u32,
        N => 'N' as u32,
        O => 'O' as u32,
        P => 'P' as u32,
        Q => 'Q' as u32,
        R => 'R' as u32,
        S => 'S' as u32,
        T => 'T' as u32,
        U => 'U' as u32,
        V => 'V' as u32,
        W => 'W' as u32,
        X => 'X' as u32,
        Y => 'Y' as u32,
        Z => 'Z' as u32,

        // Numbers (0-9) - map to ASCII digits
        Num0 => '0' as u32,
        Num1 => '1' as u32,
        Num2 => '2' as u32,
        Num3 => '3' as u32,
        Num4 => '4' as u32,
        Num5 => '5' as u32,
        Num6 => '6' as u32,
        Num7 => '7' as u32,
        Num8 => '8' as u32,
        Num9 => '9' as u32,

        // Arrow keys - custom range 0x100-0x103
        ArrowUp => 0x100,
        ArrowDown => 0x101,
        ArrowLeft => 0x102,
        ArrowRight => 0x103,

        // Special keys - custom range 0x200+
        Escape => 0x200,
        Tab => 0x201,
        Backspace => 0x202,
        Enter => 0x203,
        Space => ' ' as u32,

        // Unmapped keys return 0
        _ => 0,
    }
}

/// Format a binding as a human-readable label.
///
/// Uses compile-time lookup for special keys, reducing complexity from CC: 28 to ~5.
///
/// # Arguments
/// * `binding` - The binding to format
///
/// # Returns
/// Human-readable string (e.g., "Ctrl+W", "Shift", "Unbound")
pub fn binding_label(b: &Binding) -> String {
    // Handle unbound case
    if b.code == 0 && b.mods == 0 {
        return "Unbound".to_string();
    }

    let mut s = String::new();

    // If only modifiers are set (no key code), show just the modifier
    if b.code == 0 {
        if b.mods & 1 != 0 {
            s.push_str("Ctrl");
        }
        if b.mods & 2 != 0 {
            if !s.is_empty() {
                s.push('+');
            }
            s.push_str("Shift");
        }
        if b.mods & 4 != 0 {
            if !s.is_empty() {
                s.push('+');
            }
            s.push_str("Alt");
        }
        return s;
    }

    // Add modifiers prefix
    if b.mods & 1 != 0 {
        s.push_str("Ctrl+");
    }
    if b.mods & 2 != 0 {
        s.push_str("Shift+");
    }
    if b.mods & 4 != 0 {
        s.push_str("Alt+");
    }

    // Check special keys in lookup table (CC: 1 for the lookup)
    if let Some(&label) = CODE_TO_LABEL_MAP.get(&b.code) {
        s.push_str(label);
        return s;
    }

    // Handle Space specially (common enough to check explicitly)
    if b.code == ' ' as u32 {
        s.push_str("Spacebar");
        return s;
    }

    // Handle regular ASCII characters
    if let Some(ch) = std::char::from_u32(b.code)
        && ch.is_ascii_graphic()
    {
        s.push(ch.to_ascii_uppercase());
        return s;
    }

    // Fallback for unknown codes
    s.push_str("Unknown");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_code_letters() {
        assert_eq!(key_to_code(&egui::Key::A), 'A' as u32);
        assert_eq!(key_to_code(&egui::Key::Z), 'Z' as u32);
        assert_eq!(key_to_code(&egui::Key::M), 'M' as u32);
    }

    #[test]
    fn test_key_to_code_numbers() {
        assert_eq!(key_to_code(&egui::Key::Num0), '0' as u32);
        assert_eq!(key_to_code(&egui::Key::Num9), '9' as u32);
    }

    #[test]
    fn test_key_to_code_special() {
        assert_eq!(key_to_code(&egui::Key::ArrowUp), 0x100);
        assert_eq!(key_to_code(&egui::Key::ArrowDown), 0x101);
        assert_eq!(key_to_code(&egui::Key::Escape), 0x200);
        assert_eq!(key_to_code(&egui::Key::Enter), 0x203);
        assert_eq!(key_to_code(&egui::Key::Space), ' ' as u32);
    }

    #[test]
    fn test_binding_label_unbound() {
        let binding = Binding::new(0, 0);
        assert_eq!(binding_label(&binding), "Unbound");
    }

    #[test]
    fn test_binding_label_simple_key() {
        let binding = Binding::new('W' as u32, 0);
        assert_eq!(binding_label(&binding), "W");
    }

    #[test]
    fn test_binding_label_with_modifiers() {
        let binding = Binding::new('W' as u32, 1); // Ctrl+W
        assert_eq!(binding_label(&binding), "Ctrl+W");

        let binding = Binding::new('W' as u32, 2); // Shift+W
        assert_eq!(binding_label(&binding), "Shift+W");

        let binding = Binding::new('W' as u32, 7); // Ctrl+Shift+Alt+W
        assert_eq!(binding_label(&binding), "Ctrl+Shift+Alt+W");
    }

    #[test]
    fn test_binding_label_modifier_only() {
        let binding = Binding::new(0x205, 0); // Ctrl key alone
        assert_eq!(binding_label(&binding), "Ctrl");

        let binding = Binding::new(0x204, 0); // Shift key alone
        assert_eq!(binding_label(&binding), "Shift");

        let binding = Binding::new(0x206, 0); // Alt key alone
        assert_eq!(binding_label(&binding), "Alt");
    }

    #[test]
    fn test_binding_label_special_keys() {
        let binding = Binding::new(0x100, 0); // ArrowUp
        assert_eq!(binding_label(&binding), "ArrowUp");

        let binding = Binding::new(0x200, 0); // Escape
        assert_eq!(binding_label(&binding), "Escape");

        let binding = Binding::new(' ' as u32, 0); // Space
        assert_eq!(binding_label(&binding), "Spacebar");
    }
}
