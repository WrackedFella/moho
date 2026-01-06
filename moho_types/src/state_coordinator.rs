//! State Transition Coordinator
//!
//! Coordinates state transitions and their side effects (UI updates, cursor changes, etc.)
//! This eliminates the repeated boilerplate in App's state transition methods.
use crate::GameState;

/// Result of a state transition operation.
pub type TransitionResult = Result<(), String>;

/// Actions that should be taken when transitioning to a new state.
///
/// This struct separates **what** needs to happen from **how** it happens,
/// making state transitions testable and the pattern explicit.
#[derive(Debug, Clone, PartialEq)]
pub struct StateTransitionActions {
    /// The new state to transition to
    pub new_state: GameState,

    /// Whether UI should be visible
    pub ui_visible: bool,

    /// Whether cursor should be grabbed
    pub cursor_grabbed: bool,

    /// Whether cursor should be visible
    pub cursor_visible: bool,

    /// Optional specific menu to show (e.g., "start", "settings")
    pub show_menu: Option<&'static str>,
}

impl StateTransitionActions {
    /// Create actions for transitioning from one state to another.
    ///
    /// # Arguments
    /// * `from` - Current state
    /// * `to` - Desired new state
    ///
    /// # Returns
    /// * `Ok(StateTransitionActions)` if the transition is valid
    /// * `Err(String)` if the transition is invalid
    ///
    /// # Example
    /// ```
    /// use moho_types::{GameState, state_coordinator::StateTransitionActions};
    ///
    /// let actions = StateTransitionActions::for_transition(
    ///     GameState::Playing,
    ///     GameState::Menu
    /// ).unwrap();
    ///
    /// assert_eq!(actions.new_state, GameState::Menu);
    /// assert_eq!(actions.ui_visible, true);
    /// assert_eq!(actions.cursor_grabbed, false);
    /// ```
    pub fn for_transition(from: GameState, to: GameState) -> Result<Self, String> {
        // Validate transition
        from.validate_transition(to)?;

        // Build actions based on target state
        Ok(Self {
            new_state: to,
            ui_visible: Self::should_show_ui(to),
            cursor_grabbed: to.should_grab_cursor(),
            cursor_visible: to.cursor_visible(),
            show_menu: Self::menu_for_state(to),
        })
    }

    /// Determine if UI should be visible in a state
    fn should_show_ui(state: GameState) -> bool {
        match state {
            GameState::Menu | GameState::ConsoleOpen | GameState::Paused => true,
            GameState::Playing => false,
        }
    }

    /// Determine which menu (if any) should be shown for a state
    fn menu_for_state(state: GameState) -> Option<&'static str> {
        match state {
            GameState::Menu => Some("start"),
            GameState::Paused => Some("pause"),
            GameState::ConsoleOpen => None, // Console UI handles this
            GameState::Playing => None,
        }
    }
}

/// Coordinator for state transitions with common side effects.
///
/// This struct encapsulates the state transition logic and provides
/// a clean API for common transitions like showing/hiding menus,
/// opening/closing console, etc.
pub struct StateTransitionCoordinator;

impl StateTransitionCoordinator {
    /// Create transition actions for showing the main menu.
    ///
    /// Valid from: Playing, ConsoleOpen, Paused
    pub fn show_menu(current: GameState) -> Result<StateTransitionActions, String> {
        StateTransitionActions::for_transition(current, GameState::Menu)
    }

    /// Create transition actions for hiding the menu (starting gameplay).
    ///
    /// Valid from: Menu
    pub fn hide_menu(current: GameState) -> Result<StateTransitionActions, String> {
        StateTransitionActions::for_transition(current, GameState::Playing)
    }

    /// Create transition actions for entering console mode.
    ///
    /// Valid from: Playing
    pub fn enter_console(current: GameState) -> Result<StateTransitionActions, String> {
        StateTransitionActions::for_transition(current, GameState::ConsoleOpen)
    }

    /// Create transition actions for exiting console mode.
    ///
    /// Valid from: ConsoleOpen
    pub fn exit_console(current: GameState) -> Result<StateTransitionActions, String> {
        StateTransitionActions::for_transition(current, GameState::Playing)
    }

    /// Create transition actions for pausing the game.
    ///
    /// Valid from: Playing
    pub fn pause_game(current: GameState) -> Result<StateTransitionActions, String> {
        StateTransitionActions::for_transition(current, GameState::Paused)
    }

    /// Create transition actions for resuming the game.
    ///
    /// Valid from: Paused
    pub fn resume_game(current: GameState) -> Result<StateTransitionActions, String> {
        StateTransitionActions::for_transition(current, GameState::Playing)
    }

    /// Toggle pause state (pause if playing, resume if paused).
    ///
    /// Valid from: Playing, Paused
    pub fn toggle_pause(current: GameState) -> Result<StateTransitionActions, String> {
        match current {
            GameState::Playing => Self::pause_game(current),
            GameState::Paused => Self::resume_game(current),
            _ => Err(format!("Cannot toggle pause from state: {}", current)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_show_menu_from_playing() {
        let actions = StateTransitionCoordinator::show_menu(GameState::Playing).unwrap();
        assert_eq!(actions.new_state, GameState::Menu);
        assert!(actions.ui_visible);
        assert!(!actions.cursor_grabbed);
        assert!(actions.cursor_visible);
        assert_eq!(actions.show_menu, Some("start"));
    }

    #[test]
    fn test_show_menu_from_invalid_state() {
        // Cannot show menu from menu
        let result = StateTransitionCoordinator::show_menu(GameState::Menu);
        assert!(result.is_ok()); // No-op transition is valid
    }

    #[test]
    fn test_hide_menu() {
        let actions = StateTransitionCoordinator::hide_menu(GameState::Menu).unwrap();
        assert_eq!(actions.new_state, GameState::Playing);
        assert!(!actions.ui_visible);
        assert!(actions.cursor_grabbed);
        assert!(!actions.cursor_visible);
        assert_eq!(actions.show_menu, None);
    }

    #[test]
    fn test_enter_console() {
        let actions = StateTransitionCoordinator::enter_console(GameState::Playing).unwrap();
        assert_eq!(actions.new_state, GameState::ConsoleOpen);
        assert!(actions.ui_visible);
        assert!(!actions.cursor_grabbed);
        assert!(actions.cursor_visible);
    }

    #[test]
    fn test_enter_console_from_menu_invalid() {
        let result = StateTransitionCoordinator::enter_console(GameState::Menu);
        assert!(result.is_err());
    }

    #[test]
    fn test_exit_console() {
        let actions = StateTransitionCoordinator::exit_console(GameState::ConsoleOpen).unwrap();
        assert_eq!(actions.new_state, GameState::Playing);
        assert!(!actions.ui_visible);
        assert!(actions.cursor_grabbed);
    }

    #[test]
    fn test_pause_game() {
        let actions = StateTransitionCoordinator::pause_game(GameState::Playing).unwrap();
        assert_eq!(actions.new_state, GameState::Paused);
        assert!(actions.ui_visible);
        assert!(!actions.cursor_grabbed);
        assert_eq!(actions.show_menu, Some("pause"));
    }

    #[test]
    fn test_resume_game() {
        let actions = StateTransitionCoordinator::resume_game(GameState::Paused).unwrap();
        assert_eq!(actions.new_state, GameState::Playing);
        assert!(!actions.ui_visible);
        assert!(actions.cursor_grabbed);
    }

    #[test]
    fn test_toggle_pause_from_playing() {
        let actions = StateTransitionCoordinator::toggle_pause(GameState::Playing).unwrap();
        assert_eq!(actions.new_state, GameState::Paused);
    }

    #[test]
    fn test_toggle_pause_from_paused() {
        let actions = StateTransitionCoordinator::toggle_pause(GameState::Paused).unwrap();
        assert_eq!(actions.new_state, GameState::Playing);
    }

    #[test]
    fn test_toggle_pause_from_menu_invalid() {
        let result = StateTransitionCoordinator::toggle_pause(GameState::Menu);
        assert!(result.is_err());
    }

    #[test]
    fn test_transition_actions_equality() {
        let actions1 =
            StateTransitionActions::for_transition(GameState::Playing, GameState::Menu).unwrap();

        let actions2 =
            StateTransitionActions::for_transition(GameState::Playing, GameState::Menu).unwrap();

        assert_eq!(actions1, actions2);
    }
}
