//! Application state types shared across the moho workspace.

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
    Paused,
}

impl GameState {
    /// Check if a transition from the current state to a new state is valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use moho_types::GameState;
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

    /// Validate a state transition and return an error if invalid.
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
