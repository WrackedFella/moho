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
    fn every_mapped_egui_key_maps_to_its_key() {
        let table = [
            (egui::Key::A, Key::A),
            (egui::Key::B, Key::B),
            (egui::Key::C, Key::C),
            (egui::Key::D, Key::D),
            (egui::Key::E, Key::E),
            (egui::Key::F, Key::F),
            (egui::Key::G, Key::G),
            (egui::Key::H, Key::H),
            (egui::Key::I, Key::I),
            (egui::Key::J, Key::J),
            (egui::Key::K, Key::K),
            (egui::Key::L, Key::L),
            (egui::Key::M, Key::M),
            (egui::Key::N, Key::N),
            (egui::Key::O, Key::O),
            (egui::Key::P, Key::P),
            (egui::Key::Q, Key::Q),
            (egui::Key::R, Key::R),
            (egui::Key::S, Key::S),
            (egui::Key::T, Key::T),
            (egui::Key::U, Key::U),
            (egui::Key::V, Key::V),
            (egui::Key::W, Key::W),
            (egui::Key::X, Key::X),
            (egui::Key::Y, Key::Y),
            (egui::Key::Z, Key::Z),
            (egui::Key::Num0, Key::Digit0),
            (egui::Key::Num1, Key::Digit1),
            (egui::Key::Num2, Key::Digit2),
            (egui::Key::Num3, Key::Digit3),
            (egui::Key::Num4, Key::Digit4),
            (egui::Key::Num5, Key::Digit5),
            (egui::Key::Num6, Key::Digit6),
            (egui::Key::Num7, Key::Digit7),
            (egui::Key::Num8, Key::Digit8),
            (egui::Key::Num9, Key::Digit9),
            (egui::Key::ArrowUp, Key::ArrowUp),
            (egui::Key::ArrowDown, Key::ArrowDown),
            (egui::Key::ArrowLeft, Key::ArrowLeft),
            (egui::Key::ArrowRight, Key::ArrowRight),
            (egui::Key::Escape, Key::Escape),
            (egui::Key::Tab, Key::Tab),
            (egui::Key::Backspace, Key::Backspace),
            (egui::Key::Enter, Key::Enter),
            (egui::Key::Space, Key::Space),
        ];

        for (egui_key, key) in table {
            assert_eq!(egui_key_to_key(egui_key), Some(key), "{egui_key:?}");
        }
    }

    #[test]
    fn unnamed_egui_keys_do_not_map() {
        for k in [egui::Key::F5, egui::Key::Insert, egui::Key::Home] {
            assert_eq!(egui_key_to_key(k), None, "{k:?}");
        }
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
        assert_eq!(modifier_bits(&mods(true, true, false)), 3);
        assert_eq!(modifier_bits(&mods(true, false, true)), 5);
        assert_eq!(modifier_bits(&mods(false, true, true)), 6);
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
