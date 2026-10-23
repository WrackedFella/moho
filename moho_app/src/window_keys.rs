//! Window-mode toggle keys: the runner consumes them so the game never sees them.

use std::collections::{BTreeMap, HashSet};

use moho_input::bindings::{Action, ActionBindings, Binding, Input};
use moho_input::key::Key;
use winit::keyboard::{KeyCode, PhysicalKey};

/// The reserved `[bindings]` entry that names the fullscreen key.
pub const FULLSCREEN_BINDING: &str = "fullscreen";

/// Splits the `fullscreen` entry out of a `[bindings]` section: the keys that toggle
/// fullscreen besides Alt+Enter, and the rest of the section for the game's own loader.
///
/// An absent entry gives F11. An entry that does not parse, names a protected key alone, or
/// holds anything but plain keys (a chord, hold or mouse button, which `ToggleKeys` cannot
/// match) logs a warning and also gives F11.
pub fn split_fullscreen_binding(
    section: &BTreeMap<String, String>,
) -> (Option<Vec<Input>>, BTreeMap<String, String>) {
    let mut rest = section.clone();
    let Some(value) = rest.remove(FULLSCREEN_BINDING) else {
        return (Some(vec![DEFAULT_FULLSCREEN_KEY]), rest);
    };
    let entry = BTreeMap::from([(FULLSCREEN_BINDING.to_owned(), value.clone())]);
    let (loaded, warnings) = ActionBindings::<FullscreenKey>::load(&entry);
    let bindings = loaded.get(FullscreenKey);
    let usable = warnings.is_empty()
        && !bindings.is_empty()
        && bindings
            .iter()
            .all(|b| b.is_plain() && matches!(b.input, Input::Key(_)));
    if usable {
        let keys = bindings.iter().map(|b| b.input).collect();
        return (Some(keys), rest);
    }
    tracing::warn!(value = %value, "unusable fullscreen binding; keeping F11");
    (Some(vec![DEFAULT_FULLSCREEN_KEY]), rest)
}

const DEFAULT_FULLSCREEN_KEY: Input = Input::Key(Key::F11);

/// The fullscreen key as an action, so it loads and validates like any other binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FullscreenKey;

impl Action for FullscreenKey {
    const ALL: &'static [Self] = &[FullscreenKey];

    fn name(self) -> &'static str {
        FULLSCREEN_BINDING
    }

    fn default_bindings(self) -> &'static [Binding] {
        const { &[Binding::key(Key::F11)] }
    }
}

/// What the runner does with one observed key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    Pass,
    Swallow,
    Flip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowMode {
    #[default]
    Windowed,
    Borderless,
}

impl WindowMode {
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
        if repeat && self.swallowed.contains(&code) {
            return Observed::Swallow;
        }
        // A fresh press means the earlier release never arrived.
        self.swallowed.remove(&code);
        if repeat || !self.is_toggle(code) {
            return Observed::Pass;
        }
        self.swallowed.insert(code);
        Observed::Flip
    }

    /// Forgets held modifiers and swallowed keys: their releases go to another window.
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

    #[test]
    fn a_new_press_after_a_lost_release_flips_again() {
        let mut keys = f11();

        keys.observe(KeyCode::F11, true, false);
        let second = keys.observe(KeyCode::F11, true, false);

        assert_eq!(second, Observed::Flip);
    }

    #[test]
    fn a_plain_enter_after_a_lost_alt_enter_release_passes() {
        let mut keys = f11();
        keys.observe(KeyCode::AltLeft, true, false);
        keys.observe(KeyCode::Enter, true, false);
        keys.observe(KeyCode::AltLeft, false, false);

        let plain = keys.observe(KeyCode::Enter, true, false);

        assert_eq!(plain, Observed::Pass);
    }

    #[test]
    fn enter_repeating_after_alt_goes_down_does_not_flip() {
        let mut keys = f11();
        keys.observe(KeyCode::Enter, true, false);
        keys.observe(KeyCode::AltLeft, true, false);

        let repeat = keys.observe(KeyCode::Enter, true, true);

        assert_eq!(repeat, Observed::Pass);
    }

    fn section(lines: &[(&str, &str)]) -> BTreeMap<String, String> {
        lines
            .iter()
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }

    /// Runs `run` and returns its result with what `tracing` wrote meanwhile.
    fn logged<T>(run: impl FnOnce() -> T) -> (T, String) {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("log sink").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || Sink(Arc::clone(&writer)))
            .with_ansi(false)
            .finish();
        let result = tracing::subscriber::with_default(subscriber, run);
        let bytes = buffer.lock().expect("log sink").clone();
        (
            result,
            String::from_utf8(bytes).expect("log output is utf-8"),
        )
    }

    #[test]
    fn a_rebound_fullscreen_key_toggles_and_f11_then_passes() {
        let raw = section(&[(FULLSCREEN_BINDING, "F8"), ("jump", "Space")]);

        let ((fullscreen, rest), log) = logged(|| split_fullscreen_binding(&raw));
        let mut keys = ToggleKeys::new(fullscreen.expect("a usable entry gives its keys"));
        let f11_press = keys.observe(KeyCode::F11, true, false);
        let f11_release = keys.observe(KeyCode::F11, false, false);
        let f8_press = keys.observe(KeyCode::F8, true, false);
        let f8_release = keys.observe(KeyCode::F8, false, false);

        assert_eq!(f11_press, Observed::Pass);
        assert_eq!(f11_release, Observed::Pass);
        assert_eq!(f8_press, Observed::Flip);
        assert_eq!(f8_release, Observed::Swallow);
        assert_eq!(rest, section(&[("jump", "Space")]));
        assert_eq!(log, "");
    }

    #[test]
    fn alt_enter_still_toggles_with_a_rebound_key() {
        let raw = section(&[(FULLSCREEN_BINDING, "F8")]);
        let (fullscreen, _) = split_fullscreen_binding(&raw);
        let mut keys = ToggleKeys::new(fullscreen.expect("a usable entry gives its keys"));

        keys.observe(KeyCode::AltLeft, true, false);
        let press = keys.observe(KeyCode::Enter, true, false);
        let release = keys.observe(KeyCode::Enter, false, false);

        assert_eq!(press, Observed::Flip);
        assert_eq!(release, Observed::Swallow);
    }

    #[test]
    fn an_absent_fullscreen_entry_keeps_f11_without_a_warning() {
        let raw = section(&[("jump", "Space")]);

        let ((fullscreen, rest), log) = logged(|| split_fullscreen_binding(&raw));

        assert_eq!(fullscreen, Some(vec![Input::Key(Key::F11)]));
        assert_eq!(rest, raw);
        assert_eq!(log, "");
    }

    #[test]
    fn an_unusable_or_protected_fullscreen_value_keeps_f11_with_a_warning() {
        let values = [
            "Nonsense",
            "Enter",
            "Escape",
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "Hold Enter",
            "Unbound",
            "Ctrl+F8",
            "Hold F8",
            "Mouse Left",
        ];

        for value in values {
            let raw = section(&[(FULLSCREEN_BINDING, value), ("jump", "Space")]);

            let ((fullscreen, rest), log) = logged(|| split_fullscreen_binding(&raw));
            let mut keys = ToggleKeys::new(fullscreen.expect("the default stands in"));
            let f11 = keys.observe(KeyCode::F11, true, false);

            assert_eq!(f11, Observed::Flip, "{value}");
            assert_eq!(rest, section(&[("jump", "Space")]), "{value}");
            assert!(log.contains("WARN"), "{value}: {log:?}");
            assert!(log.contains(value), "{value}: {log:?}");
        }
    }
}
