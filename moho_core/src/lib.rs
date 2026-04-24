//! Core systems and primitives for the Moho game engine.
//!
//! Provides the foundational building blocks shared across the engine:
//!
//! - **[`events`]** — Pub/sub event bus for inter-system communication
//! - **[`voxel`]** — Voxel grid, chunk management, and mesh generation
//! - **[`controller`]** — First-person / isometric player controller
//! - **[`game_clock`]** — Day/night cycle with celestial body tracking
//! - **[`actors`]** — Primitive scene objects (sphere, cube, custom mesh)
//! - **[`materials`]** — Material types for voxel blocks and actors
//! - **[`raycast`]** — Voxel grid raycasting utility
//! - **[`input`]** — Low-level input collection

pub mod actors;
pub mod controller;
pub mod events;
pub mod game_clock;
pub mod input;
pub mod materials;
pub mod raycast;
pub mod scene_builders;
pub mod voxel;

// Re-export commonly used event types
pub use events::{Event, EventBus};
