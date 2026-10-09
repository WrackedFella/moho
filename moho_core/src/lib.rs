//! Core engine primitives: event bus, materials, input, preferences, persistence.

pub mod events;
pub mod input;
pub mod materials;
pub mod persist;
pub mod prefs;

pub use events::{Event, EventBus};
