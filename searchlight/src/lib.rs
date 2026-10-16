//! The FPS prototype: arena, shot resolution and the fixed-tick simulation, adapted from
//! the engine crates and the platform-free rules in `searchlight_rules`.

pub mod arena;
pub mod brain;
pub mod console_commands;
pub mod cover;
pub mod game;
pub mod hud;
pub mod input;
pub mod shot;
pub mod sim;
pub mod tag;

/// Ticks per second the prototype runs at.
pub const TICK_HZ: u32 = 60;

#[cfg(test)]
pub(crate) mod test_support;
