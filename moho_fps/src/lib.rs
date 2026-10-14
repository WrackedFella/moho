//! The FPS prototype: arena, shot resolution and the fixed-tick simulation, adapted from
//! the engine crates and the platform-free rules in `moho_fps_rules`.

pub mod arena;
pub mod game;
pub mod hud;
pub mod input;
pub mod shot;
pub mod sim;
pub mod tag;

#[cfg(test)]
pub(crate) mod test_support;
