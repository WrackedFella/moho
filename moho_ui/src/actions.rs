//! The strategy game's actions: stable ids and default bindings.

use moho_core::prefs::Prefs;
use moho_input::Action;
use moho_input::bindings::{ActionBindings, Binding};
use moho_input::key::Key;

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
        }
    }

    fn default_bindings(self) -> &'static [Binding] {
        match self {
            StrategyAction::MoveForward => &[Binding::Key(Key::W)],
            StrategyAction::MoveBack => &[Binding::Key(Key::S)],
            StrategyAction::MoveLeft => &[Binding::Key(Key::A)],
            StrategyAction::MoveRight => &[Binding::Key(Key::D)],
            StrategyAction::Ascend | StrategyAction::Jump => &[Binding::Key(Key::Space)],
            StrategyAction::Descend => &[Binding::Key(Key::Ctrl)],
            StrategyAction::Sprint => &[Binding::Key(Key::Shift)],
        }
    }
}

/// Reads the strategy bindings from prefs, warning about each line that fell back to a default.
pub fn load_bindings(prefs: &Prefs) -> ActionBindings<StrategyAction> {
    let (bindings, warnings) = ActionBindings::load(prefs.bindings());
    for w in warnings {
        tracing::warn!(action = %w.name, value = %w.value, "Unusable binding, default kept");
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_input::key::Key;
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
            .map(|a| (*a, a.name(), a.default_bindings().to_vec()))
            .collect();

        let want: Vec<_> = expected
            .iter()
            .map(|&(a, n, k, p)| (a, n, vec![Binding::Key(k), p]))
            .collect();
        assert_eq!(actual, want);
    }

    #[test]
    fn old_per_key_lines_are_not_migrated() {
        let (prefs, _) = Prefs::parse("[prefs]\nkey_w = Z\n");

        let bindings = load_bindings(&prefs);

        assert_eq!(
            bindings.get(StrategyAction::MoveForward),
            [Binding::Key(Key::W)]
        );
    }
}
