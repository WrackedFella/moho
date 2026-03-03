use crate::audio_source::AudioCategory;

/// Events that trigger audio playback in the Moho engine.
///
/// This event-driven approach allows for clean separation between
/// UI/game logic and audio system implementation.

#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    /// Button click or UI interaction sound
    ButtonClick,

    /// Menu navigation sound (hover, focus change)
    MenuNavigate,

    /// Confirmation/accept action
    Confirm,

    /// Cancel/back action  
    Cancel,

    /// Error or invalid action sound
    Error,

    /// Custom sound effect with file path and volume
    CustomSound { path: String, volume: f32 },

    /// Background music control
    BackgroundMusic {
        path: String,
        volume: f32,
        looped: bool,
    },

    /// Stop audio. `None` stops all categories.
    Stop(Option<AudioCategory>),
}
