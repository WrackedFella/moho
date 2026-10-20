//! The strategy game's actions: stable ids and default bindings.

use moho_core::prefs::Prefs;
use moho_input::Action;
use moho_input::bindings::{ActionBindings, Binding};
use moho_input::key::{Key, MouseButton};
use moho_input::pad::{PadButton, PadInput, Stick, StickDir};
use moho_ui_shell::CONSOLE_BINDING;

const fn stick(dir: StickDir) -> Binding {
    Binding::Pad(PadInput::Stick(Stick::LeftStick, dir))
}

const fn button(button: PadButton) -> Binding {
    Binding::Pad(PadInput::Button(button))
}

const FORWARD: [Binding; 2] = [Binding::Key(Key::W), stick(StickDir::Up)];
const BACK: [Binding; 2] = [Binding::Key(Key::S), stick(StickDir::Down)];
const LEFT: [Binding; 2] = [Binding::Key(Key::A), stick(StickDir::Left)];
const RIGHT: [Binding; 2] = [Binding::Key(Key::D), stick(StickDir::Right)];
const ASCEND: [Binding; 2] = [Binding::Key(Key::Space), button(PadButton::RightTrigger)];
const JUMP: [Binding; 2] = [Binding::Key(Key::Space), button(PadButton::South)];
const DESCEND: [Binding; 2] = [Binding::Key(Key::Ctrl), button(PadButton::LeftTrigger)];
const SPRINT: [Binding; 2] = [Binding::Key(Key::Shift), button(PadButton::LeftThumb)];

const SELECT: [Binding; 1] = [Binding::Mouse(MouseButton::Left)];

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StrategyAction {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    Ascend,
    Descend,
    Sprint,
    Jump,
    /// The strategy view's primary pointer action.
    Select,
}

impl Action for StrategyAction {
    const ALL: &'static [Self] = &[
        StrategyAction::MoveForward,
        StrategyAction::MoveBack,
        StrategyAction::MoveLeft,
        StrategyAction::MoveRight,
        StrategyAction::Ascend,
        StrategyAction::Descend,
        StrategyAction::Sprint,
        StrategyAction::Jump,
        StrategyAction::Select,
    ];

    fn name(self) -> &'static str {
        match self {
            StrategyAction::MoveForward => "move_forward",
            StrategyAction::MoveBack => "move_back",
            StrategyAction::MoveLeft => "move_left",
            StrategyAction::MoveRight => "move_right",
            StrategyAction::Ascend => "ascend",
            StrategyAction::Descend => "descend",
            StrategyAction::Sprint => "sprint",
            StrategyAction::Jump => "jump",
            StrategyAction::Select => "select",
        }
    }

    fn default_bindings(self) -> &'static [Binding] {
        match self {
            StrategyAction::MoveForward => &FORWARD,
            StrategyAction::MoveBack => &BACK,
            StrategyAction::MoveLeft => &LEFT,
            StrategyAction::MoveRight => &RIGHT,
            StrategyAction::Ascend => &ASCEND,
            StrategyAction::Jump => &JUMP,
            StrategyAction::Descend => &DESCEND,
            StrategyAction::Sprint => &SPRINT,
            StrategyAction::Select => &SELECT,
        }
    }
}

/// Reads the strategy bindings from prefs, warning about each line that fell back to a default.
/// The `console` entry belongs to the console, not to a strategy action, and is skipped.
pub fn load_bindings(prefs: &Prefs) -> ActionBindings<StrategyAction> {
    let mut section = prefs.bindings().clone();
    section.remove(CONSOLE_BINDING);
    let (bindings, warnings) = ActionBindings::load(&section);
    for w in warnings {
        tracing::warn!(action = %w.name, value = %w.value, "Unusable binding, default kept");
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_input::key::{Key, MouseButton};
    use moho_input::pad::{PadButton, PadInput, Stick, StickDir};

    #[test]
    fn strategy_actions_keep_their_ids_and_defaults() {
        let stick = |dir| Binding::Pad(PadInput::Stick(Stick::LeftStick, dir));
        let button = |b| Binding::Pad(PadInput::Button(b));
        let expected = [
            (
                StrategyAction::MoveForward,
                "move_forward",
                Key::W,
                stick(StickDir::Up),
            ),
            (
                StrategyAction::MoveBack,
                "move_back",
                Key::S,
                stick(StickDir::Down),
            ),
            (
                StrategyAction::MoveLeft,
                "move_left",
                Key::A,
                stick(StickDir::Left),
            ),
            (
                StrategyAction::MoveRight,
                "move_right",
                Key::D,
                stick(StickDir::Right),
            ),
            (
                StrategyAction::Ascend,
                "ascend",
                Key::Space,
                button(PadButton::RightTrigger),
            ),
            (
                StrategyAction::Descend,
                "descend",
                Key::Ctrl,
                button(PadButton::LeftTrigger),
            ),
            (
                StrategyAction::Sprint,
                "sprint",
                Key::Shift,
                button(PadButton::LeftThumb),
            ),
            (
                StrategyAction::Jump,
                "jump",
                Key::Space,
                button(PadButton::South),
            ),
        ];

        let actual: Vec<_> = StrategyAction::ALL
            .iter()
            .filter(|&&a| a != StrategyAction::Select)
            .map(|a| (*a, a.name(), a.default_bindings().to_vec()))
            .collect();

        let want: Vec<_> = expected
            .iter()
            .map(|&(a, n, k, p)| (a, n, vec![Binding::Key(k), p]))
            .collect();
        assert_eq!(actual, want);
    }

    #[test]
    fn select_is_the_left_mouse_button() {
        assert_eq!(StrategyAction::Select.name(), "select");
        assert_eq!(
            StrategyAction::Select.default_bindings(),
            [Binding::Mouse(MouseButton::Left)]
        );
        assert!(StrategyAction::ALL.contains(&StrategyAction::Select));
    }

    #[test]
    fn saved_bindings_without_select_load_its_default() {
        let (prefs, _) = Prefs::parse("[bindings]\nmove_forward = Q\n");

        let bindings = load_bindings(&prefs);

        assert_eq!(
            bindings.get(StrategyAction::Select),
            [Binding::Mouse(MouseButton::Left)]
        );
    }

    #[test]
    fn old_per_key_lines_are_not_migrated() {
        let (prefs, _) = Prefs::parse("[prefs]\nkey_w = Z\n");

        let bindings = load_bindings(&prefs);

        assert_eq!(
            bindings.get(StrategyAction::MoveForward),
            [Binding::Key(Key::W), stick(StickDir::Up)]
        );
    }

    /// Collects what `tracing` writes while `run` executes.
    fn logged(run: impl FnOnce()) -> String {
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
        tracing::subscriber::with_default(subscriber, run);
        let bytes = buffer.lock().expect("log sink").clone();
        String::from_utf8(bytes).expect("log output is utf-8")
    }

    #[test]
    fn load_bindings_skips_the_console_entry() {
        let (prefs, _) = Prefs::parse("[bindings]\nconsole = Backslash\nmove_forward = Q\n");
        let mut loaded = None;

        let log = logged(|| loaded = Some(load_bindings(&prefs)));

        let bindings = loaded.expect("bindings loaded");
        assert!(!log.contains("console"), "warned about the console: {log}");
        assert_eq!(
            bindings.get(StrategyAction::MoveForward),
            [Binding::Key(Key::Q)]
        );
        assert_eq!(
            bindings.get(StrategyAction::Jump),
            StrategyAction::Jump.default_bindings()
        );
    }

    #[test]
    fn load_bindings_still_warns_about_an_unknown_entry() {
        let (prefs, _) = Prefs::parse("[bindings]\nfly_to_moon = Q\n");

        let log = logged(|| {
            load_bindings(&prefs);
        });

        assert!(log.contains("fly_to_moon"), "no warning logged: {log}");
    }
}
