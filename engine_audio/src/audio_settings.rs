/// Audio settings and configuration for the Moho engine.
/// 
/// Provides user-configurable audio options including volume levels
/// for different audio categories.

#[derive(Debug, Clone)]
pub struct AudioSettings {
    /// Master volume (0.0 to 1.0)
    pub master_volume: f32,
    
    /// Sound effects volume (0.0 to 1.0)
    pub sound_effects_volume: f32,
    
    /// Background music volume (0.0 to 1.0)
    pub music_volume: f32,
    
    /// UI sounds volume (0.0 to 1.0)
    pub ui_volume: f32,
    
    /// Voice/dialogue volume (0.0 to 1.0)
    pub voice_volume: f32,
    
    /// Whether audio is globally muted
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            sound_effects_volume: 0.8,
            music_volume: 0.6,
            ui_volume: 0.7,
            voice_volume: 0.9,
            muted: false,
        }
    }
}

impl AudioSettings {
    /// Create new audio settings with default values
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set master volume (affects all audio)
    pub fn with_master_volume(mut self, volume: f32) -> Self {
        self.master_volume = volume.clamp(0.0, 1.0);
        self
    }
    
    /// Set sound effects volume
    pub fn with_sound_effects_volume(mut self, volume: f32) -> Self {
        self.sound_effects_volume = volume.clamp(0.0, 1.0);
        self
    }
    
    /// Set background music volume
    pub fn with_music_volume(mut self, volume: f32) -> Self {
        self.music_volume = volume.clamp(0.0, 1.0);
        self
    }
    
    /// Set UI sounds volume
    pub fn with_ui_volume(mut self, volume: f32) -> Self {
        self.ui_volume = volume.clamp(0.0, 1.0);
        self
    }
    
    /// Set voice/dialogue volume
    pub fn with_voice_volume(mut self, volume: f32) -> Self {
        self.voice_volume = volume.clamp(0.0, 1.0);
        self
    }
    
    /// Mute or unmute all audio
    pub fn set_muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }
    
    /// Get effective volume for a specific audio category
    pub fn effective_volume(&self, category: &crate::audio_source::AudioCategory) -> f32 {
        if self.muted {
            return 0.0;
        }
        
        let category_volume = match category {
            crate::audio_source::AudioCategory::SoundEffect => self.sound_effects_volume,
            crate::audio_source::AudioCategory::Music => self.music_volume,
            crate::audio_source::AudioCategory::UserInterface => self.ui_volume,
            crate::audio_source::AudioCategory::Voice => self.voice_volume,
        };
        
        self.master_volume * category_volume
    }
}