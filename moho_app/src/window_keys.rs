//! Window-mode toggle keys: the runner consumes them so the game never sees them.

use std::collections::HashSet;

use moho_input::bindings::Input;
use moho_input::key::Key;
use winit::keyboard::{KeyCode, PhysicalKey};

/// What the runner does with one observed key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    /// Hand the event to the game.
    Pass,
    /// Drop the event.
    Swallow,
    /// Drop the event and flip the window mode.
    Flip,
}

/// How the window fills the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowMode {
    #[default]
    Windowed,
    Borderless,
}

impl WindowMode {
    /// The other mode.
    pub fn toggled(self) -> Self {
        match self {
            WindowMode::Windowed => WindowMode::Borderless,
            WindowMode::Borderless => WindowMode::Windowed,
        }
    }
}

/// Recognises the configured toggle keys and Alt+Enter.
#[derive(Debug, Clone)]
pub struct ToggleKeys {
    keys: Vec<Input>,
    alt_left: bool,
    alt_right: bool,
    /// Physical keys whose press was consumed, so their repeats and release are too.
    swallowed: HashSet<KeyCode>,
}

impl ToggleKeys {
    pub fn new(keys: Vec<Input>) -> Self {
        Self {
            keys,
            alt_left: false,
            alt_right: false,
            swallowed: HashSet::new(),
        }
    }

    /// Classifies one key event.
    pub fn observe(&mut self, code: KeyCode, pressed: bool, repeat: bool) -> Observed {
        match code {
            KeyCode::AltLeft => self.alt_left = pressed,
            KeyCode::AltRight => self.alt_right = pressed,
            _ => {}
        }
        if matches!(code, KeyCode::AltLeft | KeyCode::AltRight) {
            return Observed::Pass;
        }

        if !pressed {
            return if self.swallowed.remove(&code) {
                Observed::Swallow
            } else {
                Observed::Pass
            };
        }
        if self.swallowed.contains(&code) {
            return Observed::Swallow;
        }
        if repeat || !self.is_toggle(code) {
            return Observed::Pass;
        }
        self.swallowed.insert(code);
        Observed::Flip
    }

    /// Forgets held modifiers: their releases go to another window.
    pub fn focus_lost(&mut self) {
        self.alt_left = false;
        self.alt_right = false;
        self.swallowed.clear();
    }

    fn is_toggle(&self, code: KeyCode) -> bool {
        let alt_enter = (self.alt_left || self.alt_right)
            && matches!(code, KeyCode::Enter | KeyCode::NumpadEnter);
        alt_enter
            || self.keys.iter().any(|input| match input {
                Input::Key(key) => Key::from_winit(PhysicalKey::Code(code)) == Some(*key),
                _ => false,
            })
    }
}

#[cfg(test)]
mod tests {
    use moho_input::key::Key;

    use super::*;

    fn f11() -> ToggleKeys {
        ToggleKeys::new(vec![Input::Key(Key::F11)])
    }

    #[test]
    fn f11_flips_once_and_its_release_is_swallowed() {
        let mut keys = f11();

        let press = keys.observe(KeyCode::F11, true, false);
        let release = keys.observe(KeyCode::F11, false, false);

        assert_eq!(press, Observed::Flip);
        assert_eq!(release, Observed::Swallow);
    }

    #[test]
    fn alt_enter_flips_and_neither_enter_press_nor_release_passes() {
        let mut keys = f11();

        let alt = keys.observe(KeyCode::AltLeft, true, false);
        let press = keys.observe(KeyCode::Enter, true, false);
        let release = keys.observe(KeyCode::Enter, false, false);
        let alt_release = keys.observe(KeyCode::AltLeft, false, false);

        assert_eq!(alt, Observed::Pass);
        assert_eq!(press, Observed::Flip);
        assert_eq!(release, Observed::Swallow);
        assert_eq!(alt_release, Observed::Pass);
    }

    #[test]
    fn right_alt_enter_flips() {
        let mut keys = f11();

        keys.observe(KeyCode::AltRight, true, false);
        let press = keys.observe(KeyCode::Enter, true, false);

        assert_eq!(press, Observed::Flip);
    }

    #[test]
    fn numpad_enter_with_alt_flips() {
        let mut keys = f11();

        keys.observe(KeyCode::AltLeft, true, false);
        let press = keys.observe(KeyCode::NumpadEnter, true, false);
        let release = keys.observe(KeyCode::NumpadEnter, false, false);

        assert_eq!(press, Observed::Flip);
        assert_eq!(release, Observed::Swallow);
    }

    #[test]
    fn enter_without_alt_passes() {
        let mut keys = f11();

        let press = keys.observe(KeyCode::Enter, true, false);
        let release = keys.observe(KeyCode::Enter, false, false);

        assert_eq!(press, Observed::Pass);
        assert_eq!(release, Observed::Pass);
    }

    #[test]
    fn alt_released_before_enter_does_not_flip() {
        let mut keys = f11();

        keys.observe(KeyCode::AltLeft, true, false);
        keys.observe(KeyCode::AltLeft, false, false);
        let press = keys.observe(KeyCode::Enter, true, false);

        assert_eq!(press, Observed::Pass);
    }

    #[test]
    fn a_repeat_press_does_not_flip_again() {
        let mut keys = f11();

        let first = keys.observe(KeyCode::F11, true, false);
        let repeat = keys.observe(KeyCode::F11, true, true);
        let release = keys.observe(KeyCode::F11, false, false);

        assert_eq!(first, Observed::Flip);
        assert_eq!(repeat, Observed::Swallow);
        assert_eq!(release, Observed::Swallow);
    }

    #[test]
    fn a_repeat_of_alt_enter_does_not_flip_again() {
        let mut keys = f11();

        keys.observe(KeyCode::AltLeft, true, false);
        let first = keys.observe(KeyCode::Enter, true, false);
        let repeat = keys.observe(KeyCode::Enter, true, true);

        assert_eq!(first, Observed::Flip);
        assert_eq!(repeat, Observed::Swallow);
    }

    #[test]
    fn losing_focus_clears_alt() {
        let mut keys = f11();
        keys.observe(KeyCode::AltLeft, true, false);

        keys.focus_lost();
        let press = keys.observe(KeyCode::Enter, true, false);

        assert_eq!(press, Observed::Pass);
    }

    #[test]
    fn a_custom_toggle_key_flips_and_f11_then_passes() {
        let mut keys = ToggleKeys::new(vec![Input::Key(Key::F10)]);

        let custom = keys.observe(KeyCode::F10, true, false);
        let custom_release = keys.observe(KeyCode::F10, false, false);
        let f11_press = keys.observe(KeyCode::F11, true, false);
        let f11_release = keys.observe(KeyCode::F11, false, false);

        assert_eq!(custom, Observed::Flip);
        assert_eq!(custom_release, Observed::Swallow);
        assert_eq!(f11_press, Observed::Pass);
        assert_eq!(f11_release, Observed::Pass);
    }

    #[test]
    fn toggling_flips_windowed_and_borderless_both_ways() {
        assert_eq!(WindowMode::Windowed.toggled(), WindowMode::Borderless);
        assert_eq!(WindowMode::Borderless.toggled(), WindowMode::Windowed);
    }
}
