//! What a placed control does when the player uses it, and who may use it.

/// The action a placed control triggers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Starts the next wave once the running one is cleared.
    NextWave,
    /// Flips god mode for the session.
    ToggleGodMode,
}

impl ControlAction {
    /// The action a marker's `action` value names, or `None` when it names none on the list.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "next_wave" => Some(Self::NextWave),
            "god_mode" => Some(Self::ToggleGodMode),
            _ => None,
        }
    }

    /// Whether only the host may trigger this action.
    pub fn is_host_action(self) -> bool {
        match self {
            Self::NextWave | Self::ToggleGodMode => true,
        }
    }

    /// Whether the player in `slot` may trigger this action: a host action only from slot 0.
    pub fn honored_from(self, slot: u8) -> bool {
        !self.is_host_action() || slot == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        let honored: Vec<bool> = [0, 1, 2, u8::MAX]
            .into_iter()
            .map(|slot| action.honored_from(slot))
            .collect();

        assert!(action.is_host_action());
        assert_eq!(honored, [true, false, false, false]);
    }

    #[test]
    fn god_mode_is_honored_only_from_slot_zero() {
        let action = ControlAction::ToggleGodMode;

        let honored: Vec<bool> = [0, 1, 2, u8::MAX]
            .into_iter()
            .map(|slot| action.honored_from(slot))
            .collect();

        assert!(action.is_host_action());
        assert_eq!(honored, [true, false, false, false]);
    }

    #[test]
    fn god_mode_parses_from_its_marker_value() {
        let named = ControlAction::parse("god_mode");
        let refused = [
            "God_Mode",
            "god mode",
            "godmode",
            " god_mode",
            "god_mode ",
            "toggle_god_mode",
            "tgm",
        ]
        .map(ControlAction::parse);

        assert_eq!(named, Some(ControlAction::ToggleGodMode));
        assert_eq!(refused, [None; 7]);
    }

    #[test]
    fn parse_names_next_wave_and_refuses_anything_else() {
        let named = ControlAction::parse("next_wave");
        let refused = [
            "",
            "bogus",
            "Next_Wave",
            "next wave",
            " next_wave",
            "next_wave ",
        ]
        .map(ControlAction::parse);

        assert_eq!(named, Some(ControlAction::NextWave));
        assert_eq!(refused, [None; 6]);
    }
}
