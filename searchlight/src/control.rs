//! What a placed control can do when the player uses it.

/// An action a level's `control_NN` marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Calls the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a marker's `action` property names, or `None` when no action has that name.
    pub fn parse(_name: &str) -> Option<Self> {
        todo!("parse the marker's action name")
    }

    /// Whether the action changes the game for everyone, so only the host may use it.
    pub fn is_host_action(self) -> bool {
        todo!("name the host actions")
    }

    /// Whether a use from player `slot` is honored: host actions only from slot 0.
    pub fn honored_from(self, _slot: u8) -> bool {
        todo!("honor host actions only from slot 0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_wave_is_named_next_wave() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
    }

    #[test]
    fn an_unknown_or_empty_or_miscased_action_name_parses_to_nothing() {
        for name in ["", "nextwave", "Next_Wave", "next_wave ", "open_door"] {
            assert_eq!(ControlAction::parse(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        let from_host = action.honored_from(0);
        let from_guests = [1, 2, 255].map(|slot| action.honored_from(slot));

        assert!(action.is_host_action());
        assert!(from_host);
        assert_eq!(from_guests, [false, false, false]);
    }
}
