//! Screen components for full-screen UI elements
//!
//! This module contains all full-screen UI components including menus and forms.
//! Screens are modal by default, taking exclusive focus and blocking the game.

mod form_controls;
mod menu;
mod new_world;
mod settings;
mod start;

// Re-export core traits and types
pub use menu::{Menu, MenuAction, MenuItem, Screen, ScreenSpec, UiComponent};

// Re-export screen implementations
pub use new_world::NewWorldMenu;
pub use settings::SettingsMenu;
pub use start::StartMenu;

// Re-export form controls
pub use form_controls::FormControls;
