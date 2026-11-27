//! What a placed control does when the player uses it, and who may use it.

/// An action a level's `control_NN` marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Starts the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a marker's `action` value names, or `None` for an unknown name.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "next_wave" => Some(Self::NextWave),
            _ => None,
        }
    }

    /// Whether the action changes the whole game rather than the player who used it.
    pub fn is_host_action(self) -> bool {
        match self {
            Self::NextWave => true,
        }
    }

    /// Whether a use from player `slot` takes effect: a host action only from slot 0.
    pub fn honored_from(self, slot: u8) -> bool {
        !self.is_host_action() || slot == 0
    }

    /// What the player reads in the prompt for using the control.
    pub fn prompt_text(self) -> &'static str {
        match self {
            Self::NextWave => "Next wave",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_wave_name_parses_to_the_next_wave_action() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
    }

    #[test]
    fn a_name_that_is_not_on_the_list_does_not_parse() {
        for name in [
            "",
            "fly",
            "next wave",
            "Next_Wave",
            "next_wave ",
            "next_wave2",
        ] {
            assert_eq!(ControlAction::parse(name), None, "{name:?}");
        }
    }

    #[test]
    fn the_next_wave_action_is_a_host_action() {
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
