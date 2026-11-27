//! What a placed control does when used, and which player slot may use it.

/// What using a control does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Starts the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a level marker's `action` value names, `None` for an unknown name.
    pub fn parse(_name: &str) -> Option<Self> {
        todo!("parse a control action name")
    }

    /// Whether the action changes the whole session rather than one player.
    pub fn is_host_action(self) -> bool {
        todo!("say which actions are host actions")
    }

    /// Whether a use from player `slot` takes effect: a host action only from slot 0.
    pub fn honored_from(self, _slot: u8) -> bool {
        todo!("apply the host rule to a slot")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        let from_zero = action.honored_from(0);
        let from_one = action.honored_from(1);
        let from_last = action.honored_from(u8::MAX);

        assert!(action.is_host_action());
        assert!(from_zero);
        assert!(!from_one);
        assert!(!from_last);
    }

    #[test]
    fn the_next_wave_name_parses_and_anything_else_does_not() {
        let known = ControlAction::parse("next_wave");
        let unknown = ControlAction::parse("open_door");
        let empty = ControlAction::parse("");

        assert_eq!(known, Some(ControlAction::NextWave));
        assert_eq!(unknown, None);
        assert_eq!(empty, None);
    }
}
