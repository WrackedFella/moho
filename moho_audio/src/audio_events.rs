//! Events that trigger audio playback in the Moho engine.
//!
//! Re-exported from `moho_core` — the event bus is the single source of
//! truth for event shapes; `moho_audio` consumes them without redefining.
pub use moho_core::events::AudioEvent;
