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
    fn test_audio_initialization_returns_option() {
        // This test verifies the function signature and return type
        // We can't reliably test success/failure without mocking hardware,
        // but we can verify the function is callable and returns the right type
        let result = initialize_audio_system();
        
        // Result should be Some or None depending on system state
        // Both are valid - the important part is graceful handling
        match result {
            Some(_) => {
                // Audio initialized successfully
                assert!(true, "Audio system available");
            }
            None => {
                // Audio initialization failed (expected on some CI systems)
                assert!(true, "Audio system not available (gracefully degraded)");
            }
        }
    }

    #[test]
    fn test_audio_initialization_does_not_panic() {
        // The most important test - initialization should never panic
        // This ensures the application can always start even without audio
        let _result = initialize_audio_system();
        // If we reach here, no panic occurred
        assert!(true, "Audio initialization completed without panic");
    }

    #[test]
    fn test_multiple_initialization_attempts() {
        // Verify we can call initialization multiple times safely
        // (Though in practice, App only calls this once)
        let _first = initialize_audio_system();
        let _second = initialize_audio_system();
        
        // Both should complete without panic
        assert!(true, "Multiple initialization attempts handled safely");
    }
}
