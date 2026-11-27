//! What a control placed in the arena does when the player uses it.

/// What using a control does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Starts the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a level marker's `action` value names, or `None` when it names none.
    pub fn parse(_name: &str) -> Option<Self> {
        unimplemented!("ControlAction::parse")
    }

    /// Whether only the host may use this action.
    pub fn is_host_action(self) -> bool {
        unimplemented!("ControlAction::is_host_action")
    }

    /// Whether a use by player slot `slot` is honored.
    pub fn honored_from(self, _slot: u8) -> bool {
        unimplemented!("ControlAction::honored_from")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_wave_action_parses_from_its_name() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
    }

    #[test]
    fn a_name_that_is_not_an_action_does_not_parse() {
        for name in [
            "",
            "open_sesame",
            "Next_Wave",
            "next wave",
            " next_wave",
            "next_wave ",
        ] {
            assert_eq!(ControlAction::parse(name), None, "{name:?}");
        }
    }

    #[test]
    fn next_wave_is_a_host_action() {
        assert!(ControlAction::NextWave.is_host_action());
    }

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        let honored: Vec<u8> = [0, 1, 2, 7, u8::MAX]
            .into_iter()
            .filter(|&slot| action.honored_from(slot))
            .collect();

        assert!(action.is_host_action(), "the rule applies to host actions");
        assert_eq!(honored, [0]);
    }
}
