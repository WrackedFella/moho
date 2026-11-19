//! The controls a level places in the world and what using one does.

/// What using a control does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Calls the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a level file's `action` value names, or `None` when it names no action.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "next_wave" => Some(Self::NextWave),
            _ => None,
        }
    }

    /// Whether the action changes the whole game, not only the player who used it.
    pub fn is_host_action(self) -> bool {
        match self {
            Self::NextWave => true,
        }
    }

    /// Whether a use of this action by the player in `slot` takes effect: host actions only
    /// from slot 0.
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

        let from_the_host = action.honored_from(0);
        let from_a_guest = [1, 2, u8::MAX].map(|slot| action.honored_from(slot));

        assert!(action.is_host_action());
        assert!(from_the_host);
        assert_eq!(from_a_guest, [false; 3]);
    }

    #[test]
    fn next_wave_parses_from_its_file_name_and_nothing_else_does() {
        let named = ControlAction::parse("next_wave");
        let unknown = ControlAction::parse("open_door");
        let empty = ControlAction::parse("");

        assert_eq!(named, Some(ControlAction::NextWave));
        assert_eq!(unknown, None);
        assert_eq!(empty, None);
    }
}
