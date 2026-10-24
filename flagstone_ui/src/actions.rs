//! The strategy game's actions: stable ids and default bindings.

use moho_core::prefs::Prefs;
use moho_input::Action;
use moho_input::bindings::{ActionBindings, Binding};
use moho_input::key::{Key, MouseButton};
use moho_input::pad::{PadButton, PadInput, Stick, StickDir};
use moho_ui_shell::CONSOLE_BINDING;

const fn stick(dir: StickDir) -> Binding {
    Binding::pad(PadInput::Stick(Stick::LeftStick, dir))
}

const fn button(button: PadButton) -> Binding {
    Binding::pad(PadInput::Button(button))
}

const FORWARD: [Binding; 2] = [Binding::key(Key::W), stick(StickDir::Up)];
const BACK: [Binding; 2] = [Binding::key(Key::S), stick(StickDir::Down)];
const LEFT: [Binding; 2] = [Binding::key(Key::A), stick(StickDir::Left)];
const RIGHT: [Binding; 2] = [Binding::key(Key::D), stick(StickDir::Right)];
const ASCEND: [Binding; 2] = [Binding::key(Key::Space), button(PadButton::RightTrigger)];
const JUMP: [Binding; 2] = [Binding::key(Key::Space), button(PadButton::South)];
const DESCEND: [Binding; 2] = [Binding::key(Key::Ctrl), button(PadButton::LeftTrigger)];
const SPRINT: [Binding; 2] = [Binding::key(Key::Shift), button(PadButton::LeftThumb)];

const SELECT: [Binding; 1] = [Binding::mouse(MouseButton::Left)];
const POSSESS: [Binding; 1] = [Binding::key(Key::P)];

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
    /// Toggles between the strategy view and first person.
    Possess,
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
        StrategyAction::Possess,
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
            StrategyAction::Possess => "possess",
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
            StrategyAction::Possess => &POSSESS,
        }
    }

    fn toggleable(self) -> bool {
        self == StrategyAction::Sprint
    }
}

/// Reads the strategy bindings from prefs, warning about each line that fell back to a default.
/// The `console` entry belongs to the console, not to a strategy action, and is skipped.
pub fn load_bindings(prefs: &Prefs) -> ActionBindings<StrategyAction> {
    let mut section = prefs.bindings().clone();
    section.remove(CONSOLE_BINDING);
    let (mut bindings, warnings) = ActionBindings::load(&section);
    for w in warnings {
        tracing::warn!(action = %w.name, value = %w.value, "Unusable binding, default kept");
    }
    for w in bindings.load_modes(prefs.action_modes()) {
        tracing::warn!(action = %w.name, value = %w.value, "Unusable action mode, hold kept");
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_input::bindings::ActionMode;
    use moho_input::key::{Key, MouseButton};
    use moho_input::pad::{PadButton, PadInput, Stick, StickDir};

    #[test]
    fn strategy_actions_keep_their_ids_and_defaults() {
        let stick = |dir| Binding::pad(PadInput::Stick(Stick::LeftStick, dir));
        let button = |b| Binding::pad(PadInput::Button(b));
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
            .filter(|&&a| a != StrategyAction::Select && a != StrategyAction::Possess)
            .map(|a| (*a, a.name(), a.default_bindings().to_vec()))
            .collect();

        let want: Vec<_> = expected
            .iter()
            .map(|&(a, n, k, p)| (a, n, vec![Binding::key(k), p]))
            .collect();
        assert_eq!(actual, want);
    }

    #[test]
    fn only_sprint_is_toggleable() {
        for &action in StrategyAction::ALL {
            assert_eq!(
                action.toggleable(),
                action == StrategyAction::Sprint,
                "{}",
                action.name()
            );
        }
    }

    #[test]
    fn saved_action_modes_load_into_the_bindings() {
        let (prefs, _) = Prefs::parse("[action_modes]\nsprint = toggle\n");

        let bindings = load_bindings(&prefs);

        assert_eq!(bindings.mode(StrategyAction::Sprint), ActionMode::Toggle);
        assert_eq!(bindings.mode(StrategyAction::Jump), ActionMode::Hold);
    }

    #[test]
    fn unparseable_action_mode_warns_naming_sprint_and_keeps_hold() {
        let (prefs, _) = Prefs::parse("[action_modes]\nsprint = sometimes\n");
        let mut loaded = None;

        let log = logged(|| loaded = Some(load_bindings(&prefs)));

        let bindings = loaded.expect("bindings loaded");
        assert!(log.contains("sprint"), "no warning naming sprint: {log}");
        assert_eq!(bindings.mode(StrategyAction::Sprint), ActionMode::Hold);
    }

    #[test]
    fn select_is_the_left_mouse_button() {
        assert_eq!(StrategyAction::Select.name(), "select");
        assert_eq!(
            StrategyAction::Select.default_bindings(),
            [Binding::mouse(MouseButton::Left)]
        );
        assert!(StrategyAction::ALL.contains(&StrategyAction::Select));
    }

    #[test]
    fn possess_defaults_to_tab_and_collides_with_no_other_default() {
        let possess = StrategyAction::Possess.default_bindings();

        let collisions: Vec<_> = StrategyAction::ALL
            .iter()
            .filter(|&&other| other != StrategyAction::Possess)
            .filter(|other| {
                other
                    .default_bindings()
                    .iter()
                    .any(|binding| possess.contains(binding))
            })
            .map(|other| other.name())
            .collect();

        assert_eq!(StrategyAction::Possess.name(), "possess");
        assert_eq!(possess, [Binding::key(Key::Tab)]);
        assert!(collisions.is_empty(), "Possess collides with {collisions:?}");
    }

    #[test]
    fn saved_bindings_without_possess_load_its_default() {
        let (prefs, _) = Prefs::parse("[bindings]\nmove_forward = Q\n");

        let bindings = load_bindings(&prefs);

        assert_eq!(
            bindings.get(StrategyAction::Possess),
            [Binding::key(Key::Tab)]
        );
    }

    #[test]
    fn saved_bindings_without_select_load_its_default() {
        let (prefs, _) = Prefs::parse("[bindings]\nmove_forward = Q\n");

        let bindings = load_bindings(&prefs);

        assert_eq!(
            bindings.get(StrategyAction::Select),
            [Binding::mouse(MouseButton::Left)]
        );
    }

    #[test]
    fn old_per_key_lines_are_not_migrated() {
        let (prefs, _) = Prefs::parse("[prefs]\nkey_w = Z\n");

        let bindings = load_bindings(&prefs);

        assert_eq!(
            bindings.get(StrategyAction::MoveForward),
            [Binding::key(Key::W), stick(StickDir::Up)]
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
            [Binding::key(Key::Q)]
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
