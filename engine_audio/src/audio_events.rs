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
    CustomSound {
        path: String,
        volume: f32,
    },
    
    /// Background music control
    BackgroundMusic {
        path: String,
        volume: f32,
        looped: bool,
    },
    
    /// Stop all audio or specific category
    Stop(AudioCategory),
}

/// Categories of audio for selective control
#[derive(Debug, Clone, PartialEq)]
pub enum AudioCategory {
    /// All audio sources
    All,
    /// Sound effects only
    SoundEffects,
    /// Background music only
    Music,
    /// UI interaction sounds only
    UserInterface,
}