//! Core engine primitives: event bus, preferences, persistence, seeded RNG.

pub mod content;
pub mod events;
pub mod persist;
pub mod pointer;
pub mod prefs;
pub mod rng;

pub use events::{Event, EventBus};
