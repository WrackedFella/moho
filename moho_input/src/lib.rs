//! Engine input: platform-free keys and mouse buttons, action-keyed bindings that
//! persist by name, and the action map that turns device input into one frame per tick.

pub mod action_map;
pub mod bindings;
mod filter;
pub mod gamepad;
pub mod key;
pub mod pad;

pub use action_map::LookSettings;
pub use bindings::{Action, DEFAULT_HOLD_TICKS, Input, Modifiers, Trigger};
