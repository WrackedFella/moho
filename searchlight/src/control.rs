//! What a placed control in the arena does when the player uses it.

/// An action a placed control stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Starts the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a level file's `action` property names, or `None` for an unknown name.
    pub fn parse(_name: &str) -> Option<Self> {
        None
    }

    /// Whether the action changes the whole session rather than the one player using it.
    pub fn is_host_action(self) -> bool {
        false
    }

    /// Whether a use by the player in `slot` is honoured; a host action only from slot 0.
    pub fn honored_from(self, _slot: u8) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        let from_host = action.honored_from(0);
        let from_guests: Vec<bool> = [1, 2, 3, u8::MAX]
            .into_iter()
            .map(|slot| action.honored_from(slot))
            .collect();

        assert!(action.is_host_action());
        assert!(from_host);
        assert_eq!(from_guests, [false; 4]);
    }

    #[test]
    fn the_next_wave_action_is_named_next_wave() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
        assert_eq!(ControlAction::parse("Next_Wave"), None);
        assert_eq!(ControlAction::parse(""), None);
        assert_eq!(ControlAction::parse("open_door"), None);
    }
}
