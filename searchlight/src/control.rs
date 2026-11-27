//! What a placed control does when the player uses it, and who may use it.

/// An action a level's control marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Calls the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a marker's `action` extra names, or `None` when it is not on the list.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "next_wave" => Some(Self::NextWave),
            _ => None,
        }
    }

    /// Whether only the host may trigger this action.
    pub fn is_host_action(self) -> bool {
        match self {
            Self::NextWave => true,
        }
    }

    /// Whether a use from player `slot` triggers this action; host actions only from slot 0.
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

        let from_host = action.honored_from(0);
        let from_one = action.honored_from(1);
        let from_last = action.honored_from(u8::MAX);

        assert!(action.is_host_action());
        assert!(from_host);
        assert!(!from_one);
        assert!(!from_last);
    }

    #[test]
    fn next_wave_is_named_next_wave_and_nothing_else_parses() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
        assert_eq!(ControlAction::parse("god_mode"), None);
        assert_eq!(ControlAction::parse(""), None);
        assert_eq!(ControlAction::parse("Next_Wave"), None);
    }
}
