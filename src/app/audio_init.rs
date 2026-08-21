//! Audio system initialization.
//!
//! This module handles the initialization of the audio system with proper error handling.
//! The audio system is optional - if initialization fails, the application continues
//! without audio rather than crashing.
//!
//! # Design Notes
//!
//! The audio system cannot be subscribed to the event bus because rodio types
//! are not Send/Sync. Audio events must be processed manually in the frame loop.
//!
//! # Graceful Degradation
//!
//! Audio initialization can fail for several reasons:
//! - No audio output device available
//! - Audio device in use by another application
//! - Driver issues or permissions problems
//!
//! In all cases, the application logs a warning and continues without audio.

use moho_audio::AudioSystem;

/// Result of audio system initialization.
///
/// Returns `Some(AudioSystem)` on success, `None` if initialization failed.
/// Errors are logged internally - the caller just needs to check if audio is available.
pub fn initialize_audio_system() -> Option<AudioSystem> {
    match AudioSystem::new() {
        Ok(audio) => {
            log::info!("Audio system initialized successfully");
            Some(audio)
        }
        Err(e) => {
            log::warn!("Failed to initialize audio system: {}", e);
            log::info!("Application will continue without audio");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_initialization_does_not_panic() {
        // Audio init must degrade gracefully (no device, driver issues, etc.)
        // rather than crash app startup — this is the behavior worth guarding.
        let _result = initialize_audio_system();
    }
}
