//! Platform-free key identity and its canonical text names.

use winit::keyboard::{KeyCode, PhysicalKey};

/// A physical key the engine can name, bind and persist.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Period,
    Comma,
    Slash,
    Backslash,
    Semicolon,
    Apostrophe,
    LeftBracket,
    RightBracket,
    Minus,
    Equals,
    Backtick,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Escape,
    Tab,
    Backspace,
    Enter,
    Space,
    Shift,
    Ctrl,
    Alt,
}

impl Key {
    /// Every variant, once.
    pub const ALL: &'static [Key] = &[
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
        Key::Digit0,
        Key::Digit1,
        Key::Digit2,
        Key::Digit3,
        Key::Digit4,
        Key::Digit5,
        Key::Digit6,
        Key::Digit7,
        Key::Digit8,
        Key::Digit9,
        Key::Period,
        Key::Comma,
        Key::Slash,
        Key::Backslash,
        Key::Semicolon,
        Key::Apostrophe,
        Key::LeftBracket,
        Key::RightBracket,
        Key::Minus,
        Key::Equals,
        Key::Backtick,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::Escape,
        Key::Tab,
        Key::Backspace,
        Key::Enter,
        Key::Space,
        Key::Shift,
        Key::Ctrl,
        Key::Alt,
    ];

    /// Canonical persisted name; `parse` accepts it back. Punctuation is spelled
    /// as a word because `,` separates list items and `;`/`#` start ini comments.
    pub fn name(self) -> &'static str {
        match self {
            Key::A => "A",
            Key::B => "B",
            Key::C => "C",
            Key::D => "D",
            Key::E => "E",
            Key::F => "F",
            Key::G => "G",
            Key::H => "H",
            Key::I => "I",
            Key::J => "J",
            Key::K => "K",
            Key::L => "L",
            Key::M => "M",
            Key::N => "N",
            Key::O => "O",
            Key::P => "P",
            Key::Q => "Q",
            Key::R => "R",
            Key::S => "S",
            Key::T => "T",
            Key::U => "U",
            Key::V => "V",
            Key::W => "W",
            Key::X => "X",
            Key::Y => "Y",
            Key::Z => "Z",
            Key::Digit0 => "0",
            Key::Digit1 => "1",
            Key::Digit2 => "2",
            Key::Digit3 => "3",
            Key::Digit4 => "4",
            Key::Digit5 => "5",
            Key::Digit6 => "6",
            Key::Digit7 => "7",
            Key::Digit8 => "8",
            Key::Digit9 => "9",
            Key::Period => "Period",
            Key::Comma => "Comma",
            Key::Slash => "Slash",
            Key::Backslash => "Backslash",
            Key::Semicolon => "Semicolon",
            Key::Apostrophe => "Apostrophe",
            Key::LeftBracket => "LeftBracket",
            Key::RightBracket => "RightBracket",
            Key::Minus => "Minus",
            Key::Equals => "Equals",
            Key::Backtick => "Backtick",
            Key::ArrowUp => "ArrowUp",
            Key::ArrowDown => "ArrowDown",
            Key::ArrowLeft => "ArrowLeft",
            Key::ArrowRight => "ArrowRight",
            Key::Escape => "Escape",
            Key::Tab => "Tab",
            Key::Backspace => "Backspace",
            Key::Enter => "Enter",
            Key::Space => "Spacebar",
            Key::Shift => "Shift",
            Key::Ctrl => "Ctrl",
            Key::Alt => "Alt",
        }
    }

    /// Player-facing label for the settings screen.
    pub fn label(self) -> &'static str {
        match self {
            Key::Period => ".",
            Key::Comma => ",",
            Key::Slash => "/",
            Key::Backslash => "\\",
            Key::Semicolon => ";",
            Key::Apostrophe => "'",
            Key::LeftBracket => "[",
            Key::RightBracket => "]",
            Key::Minus => "-",
            Key::Equals => "=",
            Key::Backtick => "`",
            other => other.name(),
        }
    }

    /// Case-insensitive; accepts the canonical name and the aliases.
    pub fn parse(s: &str) -> Option<Key> {
        let glyph_or_alias = match s {
            "." => Some(Key::Period),
            "," => Some(Key::Comma),
            "/" => Some(Key::Slash),
            "\\" => Some(Key::Backslash),
            ";" => Some(Key::Semicolon),
            "'" => Some(Key::Apostrophe),
            "[" => Some(Key::LeftBracket),
            "]" => Some(Key::RightBracket),
            "-" => Some(Key::Minus),
            "=" => Some(Key::Equals),
            "`" => Some(Key::Backtick),
            _ => match s.to_ascii_lowercase().as_str() {
                "up" => Some(Key::ArrowUp),
                "down" => Some(Key::ArrowDown),
                "left" => Some(Key::ArrowLeft),
                "right" => Some(Key::ArrowRight),
                "esc" => Some(Key::Escape),
                "return" => Some(Key::Enter),
                "space" => Some(Key::Space),
                "control" => Some(Key::Ctrl),
                _ => None,
            },
        };
        glyph_or_alias.or_else(|| {
            Key::ALL
                .iter()
                .copied()
                .find(|k| k.name().eq_ignore_ascii_case(s))
        })
    }

    /// `None` for keys the engine does not name.
    pub fn from_winit(key: PhysicalKey) -> Option<Key> {
        let PhysicalKey::Code(code) = key else {
            return None;
        };
        Some(match code {
            KeyCode::KeyA => Key::A,
            KeyCode::KeyB => Key::B,
            KeyCode::KeyC => Key::C,
            KeyCode::KeyD => Key::D,
            KeyCode::KeyE => Key::E,
            KeyCode::KeyF => Key::F,
            KeyCode::KeyG => Key::G,
            KeyCode::KeyH => Key::H,
            KeyCode::KeyI => Key::I,
            KeyCode::KeyJ => Key::J,
            KeyCode::KeyK => Key::K,
            KeyCode::KeyL => Key::L,
            KeyCode::KeyM => Key::M,
            KeyCode::KeyN => Key::N,
            KeyCode::KeyO => Key::O,
            KeyCode::KeyP => Key::P,
            KeyCode::KeyQ => Key::Q,
            KeyCode::KeyR => Key::R,
            KeyCode::KeyS => Key::S,
            KeyCode::KeyT => Key::T,
            KeyCode::KeyU => Key::U,
            KeyCode::KeyV => Key::V,
            KeyCode::KeyW => Key::W,
            KeyCode::KeyX => Key::X,
            KeyCode::KeyY => Key::Y,
            KeyCode::KeyZ => Key::Z,
            KeyCode::Digit0 => Key::Digit0,
            KeyCode::Digit1 => Key::Digit1,
            KeyCode::Digit2 => Key::Digit2,
            KeyCode::Digit3 => Key::Digit3,
            KeyCode::Digit4 => Key::Digit4,
            KeyCode::Digit5 => Key::Digit5,
            KeyCode::Digit6 => Key::Digit6,
            KeyCode::Digit7 => Key::Digit7,
            KeyCode::Digit8 => Key::Digit8,
            KeyCode::Digit9 => Key::Digit9,
            KeyCode::Period => Key::Period,
            KeyCode::Comma => Key::Comma,
            KeyCode::Slash => Key::Slash,
            KeyCode::Backslash => Key::Backslash,
            KeyCode::Semicolon => Key::Semicolon,
            KeyCode::Quote => Key::Apostrophe,
            KeyCode::BracketLeft => Key::LeftBracket,
            KeyCode::BracketRight => Key::RightBracket,
            KeyCode::Minus => Key::Minus,
            KeyCode::Equal => Key::Equals,
            KeyCode::Backquote => Key::Backtick,
            KeyCode::ArrowUp => Key::ArrowUp,
            KeyCode::ArrowDown => Key::ArrowDown,
            KeyCode::ArrowLeft => Key::ArrowLeft,
            KeyCode::ArrowRight => Key::ArrowRight,
            KeyCode::Escape => Key::Escape,
            KeyCode::Tab => Key::Tab,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Enter => Key::Enter,
            KeyCode::Space => Key::Space,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
            KeyCode::ControlLeft | KeyCode::ControlRight => Key::Ctrl,
            KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use winit::keyboard::KeyCode;

    /// Exhaustive on purpose: adding a variant fails to compile here until
    /// `Key::ALL` and this table are updated together.
    fn ordinal(key: Key) -> usize {
        match key {
            Key::A => 0,
            Key::B => 1,
            Key::C => 2,
            Key::D => 3,
            Key::E => 4,
            Key::F => 5,
            Key::G => 6,
            Key::H => 7,
            Key::I => 8,
            Key::J => 9,
            Key::K => 10,
            Key::L => 11,
            Key::M => 12,
            Key::N => 13,
            Key::O => 14,
            Key::P => 15,
            Key::Q => 16,
            Key::R => 17,
            Key::S => 18,
            Key::T => 19,
            Key::U => 20,
            Key::V => 21,
            Key::W => 22,
            Key::X => 23,
            Key::Y => 24,
            Key::Z => 25,
            Key::Digit0 => 26,
            Key::Digit1 => 27,
            Key::Digit2 => 28,
            Key::Digit3 => 29,
            Key::Digit4 => 30,
            Key::Digit5 => 31,
            Key::Digit6 => 32,
            Key::Digit7 => 33,
            Key::Digit8 => 34,
            Key::Digit9 => 35,
            Key::Period => 36,
            Key::Comma => 37,
            Key::Slash => 38,
            Key::Backslash => 39,
            Key::Semicolon => 40,
            Key::Apostrophe => 41,
            Key::LeftBracket => 42,
            Key::RightBracket => 43,
            Key::Minus => 44,
            Key::Equals => 45,
            Key::Backtick => 46,
            Key::ArrowUp => 47,
            Key::ArrowDown => 48,
            Key::ArrowLeft => 49,
            Key::ArrowRight => 50,
            Key::Escape => 51,
            Key::Tab => 52,
            Key::Backspace => 53,
            Key::Enter => 54,
            Key::Space => 55,
            Key::Shift => 56,
            Key::Ctrl => 57,
            Key::Alt => 58,
        }
    }

    #[test]
    fn all_lists_every_variant_exactly_once() {
        let ordinals: HashSet<usize> = Key::ALL.iter().map(|&k| ordinal(k)).collect();

        assert_eq!(Key::ALL.len(), 59);
        assert_eq!(ordinals, (0..59).collect::<HashSet<_>>());
    }

    #[test]
    fn every_key_round_trips_through_its_name() {
        for &key in Key::ALL {
            let name = key.name();

            assert_eq!(Key::parse(name), Some(key), "name {name:?}");
        }
    }

    #[test]
    fn aliases_parse_to_their_key() {
        let cases = [
            ("Up", Key::ArrowUp),
            ("up", Key::ArrowUp),
            ("ArrowUp", Key::ArrowUp),
            ("DOWN", Key::ArrowDown),
            ("Left", Key::ArrowLeft),
            ("right", Key::ArrowRight),
            ("Esc", Key::Escape),
            ("ESC", Key::Escape),
            ("escape", Key::Escape),
            ("Return", Key::Enter),
            ("RETURN", Key::Enter),
            ("Space", Key::Space),
            ("space", Key::Space),
            ("Spacebar", Key::Space),
            ("SPACEBAR", Key::Space),
            ("Control", Key::Ctrl),
            ("control", Key::Ctrl),
            ("Ctrl", Key::Ctrl),
            (",", Key::Comma),
            (".", Key::Period),
            ("[", Key::LeftBracket),
            ("comma", Key::Comma),
            ("LeftBracket", Key::LeftBracket),
            ("w", Key::W),
            ("W", Key::W),
        ];

        for (input, expected) in cases {
            assert_eq!(Key::parse(input), Some(expected), "input {input:?}");
        }
    }

    #[test]
    fn canonical_names_are_pinned() {
        let cases = [
            (Key::A, "A"),
            (Key::Z, "Z"),
            (Key::Digit0, "0"),
            (Key::Digit9, "9"),
            (Key::Space, "Spacebar"),
            (Key::ArrowUp, "ArrowUp"),
            (Key::ArrowLeft, "ArrowLeft"),
            (Key::Escape, "Escape"),
            (Key::Enter, "Enter"),
            (Key::Shift, "Shift"),
            (Key::Ctrl, "Ctrl"),
            (Key::Alt, "Alt"),
            (Key::Comma, "Comma"),
            (Key::Semicolon, "Semicolon"),
            (Key::Equals, "Equals"),
            (Key::LeftBracket, "LeftBracket"),
        ];

        for (key, name) in cases {
            assert_eq!(key.name(), name);
        }
    }

    #[test]
    fn from_winit_maps_physical_keys() {
        let code = |kc| Key::from_winit(PhysicalKey::Code(kc));

        assert_eq!(code(KeyCode::KeyA), Some(Key::A));
        assert_eq!(code(KeyCode::Space), Some(Key::Space));
        assert_eq!(code(KeyCode::ArrowUp), Some(Key::ArrowUp));
        assert_eq!(code(KeyCode::Escape), Some(Key::Escape));
        assert_eq!(code(KeyCode::ShiftLeft), Some(Key::Shift));
        assert_eq!(code(KeyCode::ShiftRight), Some(Key::Shift));
        assert_eq!(code(KeyCode::F5), None);
    }

    #[test]
    fn unknown_names_do_not_parse() {
        for input in ["Banana", "", "  ", "Unbound", "256"] {
            assert_eq!(Key::parse(input), None, "input {input:?}");
        }
    }
}
