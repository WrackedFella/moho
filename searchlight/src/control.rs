//! What a placed control does when the player uses it, and who may use it.

/// An action a level's control marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Starts the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a marker's `action` value names, or `None` for any other text.
    pub fn parse(name: &str) -> Option<Self> {
        todo!("parse the action list: {name}")
    }

    /// Whether the action changes the match for everyone, so only the host may call it.
    pub fn is_host_action(self) -> bool {
        todo!("host actions: {self:?}")
    }

    /// Whether a use from player `slot` takes effect: host actions only from slot 0.
    pub fn honored_from(self, slot: u8) -> bool {
        todo!("slot rule: {self:?} from {slot}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_wave_parses_from_its_marker_value() {
        let parsed = ControlAction::parse("next_wave");

        assert_eq!(parsed, Some(ControlAction::NextWave));
    }

    #[test]
    fn an_unknown_or_empty_or_miscased_action_does_not_parse() {
        for name in [
            "",
            "nextwave",
            "Next_Wave",
            "next_wave ",
            " next_wave",
            "god_mode",
        ] {
            let parsed = ControlAction::parse(name);

            assert_eq!(parsed, None, "{name:?}");
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

        assert_eq!(honored, [0]);
    }
}
