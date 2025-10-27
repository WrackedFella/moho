use crate::audio_events::{AudioCategory as EventCategory, AudioEvent};
use crate::audio_settings::AudioSettings;
use crate::audio_source::{AudioCategory, AudioSource};
use crate::error::{AudioError, AudioResult};
use log::{debug, warn};
use rodio::Source;
use std::collections::HashMap;
use std::path::Path;

/// Main audio system for the Moho engine.
///
/// AudioSystem manages audio playback, asset loading, and provides
/// a clean interface for the rest of the engine to play sounds.
pub struct AudioSystem {
    /// Rodio output stream for audio playback
    _stream: rodio::OutputStream,

    /// Handle to control the audio stream
    stream_handle: rodio::OutputStreamHandle,

    /// Current audio settings
    settings: AudioSettings,

    /// Cache of loaded audio sources for performance
    audio_cache: HashMap<String, Vec<u8>>,

    /// Pre-loaded decoders for low-latency UI sounds
    ui_sound_cache: HashMap<String, Vec<u8>>,

    /// Currently playing background music sink (for control)
    music_sink: Option<rodio::Sink>,
}

impl AudioSystem {
    /// Create a new audio system with optimized settings for low latency
    pub fn new() -> AudioResult<Self> {
        // Initialize rodio output stream
        let (stream, stream_handle) = rodio::OutputStream::try_default()
            .map_err(|e| AudioError::InitializationFailed(e.to_string()))?;

        debug!("Audio system initialized successfully");

        let mut audio_system = Self {
            _stream: stream,
            stream_handle,
            settings: AudioSettings::default(),
            audio_cache: HashMap::new(),
            ui_sound_cache: HashMap::new(),
            music_sink: None,
        };

        // Pre-load common UI sounds for low latency
        audio_system.preload_ui_sounds()?;

        Ok(audio_system)
    }

    /// Pre-load common UI sounds to eliminate loading delays
    fn preload_ui_sounds(&mut self) -> AudioResult<()> {
        let ui_sounds = vec!["assets/audio/ui/button_click.mp3"];

        for sound_path in ui_sounds {
            if let Ok(audio_data) = std::fs::read(sound_path) {
                self.ui_sound_cache
                    .insert(sound_path.to_string(), audio_data);
                debug!("Pre-loaded UI sound: {}", sound_path);
            } else {
                // Don't fail initialization if UI sounds are missing
                debug!(
                    "UI sound file not found (will load on-demand): {}",
                    sound_path
                );
            }
        }

        Ok(())
    }

    /// Get current audio settings
    pub fn settings(&self) -> &AudioSettings {
        &self.settings
    }

    /// Update audio settings
    pub fn set_settings(&mut self, settings: AudioSettings) {
        self.settings = settings;

        // Update music volume if currently playing
        if let Some(sink) = &self.music_sink {
            let music_volume = self.settings.effective_volume(&AudioCategory::Music);
            sink.set_volume(music_volume);
        }
    }

    /// Play a sound from an AudioSource with optimized loading
    pub fn play_audio_source(&mut self, source: &AudioSource) -> AudioResult<()> {
        let effective_volume = self.settings.effective_volume(&source.category) * source.volume;

        if effective_volume <= 0.0 {
            debug!(
                "Skipping audio playback due to zero volume: {}",
                source.path
            );
            return Ok(());
        }

        // Use pre-loaded data for UI sounds, or load on-demand for others
        let audio_data = if source.category == AudioCategory::UserInterface {
            self.get_ui_sound_data(&source.path)?
        } else {
            self.load_audio_file(&source.path)?
        };

        let cursor = std::io::Cursor::new(audio_data);
        let source_decoder = rodio::Decoder::new(cursor).map_err(|e| {
            AudioError::LoadFailed(format!("Failed to decode {}: {}", source.path, e))
        })?;

        let sink = rodio::Sink::try_new(&self.stream_handle)
            .map_err(|e| AudioError::PlaybackFailed(e.to_string()))?;

        sink.set_volume(effective_volume);

        if source.looped {
            sink.append(source_decoder.repeat_infinite());
        } else {
            sink.append(source_decoder);
        }

        // For background music, store the sink for volume control
        if source.category == AudioCategory::Music && source.looped {
            if let Some(old_sink) = self.music_sink.take() {
                old_sink.stop();
            }
            sink.play();
            self.music_sink = Some(sink);
        } else {
            sink.play();
            sink.detach(); // Let it play and clean up automatically
        }

        debug!(
            "Playing audio: {} at volume {:.2}",
            source.path, effective_volume
        );
        Ok(())
    }

    /// Play a sound file with specified volume
    pub fn play_sound<P: AsRef<Path>>(&mut self, path: P, volume: f32) -> AudioResult<()> {
        let source = AudioSource::new(path).with_volume(volume);
        self.play_audio_source(&source)
    }

    /// Handle audio events from the UI or game
    pub fn handle_event(&mut self, event: AudioEvent) -> AudioResult<()> {
        match event {
            AudioEvent::ButtonClick => {
                // Use the button_click.mp3 from assets/audio/ui directory
                self.play_ui_sound("assets/audio/ui/button_click.mp3", 0.8)
            }
            AudioEvent::MenuNavigate => self.play_ui_sound("assets/audio/ui/button_click.mp3", 0.6),
            AudioEvent::Confirm => self.play_ui_sound("assets/audio/ui/button_click.mp3", 0.8),
            AudioEvent::Cancel => self.play_ui_sound("assets/audio/ui/button_click.mp3", 0.7),
            AudioEvent::Error => self.play_ui_sound("assets/audio/ui/button_click.mp3", 0.9),
            AudioEvent::CustomSound { path, volume } => self.play_sound(&path, volume),
            AudioEvent::BackgroundMusic {
                path,
                volume,
                looped,
            } => {
                let source = AudioSource::background_music(&path, volume).with_looping(looped);
                self.play_audio_source(&source)
            }
            AudioEvent::Stop(category) => {
                self.stop_audio(category);
                Ok(())
            }
        }
    }

    /// Play a UI sound effect with optimized low-latency playback
    fn play_ui_sound<P: AsRef<Path>>(&mut self, path: P, volume: f32) -> AudioResult<()> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let effective_volume = self
            .settings
            .effective_volume(&AudioCategory::UserInterface)
            * volume;

        if effective_volume <= 0.0 {
            debug!(
                "Skipping UI audio playback due to zero volume: {}",
                path_str
            );
            return Ok(());
        }

        // Get pre-loaded audio data
        let audio_data = self.get_ui_sound_data(&path_str)?;

        // Create a new cursor and decoder for this playback
        let cursor = std::io::Cursor::new(audio_data);
        let source_decoder = rodio::Decoder::new(cursor)
            .map_err(|e| AudioError::LoadFailed(format!("Failed to decode {}: {}", path_str, e)))?;

        // For UI sounds, create a new sink each time for immediate playback
        // This avoids queueing delays that would occur with a shared sink
        let sink = rodio::Sink::try_new(&self.stream_handle)
            .map_err(|e| AudioError::PlaybackFailed(e.to_string()))?;

        sink.set_volume(effective_volume);
        sink.append(source_decoder);
        sink.play();
        sink.detach(); // Let it play and clean up automatically

        debug!(
            "Playing UI audio: {} at volume {:.2}",
            path_str, effective_volume
        );
        Ok(())
    }

    /// Stop audio based on category
    pub fn stop_audio(&mut self, category: EventCategory) {
        match category {
            EventCategory::All => {
                if let Some(sink) = self.music_sink.take() {
                    sink.stop();
                }
                // Note: Individual sound effects can't be stopped once detached
                // This is a limitation we can address later if needed
            }
            EventCategory::Music => {
                if let Some(sink) = self.music_sink.take() {
                    sink.stop();
                }
            }
            EventCategory::SoundEffects | EventCategory::UserInterface => {
                // Individual sound effects auto-cleanup when finished
                warn!("Cannot stop individual sound effects once started");
            }
        }
    }

    /// Get pre-loaded UI sound data for low-latency playback
    fn get_ui_sound_data(&mut self, path: &str) -> AudioResult<Vec<u8>> {
        // First check the UI sound cache
        if let Some(cached_data) = self.ui_sound_cache.get(path) {
            return Ok(cached_data.clone());
        }

        // Fall back to regular loading and cache in UI cache for next time
        let audio_data = std::fs::read(path)
            .map_err(|e| AudioError::FileNotFound(format!("{}: {}", path, e)))?;

        // Cache in UI cache for future low-latency access
        self.ui_sound_cache
            .insert(path.to_string(), audio_data.clone());

        debug!("Loaded and cached UI sound: {}", path);
        Ok(audio_data)
    }

    /// Load audio file data, using cache if available
    fn load_audio_file(&mut self, path: &str) -> AudioResult<Vec<u8>> {
        // Check cache first
        if let Some(cached_data) = self.audio_cache.get(path) {
            return Ok(cached_data.clone());
        }

        // Load from file
        let audio_data = std::fs::read(path)
            .map_err(|e| AudioError::FileNotFound(format!("{}: {}", path, e)))?;

        // Cache for future use
        self.audio_cache
            .insert(path.to_string(), audio_data.clone());

        debug!("Loaded and cached audio file: {}", path);
        Ok(audio_data)
    }

    /// Clear the audio cache to free memory
    pub fn clear_cache(&mut self) {
        self.audio_cache.clear();
        debug!("Audio cache cleared");
    }

    /// Get the number of cached audio files
    pub fn cache_size(&self) -> usize {
        self.audio_cache.len()
    }
}

impl Drop for AudioSystem {
    fn drop(&mut self) {
        if let Some(sink) = self.music_sink.take() {
            sink.stop();
        }
        debug!("Audio system shutdown");
    }
}
