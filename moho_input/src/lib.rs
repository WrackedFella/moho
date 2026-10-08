//! Engine input: platform-free keys and mouse buttons, action-keyed bindings that
//! persist by name, and the action map that turns device input into one frame per tick.

pub mod action_map;
pub mod bindings;
mod filter;
pub mod key;

pub use bindings::Action;
