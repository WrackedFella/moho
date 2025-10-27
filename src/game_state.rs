//! Game state management for the Moho engine.
//!
//! This module provides the core `GameState` enum which serves as the single
//! source of truth for the application's current mode of operation. All state
//! transitions are validated and explicit.

use std::fmt;

/// The current state of the game application.
///
/// This enum defines all possible states the application can be in. Each state
/// has different implications for:
/// - Input routing (which systems receive input)
/// - Rendering (what gets drawn)
/// - Cursor mode (grabbed/visible)
/// - Simulation (running/paused)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameState {
    /// Main menu is active, game world is not running.
    /// - Input: Menu system only
    /// - Rendering: Menu screens
    /// - Cursor: Visible and free
    /// - Simulation: Not running
    Menu,

    /// Actively playing the game.
    /// - Input: Game controls (movement, camera, actions)
    /// - Rendering: Game world
    /// - Cursor: Grabbed and hidden
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
    #[allow(dead_code)]
    Paused,
}

impl GameState {
    /// Check if a transition from the current state to a new state is valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use moho::game_state::GameState;
    ///
    /// assert!(GameState::Playing.can_transition_to(GameState::ConsoleOpen));
    /// assert!(GameState::Playing.can_transition_to(GameState::Paused));
    /// assert!(!GameState::Menu.can_transition_to(GameState::ConsoleOpen));
    /// ```
    pub fn can_transition_to(self, new_state: GameState) -> bool {
        use GameState::*;

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
            (Paused, Playing) => true,     // Resume
            (Paused, Menu) => true,        // Exit to menu
            (Paused, ConsoleOpen) => true, // Open console from pause

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Validate a state transition and return an error if invalid.
    ///
    /// # Errors
    ///
    /// Returns an error message if the transition is not allowed.
    #[allow(dead_code)]
    pub fn validate_transition(self, new_state: GameState) -> Result<(), String> {
        if self.can_transition_to(new_state) {
            Ok(())
        } else {
            Err(format!(
                "Invalid state transition: {:?} -> {:?}",
                self, new_state
            ))
        }
    }

    /// Returns true if the game simulation should be running in this state.
    #[allow(dead_code)]
    pub fn is_simulating(self) -> bool {
        matches!(self, GameState::Playing)
    }

    /// Returns true if the cursor should be grabbed and hidden in this state.
    #[allow(dead_code)]
    pub fn should_grab_cursor(self) -> bool {
        matches!(self, GameState::Playing)
    }

    /// Returns true if the game world should be rendered in this state.
    #[allow(dead_code)]
    pub fn should_render_world(self) -> bool {
        matches!(
            self,
            GameState::Playing | GameState::ConsoleOpen | GameState::Paused
        )
    }

    /// Returns true if UI overlays (console, pause menu) should be rendered.
    #[allow(dead_code)]
    pub fn should_render_overlay(self) -> bool {
        matches!(self, GameState::ConsoleOpen | GameState::Paused)
    }

    /// Returns true if the main menu should be rendered.
    #[allow(dead_code)]
    pub fn should_render_menu(self) -> bool {
        matches!(self, GameState::Menu)
    }
}

impl Default for GameState {
    /// Default state is Menu (game starts in menu).
    fn default() -> Self {
        GameState::Menu
    }
}

impl fmt::Display for GameState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameState::Menu => write!(f, "Menu"),
            GameState::Playing => write!(f, "Playing"),
            GameState::ConsoleOpen => write!(f, "Console Open"),
            GameState::Paused => write!(f, "Paused"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions_from_menu() {
        assert!(GameState::Menu.can_transition_to(GameState::Playing));
        assert!(GameState::Menu.can_transition_to(GameState::Menu)); // No-op
        assert!(!GameState::Menu.can_transition_to(GameState::ConsoleOpen));
        assert!(!GameState::Menu.can_transition_to(GameState::Paused));
    }

    #[test]
    fn test_valid_transitions_from_playing() {
        assert!(GameState::Playing.can_transition_to(GameState::ConsoleOpen));
        assert!(GameState::Playing.can_transition_to(GameState::Paused));
        assert!(GameState::Playing.can_transition_to(GameState::Menu));
        assert!(GameState::Playing.can_transition_to(GameState::Playing)); // No-op
    }

    #[test]
    fn test_valid_transitions_from_console() {
        assert!(GameState::ConsoleOpen.can_transition_to(GameState::Playing));
        assert!(GameState::ConsoleOpen.can_transition_to(GameState::Menu));
        assert!(GameState::ConsoleOpen.can_transition_to(GameState::ConsoleOpen)); // No-op
        assert!(!GameState::ConsoleOpen.can_transition_to(GameState::Paused));
    }

    #[test]
    fn test_valid_transitions_from_paused() {
        assert!(GameState::Paused.can_transition_to(GameState::Playing));
        assert!(GameState::Paused.can_transition_to(GameState::Menu));
        assert!(GameState::Paused.can_transition_to(GameState::ConsoleOpen));
        assert!(GameState::Paused.can_transition_to(GameState::Paused)); // No-op
    }

    #[test]
    fn test_validate_transition() {
        // Valid transitions should return Ok
        assert!(
            GameState::Playing
                .validate_transition(GameState::ConsoleOpen)
                .is_ok()
        );

        // Invalid transitions should return Err
        assert!(
            GameState::Menu
                .validate_transition(GameState::ConsoleOpen)
                .is_err()
        );
    }

    #[test]
    fn test_simulation_running() {
        assert!(!GameState::Menu.is_simulating());
        assert!(GameState::Playing.is_simulating());
        assert!(!GameState::ConsoleOpen.is_simulating());
        assert!(!GameState::Paused.is_simulating());
    }

    #[test]
    fn test_cursor_grab() {
        assert!(!GameState::Menu.should_grab_cursor());
        assert!(GameState::Playing.should_grab_cursor());
        assert!(!GameState::ConsoleOpen.should_grab_cursor());
        assert!(!GameState::Paused.should_grab_cursor());
    }

    #[test]
    fn test_rendering_flags() {
        // Menu state
        assert!(!GameState::Menu.should_render_world());
        assert!(!GameState::Menu.should_render_overlay());
        assert!(GameState::Menu.should_render_menu());

        // Playing state
        assert!(GameState::Playing.should_render_world());
        assert!(!GameState::Playing.should_render_overlay());
        assert!(!GameState::Playing.should_render_menu());

        // Console open
        assert!(GameState::ConsoleOpen.should_render_world());
        assert!(GameState::ConsoleOpen.should_render_overlay());
        assert!(!GameState::ConsoleOpen.should_render_menu());

        // Paused
        assert!(GameState::Paused.should_render_world());
        assert!(GameState::Paused.should_render_overlay());
        assert!(!GameState::Paused.should_render_menu());
    }

    #[test]
    fn test_display() {
        assert_eq!(GameState::Menu.to_string(), "Menu");
        assert_eq!(GameState::Playing.to_string(), "Playing");
        assert_eq!(GameState::ConsoleOpen.to_string(), "Console Open");
        assert_eq!(GameState::Paused.to_string(), "Paused");
    }

    #[test]
    fn test_default() {
        assert_eq!(GameState::default(), GameState::Menu);
    }
}
