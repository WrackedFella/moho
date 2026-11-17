//! What a placed control in the arena can do when the player uses it.

/// An action a control names in its level marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ControlAction {
    /// Starts the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action the marker value `name` names, or `None` if it is not on the list.
    pub fn parse(name: &str) -> Option<Self> {
        let _ = name;
        todo!("parse an action name")
    }

    /// Whether the action changes the whole session, so only the host may call it.
    pub fn is_host_action(self) -> bool {
        todo!("name the host actions")
    }

    /// Whether a use from player `slot` may run the action.
    pub fn honored_from(self, slot: u8) -> bool {
        let _ = slot;
        todo!("honor host actions from slot 0 only")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        let from_host = action.honored_from(0);
        let from_others = [1, 2, 7, u8::MAX].map(|slot| action.honored_from(slot));

        assert!(action.is_host_action());
        assert!(from_host);
        assert_eq!(from_others, [false; 4]);
    }

    #[test]
    fn next_wave_parses_from_its_marker_name_and_nothing_else_does() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
        for other in ["", "god_mode", "Next_Wave", "next_wave ", "nextwave"] {
            assert_eq!(ControlAction::parse(other), None, "{other:?}");
        }
    }
}
