//! Application state of the strategy game: the `GameState` modes and the
//! coordinator that validates transitions and describes their side effects.

pub mod game_state;
pub mod state_coordinator;

pub use game_state::GameState;
pub use state_coordinator::{StateTransitionActions, StateTransitionCoordinator};
