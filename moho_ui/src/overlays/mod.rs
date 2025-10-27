//! In-game overlay components
//!
//! This module contains non-modal UI elements that appear during gameplay,
//! such as HUD elements, notifications, and interactive in-game menus.
//!
//! Overlays differ from screens in that they:
//! - Do not block the game (non-modal)
//! - Can be partially transparent
//! - May receive limited input (some pass-through to game)
//! - Can be shown/hidden independently

pub mod console;

pub use console::{Console, ConsoleAction};
