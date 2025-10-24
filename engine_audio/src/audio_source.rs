use crate::error::{AudioError, AudioResult};
use std::path::Path;

/// Represents an audio source/asset in the engine.
/// 
/// AudioSource handles metadata and provides a clean interface
/// for audio file management and caching.

#[derive(Debug, Clone)]
pub struct AudioSource {
    /// File path to the audio asset
    pub path: String,
    
    /// Volume multiplier for this source (0.0 to 1.0)
    pub volume: f32,
    
    /// Whether this sound should loop when played
    pub looped: bool,
    
    /// Optional 3D position for spatial audio (future feature)
    pub position: Option<glam::Vec3>,
    
    /// Category this audio source belongs to
    pub category: AudioCategory,
}

/// Audio categories for volume and control grouping
#[derive(Debug, Clone, PartialEq)]
pub enum AudioCategory {
    SoundEffect,
    Music,
    UserInterface,
    Voice,
}

impl AudioSource {
    /// Create a new audio source with default settings
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_string_lossy().to_string(),
            volume: 1.0,
            looped: false,
            position: None,
            category: AudioCategory::SoundEffect,
        }
    }
    
    /// Create a UI sound effect source
    pub fn ui_sound<P: AsRef<Path>>(path: P, volume: f32) -> Self {
        Self {
            path: path.as_ref().to_string_lossy().to_string(),
            volume,
            looped: false,
            position: None,
            category: AudioCategory::UserInterface,
        }
    }
    
    /// Create a background music source
    pub fn background_music<P: AsRef<Path>>(path: P, volume: f32) -> Self {
        Self {
            path: path.as_ref().to_string_lossy().to_string(),
            volume,
            looped: true,
            position: None,
            category: AudioCategory::Music,
        }
    }
    
    /// Set the volume for this audio source
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume.clamp(0.0, 1.0);
        self
    }
    
    /// Set whether this audio source should loop
    pub fn with_looping(mut self, looped: bool) -> Self {
        self.looped = looped;
        self
    }
    
    /// Set 3D position for spatial audio (future feature)
    pub fn with_position(mut self, position: glam::Vec3) -> Self {
        self.position = Some(position);
        self
    }
    
    /// Set the audio category
    pub fn with_category(mut self, category: AudioCategory) -> Self {
        self.category = category;
        self
    }
    
    /// Validate that the audio file exists and is readable
    pub fn validate(&self) -> AudioResult<()> {
        if !Path::new(&self.path).exists() {
            return Err(AudioError::FileNotFound(self.path.clone()));
        }
        
        // Check if file extension is supported
        let path = Path::new(&self.path);
        if let Some(extension) = path.extension() {
            match extension.to_str() {
                Some("wav") | Some("ogg") | Some("mp3") | Some("flac") => Ok(()),
                Some(ext) => Err(AudioError::UnsupportedFormat(ext.to_string())),
                None => Err(AudioError::UnsupportedFormat("unknown".to_string())),
            }
        } else {
            Err(AudioError::UnsupportedFormat("no extension".to_string()))
        }
    }
}