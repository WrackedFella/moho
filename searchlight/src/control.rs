//! What a placed control does when used, and who may use it.

/// An action a placed control can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ControlAction {
    /// Calls the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a level marker names, or `None` when `name` is not on the list.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "next_wave" => Some(Self::NextWave),
            _ => None,
        }
    }

    /// Whether the action changes the match for everyone, so only the host may use it.
    pub fn is_host_action(self) -> bool {
        match self {
            Self::NextWave => true,
        }
    }

    /// Whether a use from player `slot` is honored.
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
        assert_eq!(ControlAction::parse("nextwave"), None);
        assert_eq!(ControlAction::parse(""), None);
    }

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        assert!(action.is_host_action());
        assert!(action.honored_from(0));
        assert!(!action.honored_from(1));
        assert!(!action.honored_from(u8::MAX));
    }
}
