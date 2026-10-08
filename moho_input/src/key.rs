//! Platform-free key identity and its canonical text names.

use winit::keyboard::PhysicalKey;

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

    /// Canonical persisted name; `parse` accepts it back.
    pub fn name(self) -> &'static str {
        ""
    }

    /// Player-facing label for the settings screen.
    pub fn label(self) -> &'static str {
        ""
    }

    /// Case-insensitive; accepts the canonical name and the aliases.
    pub fn parse(_s: &str) -> Option<Key> {
        None
    }

    /// `None` for keys the engine does not name.
    pub fn from_winit(_key: PhysicalKey) -> Option<Key> {
        None
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
