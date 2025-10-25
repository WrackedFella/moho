//! Small shared input utilities used by both the binary and ui crates.
//!
//! This crate provides a single helper to map winit's PhysicalKey/KeyCode
//! to the internal binding numeric code used across the project.

use winit::keyboard::{KeyCode, PhysicalKey};

/// Map a winit `PhysicalKey` (or its `Code` variant) to our internal binding code.
///
/// Returns 0 for unmapped keys.
pub fn physical_key_to_binding_code(pk: PhysicalKey) -> u32 {
    if let PhysicalKey::Code(kc) = pk {
        match kc {
            KeyCode::KeyA => 'A' as u32,
            KeyCode::KeyB => 'B' as u32,
            KeyCode::KeyC => 'C' as u32,
            KeyCode::KeyD => 'D' as u32,
            KeyCode::KeyE => 'E' as u32,
            KeyCode::KeyF => 'F' as u32,
            KeyCode::KeyG => 'G' as u32,
            KeyCode::KeyH => 'H' as u32,
            KeyCode::KeyI => 'I' as u32,
            KeyCode::KeyJ => 'J' as u32,
            KeyCode::KeyK => 'K' as u32,
            KeyCode::KeyL => 'L' as u32,
            KeyCode::KeyM => 'M' as u32,
            KeyCode::KeyN => 'N' as u32,
            KeyCode::KeyO => 'O' as u32,
            KeyCode::KeyP => 'P' as u32,
            KeyCode::KeyQ => 'Q' as u32,
            KeyCode::KeyR => 'R' as u32,
            KeyCode::KeyS => 'S' as u32,
            KeyCode::KeyT => 'T' as u32,
            KeyCode::KeyU => 'U' as u32,
            KeyCode::KeyV => 'V' as u32,
            KeyCode::KeyW => 'W' as u32,
            KeyCode::KeyX => 'X' as u32,
            KeyCode::KeyY => 'Y' as u32,
            KeyCode::KeyZ => 'Z' as u32,
            KeyCode::Space => ' ' as u32,
            KeyCode::ArrowUp => 0x100,
            KeyCode::ArrowDown => 0x101,
            KeyCode::ArrowLeft => 0x102,
            KeyCode::ArrowRight => 0x103,
            KeyCode::Escape => 0x200,
            KeyCode::Tab => 0x201,
            KeyCode::Backspace => 0x202,
            KeyCode::Enter => 0x203,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => 0x204,
            KeyCode::ControlLeft | KeyCode::ControlRight => 0x205,
            KeyCode::AltLeft | KeyCode::AltRight => 0x206,
            _ => 0,
        }
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::physical_key_to_binding_code;
    use winit::keyboard::{KeyCode, PhysicalKey};

    #[test]
    fn maps_letters_and_specials() {
        assert_eq!(
            physical_key_to_binding_code(PhysicalKey::Code(KeyCode::KeyA)),
            'A' as u32
        );
        assert_eq!(
            physical_key_to_binding_code(PhysicalKey::Code(KeyCode::Space)),
            ' ' as u32
        );
        assert_eq!(
            physical_key_to_binding_code(PhysicalKey::Code(KeyCode::ArrowUp)),
            0x100
        );
        assert_eq!(
            physical_key_to_binding_code(PhysicalKey::Code(KeyCode::Escape)),
            0x200
        );
        assert_eq!(
            physical_key_to_binding_code(PhysicalKey::Code(KeyCode::ShiftLeft)),
            0x204
        );
        // Non-Code cases are not commonly exposed via winit's PhysicalKey; we
        // already exercise mapped codes above.
    }
}
