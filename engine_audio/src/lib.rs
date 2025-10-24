pub mod audio_events;
pub mod audio_settings;
pub mod audio_source;
/// Audio system for the Moho game engine.
///
/// This module provides cross-platform audio functionality including:
/// - Sound effect playback
/// - Background music management  
/// - Audio asset loading and caching
/// - Volume controls and audio settings
/// - Event-driven audio triggers
///
/// The audio system is designed to be modular and easy to integrate
/// with the existing Moho engine architecture.
pub mod audio_system;
pub mod error;

// Re-export main types for convenience
pub use audio_events::AudioEvent;
pub use audio_settings::AudioSettings;
pub use audio_source::AudioSource;
pub use audio_system::AudioSystem;
pub use error::{AudioError, AudioResult};
