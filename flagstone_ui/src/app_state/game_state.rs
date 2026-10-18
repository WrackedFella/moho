//! The strategy game's application modes.

use flagstone_game::controller::CameraMode;
use std::fmt;

/// The current state of the game application.
///
/// This enum defines all possible states the application can be in. Each state
/// has different implications for:
/// - Input routing (which systems receive input)
/// - Rendering (what gets drawn)
/// - Cursor mode (grabbed/visible)
/// - Simulation (running/paused)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    /// Main menu is active, game world is not running.
    /// - Input: Menu system only
    /// - Rendering: Menu screens
    /// - Cursor: Visible and free
    /// - Simulation: Not running
    #[default]
    Menu,

    /// Actively playing the game.
    /// - Input: Game controls (movement, camera, actions)
    /// - Rendering: Game world
    /// - Cursor: Grabbed and hidden in first person, free in the strategy view
    ///   (see [`cursor_grabbed`])
    /// - Simulation: Running
    Playing,

    /// Debug console is open over the game.
    /// - Input: Console text input only
    /// - Rendering: Game world (frozen) + console overlay
    /// - Cursor: Visible and free
    /// - Simulation: Paused
    ConsoleOpen,

    /// Pause menu is open over the game.
    /// - Input: Pause menu navigation
    /// - Rendering: Game world (frozen) + pause menu overlay
    /// - Cursor: Visible and free
    /// - Simulation: Paused
    Paused,
}

impl GameState {
    /// Check if a transition from the current state to a new state is valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use flagstone_ui::GameState;
    ///
    /// assert!(GameState::Playing.can_transition_to(GameState::ConsoleOpen));
    /// assert!(GameState::Playing.can_transition_to(GameState::Paused));
    /// assert!(!GameState::Menu.can_transition_to(GameState::ConsoleOpen));
    /// ```
    pub fn can_transition_to(self, new_state: GameState) -> bool {
        use GameState::{ConsoleOpen, Menu, Paused, Playing};

        match (self, new_state) {
            // Same state is always allowed (no-op)
            (s, n) if s == n => true,

            // From Menu
            (Menu, Playing) => true, // Start game

            // From Playing
            (Playing, ConsoleOpen) => true, // Open console
            (Playing, Paused) => true,      // Pause game
            (Playing, Menu) => true,        // Return to menu

            // From ConsoleOpen
            (ConsoleOpen, Playing) => true, // Close console
            (ConsoleOpen, Menu) => true,    // Exit to menu

            // From Paused
            (Paused, Playing) => true, // Resume game
            (Paused, Menu) => true,    // Exit to menu

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Returns whether the cursor should be visible in this state.
    pub fn cursor_visible(self) -> bool {
        match self {
            GameState::Menu | GameState::ConsoleOpen | GameState::Paused => true,
            GameState::Playing => false,
        }
    }

    /// Returns whether the cursor should be grabbed and hidden in this state.
    pub fn should_grab_cursor(self) -> bool {
        match self {
            GameState::Playing => true,
            GameState::Menu | GameState::ConsoleOpen | GameState::Paused => false,
        }
    }

    /// Returns whether the simulation should be running in this state.
    pub fn is_simulating(self) -> bool {
        match self {
            GameState::Playing => true,
            GameState::Menu | GameState::ConsoleOpen | GameState::Paused => false,
        }
    }

    /// The screen a "back" navigation returns to in this state, if the state has one.
    pub fn home_screen(self) -> Option<&'static str> {
        match self {
            GameState::Menu => Some("start"),
            GameState::Paused => Some("pause"),
            GameState::ConsoleOpen | GameState::Playing => None,
        }
    }

    /// Validate a state transition and return an error if invalid.
    pub fn validate_transition(self, new_state: GameState) -> Result<(), String> {
        if self.can_transition_to(new_state) {
            Ok(())
        } else {
            Err(format!(
                "Invalid state transition: {self:?} -> {new_state:?}"
            ))
        }
    }

    /// Returns true if the game world should be rendered in this state.
    pub fn should_render_world(self) -> bool {
        matches!(
            self,
            GameState::Playing | GameState::ConsoleOpen | GameState::Paused
        )
    }

    /// Returns true if UI overlays should be rendered.
    pub fn should_render_overlay(self) -> bool {
        matches!(self, GameState::ConsoleOpen | GameState::Paused)
    }

    /// Returns true if the main menu should be rendered.
    pub fn should_render_menu(self) -> bool {
        matches!(self, GameState::Menu)
    }
}

/// Whether the cursor is grabbed and hidden: only while playing in first person.
///
/// The strategy view needs a free cursor to click the world with.
pub fn cursor_grabbed(state: GameState, camera_mode: CameraMode) -> bool {
    state == GameState::Playing && camera_mode == CameraMode::FirstPerson
}

impl fmt::Display for GameState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameState::Menu => write!(f, "Menu"),
            GameState::Playing => write!(f, "Playing"),
            GameState::ConsoleOpen => write!(f, "ConsoleOpen"),
            GameState::Paused => write!(f, "Paused"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GameState::{self, ConsoleOpen, Menu, Paused, Playing};
    use super::cursor_grabbed;
    use flagstone_game::controller::CameraMode;

    #[test]
    fn can_transition_to_matrix() {
        let rows: [(GameState, GameState, bool); 16] = [
            (Menu, Menu, true),
            (Menu, Playing, true),
            (Menu, ConsoleOpen, false),
            (Menu, Paused, false),
            (Playing, Menu, true),
            (Playing, Playing, true),
            (Playing, ConsoleOpen, true),
            (Playing, Paused, true),
            (ConsoleOpen, Menu, true),
            (ConsoleOpen, Playing, true),
            (ConsoleOpen, ConsoleOpen, true),
            (ConsoleOpen, Paused, false),
            (Paused, Menu, true),
            (Paused, Playing, true),
            (Paused, ConsoleOpen, false),
            (Paused, Paused, true),
        ];

        for (from, to, expected) in rows {
            assert_eq!(from.can_transition_to(to), expected, "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn cursor_free_only_in_strategy_play() {
        let rows = [
            (Playing, CameraMode::FirstPerson, true),
            (Playing, CameraMode::Isometric, false),
            (Menu, CameraMode::FirstPerson, false),
            (Menu, CameraMode::Isometric, false),
            (Paused, CameraMode::FirstPerson, false),
            (Paused, CameraMode::Isometric, false),
            (ConsoleOpen, CameraMode::FirstPerson, false),
            (ConsoleOpen, CameraMode::Isometric, false),
        ];

        for (state, mode, grabbed) in rows {
            assert_eq!(cursor_grabbed(state, mode), grabbed, "{state:?} {mode:?}");
        }
    }

    #[test]
    fn home_screen_per_state() {
        let rows: [(GameState, Option<&str>); 4] = [
            (Menu, Some("start")),
            (Paused, Some("pause")),
            (Playing, None),
            (ConsoleOpen, None),
        ];

        for (state, expected) in rows {
            assert_eq!(state.home_screen(), expected, "{state:?}");
        }
    }
}
