//! What a placed control does when the player uses it.

/// An action a placed control can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ControlAction {
    /// Calls the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a level's `action` extra names, if it is on the list.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "next_wave" => Some(Self::NextWave),
            _ => None,
        }
    }

    /// Whether only the host may trigger it.
    pub fn is_host_action(self) -> bool {
        match self {
            Self::NextWave => true,
        }
    }

    /// Whether a use from player `slot` triggers it: a host action only from slot 0.
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

        assert!(action.is_host_action());
        assert!(action.honored_from(0));
        assert!(!action.honored_from(1));
        assert!(!action.honored_from(u8::MAX));
    }

    #[test]
    fn the_next_wave_action_is_parsed_by_its_level_name() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
    }

    #[test]
    fn an_unknown_empty_or_miscased_action_name_is_not_parsed() {
        assert_eq!(ControlAction::parse("god_mode"), None);
        assert_eq!(ControlAction::parse(""), None);
        assert_eq!(ControlAction::parse("Next_Wave"), None);
        assert_eq!(ControlAction::parse(" next_wave"), None);
    }
}
