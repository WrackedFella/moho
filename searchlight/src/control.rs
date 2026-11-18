//! What a placed control does when the player uses it.

/// An action a level's control marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Calls the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a marker's `action` value names, or `None` when the list has no such action.
    pub fn parse(_value: &str) -> Option<Self> {
        todo!("the action list")
    }

    /// Whether only the host may use a control of this action.
    pub fn is_host_action(self) -> bool {
        todo!("which actions are the host's")
    }

    /// Whether a use by the player in `slot` is honored: a host action only from slot 0.
    pub fn honored_from(self, _slot: u8) -> bool {
        todo!("the slot rule")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        let from_host = action.honored_from(0);
        let from_guest = action.honored_from(1);
        let from_last = action.honored_from(u8::MAX);

        assert!(action.is_host_action());
        assert!(from_host);
        assert!(!from_guest);
        assert!(!from_last);
    }
}
