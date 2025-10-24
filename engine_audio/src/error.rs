use thiserror::Error;

/// Result type for audio operations
pub type AudioResult<T> = Result<T, AudioError>;

/// Errors that can occur during audio operations
#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Failed to initialize audio system: {0}")]
    InitializationFailed(String),
    
    #[error("Audio file not found: {0}")]
    FileNotFound(String),
    
    #[error("Unsupported audio format: {0}")]
    UnsupportedFormat(String),
    
    #[error("Failed to load audio file: {0}")]
    LoadFailed(String),
    
    #[error("Audio playback failed: {0}")]
    PlaybackFailed(String),
    
    #[error("Audio device error: {0}")]
    DeviceError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}