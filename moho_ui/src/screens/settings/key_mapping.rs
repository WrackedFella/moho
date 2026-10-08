//! Conversions between egui input and engine keys, and binding display labels.

use moho_input::bindings::Binding;
use moho_input::key::Key;

/// `None` for egui keys the engine does not name.
pub fn egui_key_to_key(k: egui::Key) -> Option<Key> {
    use egui::Key as E;
    Some(match k {
        E::A => Key::A,
        E::B => Key::B,
        E::C => Key::C,
        E::D => Key::D,
        E::E => Key::E,
        E::F => Key::F,
        E::G => Key::G,
        E::H => Key::H,
        E::I => Key::I,
        E::J => Key::J,
        E::K => Key::K,
        E::L => Key::L,
        E::M => Key::M,
        E::N => Key::N,
        E::O => Key::O,
        E::P => Key::P,
        E::Q => Key::Q,
        E::R => Key::R,
        E::S => Key::S,
        E::T => Key::T,
        E::U => Key::U,
        E::V => Key::V,
        E::W => Key::W,
        E::X => Key::X,
        E::Y => Key::Y,
        E::Z => Key::Z,
        E::Num0 => Key::Digit0,
        E::Num1 => Key::Digit1,
        E::Num2 => Key::Digit2,
        E::Num3 => Key::Digit3,
        E::Num4 => Key::Digit4,
        E::Num5 => Key::Digit5,
        E::Num6 => Key::Digit6,
        E::Num7 => Key::Digit7,
        E::Num8 => Key::Digit8,
        E::Num9 => Key::Digit9,
        E::ArrowUp => Key::ArrowUp,
        E::ArrowDown => Key::ArrowDown,
        E::ArrowLeft => Key::ArrowLeft,
        E::ArrowRight => Key::ArrowRight,
        E::Escape => Key::Escape,
        E::Tab => Key::Tab,
        E::Backspace => Key::Backspace,
        E::Enter => Key::Enter,
        E::Space => Key::Space,
        _ => return None,
    })
}

/// Modifier state as a bitfield: Ctrl = 1, Shift = 2, Alt = 4.
pub fn modifier_bits(modifiers: &egui::Modifiers) -> u8 {
    u8::from(modifiers.ctrl) | (u8::from(modifiers.shift) << 1) | (u8::from(modifiers.alt) << 2)
}

/// The modifier key held alone, for `modifier_bits` of exactly one modifier.
pub fn lone_modifier_key(bits: u8) -> Option<Key> {
    match bits {
        1 => Some(Key::Ctrl),
        2 => Some(Key::Shift),
        4 => Some(Key::Alt),
        _ => None,
    }
}

/// Human-readable label for an action's bindings, e.g. `W`, `Shift, Ctrl`, `Unbound`.
pub fn binding_label(bindings: &[Binding]) -> String {
    if bindings.is_empty() {
        return "Unbound".to_string();
    }
    bindings
        .iter()
        .map(|Binding::Key(key)| key.label())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egui_letters_and_digits_map_to_their_keys() {
        assert_eq!(egui_key_to_key(egui::Key::A), Some(Key::A));
        assert_eq!(egui_key_to_key(egui::Key::Z), Some(Key::Z));
        assert_eq!(egui_key_to_key(egui::Key::Num0), Some(Key::Digit0));
        assert_eq!(egui_key_to_key(egui::Key::Num9), Some(Key::Digit9));
    }

    #[test]
    fn egui_special_keys_map_to_their_keys() {
        assert_eq!(egui_key_to_key(egui::Key::ArrowUp), Some(Key::ArrowUp));
        assert_eq!(egui_key_to_key(egui::Key::ArrowDown), Some(Key::ArrowDown));
        assert_eq!(egui_key_to_key(egui::Key::Escape), Some(Key::Escape));
        assert_eq!(egui_key_to_key(egui::Key::Enter), Some(Key::Enter));
        assert_eq!(egui_key_to_key(egui::Key::Space), Some(Key::Space));
    }

    #[test]
    fn unnamed_egui_keys_do_not_map() {
        assert_eq!(egui_key_to_key(egui::Key::F5), None);
    }

    #[test]
    fn modifier_bits_encode_ctrl_shift_alt() {
        let mods = |ctrl, shift, alt| egui::Modifiers {
            ctrl,
            shift,
            alt,
            ..Default::default()
        };

        assert_eq!(modifier_bits(&mods(false, false, false)), 0);
        assert_eq!(modifier_bits(&mods(true, false, false)), 1);
        assert_eq!(modifier_bits(&mods(false, true, false)), 2);
        assert_eq!(modifier_bits(&mods(false, false, true)), 4);
        assert_eq!(modifier_bits(&mods(true, true, true)), 7);
    }

    #[test]
    fn only_a_single_modifier_is_a_lone_modifier_key() {
        assert_eq!(lone_modifier_key(1), Some(Key::Ctrl));
        assert_eq!(lone_modifier_key(2), Some(Key::Shift));
        assert_eq!(lone_modifier_key(4), Some(Key::Alt));
        assert_eq!(lone_modifier_key(0), None);
        assert_eq!(lone_modifier_key(3), None);
    }

    #[test]
    fn binding_label_unbound() {
        assert_eq!(binding_label(&[]), "Unbound");
    }

    #[test]
    fn binding_label_names_each_key() {
        assert_eq!(binding_label(&[Binding::Key(Key::W)]), "W");
        assert_eq!(binding_label(&[Binding::Key(Key::Shift)]), "Shift");
        assert_eq!(binding_label(&[Binding::Key(Key::ArrowUp)]), "ArrowUp");
        assert_eq!(binding_label(&[Binding::Key(Key::Space)]), "Spacebar");
        assert_eq!(
            binding_label(&[Binding::Key(Key::F), Binding::Key(Key::Space)]),
            "F, Spacebar"
        );
    }
}
