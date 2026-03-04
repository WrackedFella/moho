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
//!
//! The [`OverlayManager`] holds all registered [`Overlay`] layers and
//! renders them each frame with shared [`HudData`].

pub mod console;
pub mod debug_hud;
pub mod fps_hud;
pub mod overlay_manager;
pub mod rts_hud;

pub use console::{Console, ConsoleAction};
pub use debug_hud::DebugHud;
pub use fps_hud::FpsHud;
pub use overlay_manager::{HudData, Overlay, OverlayManager};
pub use rts_hud::RtsHud;
