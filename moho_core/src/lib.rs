//! Core engine primitives: event bus, preferences, persistence.

pub mod events;
pub mod persist;
pub mod prefs;

pub use events::{Event, EventBus};
