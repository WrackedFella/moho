//! What a placed control does when the player uses it, and who may use it.

/// An action a level's control marker can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlAction {
    /// Call the next wave once the running one is cleared.
    NextWave,
}

impl ControlAction {
    /// The action a marker's `action` value names, or `None` when it names none on the list.
    pub fn parse(name: &str) -> Option<Self> {
        let _ = name;
        todo!("ControlAction::parse is not implemented")
    }

    /// Whether the action changes the whole game, not one player.
    pub fn is_host_action(self) -> bool {
        todo!("ControlAction::is_host_action is not implemented")
    }

    /// Whether a use from player `slot` takes effect: host actions only from slot 0.
    pub fn honored_from(self, slot: u8) -> bool {
        let _ = slot;
        todo!("ControlAction::honored_from is not implemented")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_action_is_honored_only_from_slot_zero() {
        let action = ControlAction::NextWave;

        let host = action.honored_from(0);
        let others = [1, 2, 7, u8::MAX].map(|slot| action.honored_from(slot));

        assert!(action.is_host_action());
        assert!(host);
        assert_eq!(others, [false; 4]);
    }

    #[test]
    fn the_next_wave_action_is_named_next_wave_and_nothing_else_parses() {
        assert_eq!(
            ControlAction::parse("next_wave"),
            Some(ControlAction::NextWave)
        );
        for other in ["", "Next_Wave", "next wave", "god_mode", " next_wave"] {
            assert_eq!(ControlAction::parse(other), None, "{other:?}");
        }
    }
}
