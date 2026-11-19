//! What a placed control does when the player uses it, and which player slots may use it.

/// What using a placed control does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Starts the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a level marker's `action` value names, or `None` for any other value.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "next_wave" => Some(Self::NextWave),
            _ => None,
        }
    }

    /// Whether the action changes the game for everyone, so only the host may take it.
    pub fn is_host_action(self) -> bool {
        match self {
            Self::NextWave => true,
        }
    }

    /// Whether a use from player `slot` takes effect: a host action only from slot 0.
    pub fn honored_from(self, slot: u8) -> bool {
        !self.is_host_action() || slot == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_wave_action_is_named_next_wave() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
    }

    #[test]
    fn a_name_off_the_list_parses_to_nothing() {
        for name in [
            "",
            "teleport",
            "Next_Wave",
            "next_wave ",
            "next-wave",
            "nextwave",
        ] {
            assert_eq!(ControlAction::parse(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        assert!(action.is_host_action());
        assert!(action.honored_from(0));
        for slot in [1, 2, 7, u8::MAX] {
            assert!(!action.honored_from(slot), "slot {slot}");
        }
    }
}
