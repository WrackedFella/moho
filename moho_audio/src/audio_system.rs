use crate::audio_cache::AudioCache;
use crate::audio_events::AudioEvent;
use crate::audio_settings::AudioSettings;
use crate::audio_source::{AudioCategory, AudioSource};
use crate::error::{AudioError, AudioResult};
use log::{debug, warn};
use rodio::Source;
use std::path::Path;

/// Main audio system for the Moho engine.
///
/// AudioSystem manages audio playback, asset loading, and provides
/// a clean interface for the rest of the engine to play sounds.
pub struct AudioSystem {
    /// Rodio device sink (keeps the audio stream alive)
    _device: rodio::MixerDeviceSink,

    /// Current audio settings
    settings: AudioSettings,

    /// Audio cache manager (handles loading and caching)
    audio_cache: AudioCache,

    /// Currently playing background music player (for volume control)
    music_player: Option<rodio::Player>,
}

impl AudioSystem {
    /// Create a new audio system with optimized settings for low latency
    pub fn new() -> AudioResult<Self> {
        let device = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| AudioError::InitializationFailed(e.to_string()))?;

        debug!("Audio system initialized successfully");

        Ok(Self {
            _device: device,
            settings: AudioSettings::default(),
            audio_cache: AudioCache::new()?,
            music_player: None,
        })
    }

    /// Get current audio settings
    pub fn settings(&self) -> &AudioSettings {
        &self.settings
    }

    /// Update audio settings
    pub fn set_settings(&mut self, settings: AudioSettings) {
        self.settings = settings;

        // Update music volume if currently playing
        if let Some(player) = &self.music_player {
            let music_volume = self.settings.effective_volume(&AudioCategory::Music);
            player.set_volume(music_volume);
        }
    }

    /// Play a sound from an AudioSource with optimized loading
    pub fn play_audio_source(&mut self, source: &AudioSource) -> AudioResult<()> {
        let effective_volume = self.settings.effective_volume(source.category()) * source.volume();

        if effective_volume <= 0.0 {
            debug!(
                "Skipping audio playback due to zero volume: {}",
                source.path()
            );
            return Ok(());
        }

        // Use pre-loaded data for UI sounds, or load on-demand for others
        let audio_data = if *source.category() == AudioCategory::UserInterface {
            self.audio_cache.get_ui_sound(source.path())?
        } else {
            self.audio_cache.get_audio(source.path())?
        };

        let cursor = std::io::Cursor::new(audio_data);
        let source_decoder = rodio::Decoder::new(cursor).map_err(|e| {
            AudioError::LoadFailed(format!("Failed to decode {}: {}", source.path(), e))
        })?;

        let player = rodio::Player::connect_new(self._device.mixer());
        player.set_volume(effective_volume);

        if source.looped() {
            player.append(source_decoder.repeat_infinite());
        } else {
            player.append(source_decoder);
        }

        // For background music, store the player for volume control
        if *source.category() == AudioCategory::Music && source.looped() {
            if let Some(old_player) = self.music_player.take() {
                old_player.stop();
            }
            player.play();
            self.music_player = Some(player);
        } else {
            player.play();
            player.detach(); // Let it play and clean up automatically
        }

        debug!(
            "Playing audio: {} at volume {:.2}",
            source.path(),
            effective_volume
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
            AudioEvent::PlaySound { path, volume } => self.play_sound(&path, volume),
            AudioEvent::MusicStart {
                path,
                volume,
                looped,
            } => {
                let source = AudioSource::background_music(&path, volume).with_looping(looped);
                self.play_audio_source(&source)
            }
            AudioEvent::MusicStop => {
                self.stop_audio(Some(AudioCategory::Music));
                Ok(())
            }
            AudioEvent::MusicVolumeChanged { volume: _ } => {
                // TODO: adjust the active music player's volume in place once
                // AudioSystem exposes a live volume control for music_player.
                Ok(())
            }
            AudioEvent::StopAll => {
                self.stop_audio(None);
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

        // Get pre-loaded audio data (delegate to AudioCache)
        let audio_data = self.audio_cache.get_ui_sound(&path_str)?;

        // Create a new cursor and decoder for this playback
        let cursor = std::io::Cursor::new(audio_data);
        let source_decoder = rodio::Decoder::new(cursor)
            .map_err(|e| AudioError::LoadFailed(format!("Failed to decode {}: {}", path_str, e)))?;

        // For UI sounds, create a new player each time for immediate playback
        // This avoids queueing delays that would occur with a shared player
        let player = rodio::Player::connect_new(self._device.mixer());
        player.set_volume(effective_volume);
        player.append(source_decoder);
        player.play();
        player.detach(); // Let it play and clean up automatically

        debug!(
            "Playing UI audio: {} at volume {:.2}",
            path_str, effective_volume
        );
        Ok(())
    }

    /// Stop audio based on category
    pub fn stop_audio(&mut self, category: Option<AudioCategory>) {
        match category {
            None => {
                if let Some(player) = self.music_player.take() {
                    player.stop();
                }
                // Note: Individual sound effects can't be stopped once detached
            }
            Some(AudioCategory::Music) => {
                if let Some(player) = self.music_player.take() {
                    player.stop();
                }
            }
            Some(
                AudioCategory::SoundEffect | AudioCategory::UserInterface | AudioCategory::Voice,
            ) => {
                // Individual sound effects auto-cleanup when finished
                warn!("Cannot stop individual sound effects once started");
            }
        }
    }

    /// Clear the audio cache to free memory
    pub fn clear_cache(&mut self) {
        self.audio_cache.clear_audio_cache();
    }

    /// Get the number of cached audio files
    pub fn cache_size(&self) -> usize {
        self.audio_cache.audio_cache_size()
    }
}

impl Drop for AudioSystem {
    fn drop(&mut self) {
        if let Some(player) = self.music_player.take() {
            player.stop();
        }
        debug!("Audio system shutdown");
    }
}
