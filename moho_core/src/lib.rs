//! Core engine primitives: event bus, materials, preferences, persistence.

pub mod events;
pub mod materials;
pub mod persist;
pub mod prefs;

pub use events::{Event, EventBus};
