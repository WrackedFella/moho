//! What a placed world control does when the player uses it.

/// An action a level's control marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAction {
    /// Calls the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a marker's `action` value names, or `None` when it names none on the list.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "next_wave" => Some(Self::NextWave),
            _ => None,
        }
    }

    /// Whether the action changes the game for everyone, not only the player who used it.
    pub fn is_host_action(self) -> bool {
        match self {
            Self::NextWave => true,
        }
    }

    /// Whether a use by player `slot` carries out the action: a host action only from slot 0.
    pub fn honored_from(self, slot: u8) -> bool {
        slot == 0 || !self.is_host_action()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_wave_is_the_one_name_on_the_list() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
    }

    #[test]
    fn a_name_not_on_the_list_parses_to_nothing() {
        for name in ["", "nextwave", "next wave", "god_mode", "next_wave "] {
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

        let from_host = action.honored_from(0);
        let from_others: Vec<u8> = [1, 2, 3, u8::MAX]
            .into_iter()
            .filter(|&slot| action.honored_from(slot))
            .collect();

        assert!(from_host);
        assert_eq!(from_others, Vec::<u8>::new());
    }
}
