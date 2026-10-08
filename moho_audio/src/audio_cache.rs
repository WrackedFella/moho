//! Audio Caching System
//!
//! Manages loading and caching of audio files for performance optimization.
//! Pre-loads UI sounds for low latency and caches other audio on-demand.
use crate::error::{AudioError, AudioResult};
use std::collections::HashMap;
use tracing::debug;

/// Audio cache manager for performance-optimized audio loading.
///
/// This module handles two types of caching:
/// - **UI Sound Cache**: Pre-loaded for ultra-low latency (<1ms access)
/// - **General Audio Cache**: On-demand loading with caching for reuse
#[derive(Debug)]
pub struct AudioCache {
    /// General audio file cache (loaded on-demand)
    audio_cache: HashMap<String, Vec<u8>>,

    /// Pre-loaded UI sounds for low-latency playback
    ui_sound_cache: HashMap<String, Vec<u8>>,
}

impl AudioCache {
    /// Create a new audio cache and pre-load common UI sounds
    pub fn new() -> AudioResult<Self> {
        let mut cache = Self {
            audio_cache: HashMap::new(),
            ui_sound_cache: HashMap::new(),
        };

        cache.preload_ui_sounds()?;
        Ok(cache)
    }

    /// Pre-load common UI sounds to eliminate loading delays
    fn preload_ui_sounds(&mut self) -> AudioResult<()> {
        let ui_sounds = vec!["assets/audio/ui/button_click.mp3"];

        for sound_path in ui_sounds {
            if let Ok(audio_data) = std::fs::read(sound_path) {
                self.ui_sound_cache
                    .insert(sound_path.to_string(), audio_data);
                debug!(path = sound_path, "Pre-loaded UI sound");
            } else {
                // Don't fail initialization if UI sounds are missing
                debug!(
                    path = sound_path,
                    "UI sound file not found (will load on-demand)"
                );
            }
        }

        Ok(())
    }

    /// Get UI sound data with low-latency access.
    ///
    /// First checks pre-loaded cache, then loads and caches if not found.
    /// Subsequent accesses will use cached data for <1ms latency.
    pub fn get_ui_sound(&mut self, path: &str) -> AudioResult<Vec<u8>> {
        // First check the UI sound cache
        if let Some(cached_data) = self.ui_sound_cache.get(path) {
            return Ok(cached_data.clone());
        }

        // Fall back to regular loading and cache in UI cache for next time
        let audio_data =
            std::fs::read(path).map_err(|e| AudioError::FileNotFound(format!("{path}: {e}")))?;

        // Cache in UI cache for future low-latency access
        self.ui_sound_cache
            .insert(path.to_string(), audio_data.clone());

        debug!(path = path, "Loaded and cached UI sound");
        Ok(audio_data)
    }

    /// Load audio file data, using cache if available.
    ///
    /// First checks cache, then loads from filesystem if needed.
    /// Automatically caches newly loaded audio for future use.
    pub fn get_audio(&mut self, path: &str) -> AudioResult<Vec<u8>> {
        // Check cache first
        if let Some(cached_data) = self.audio_cache.get(path) {
            return Ok(cached_data.clone());
        }

        // Load from file
        let audio_data =
            std::fs::read(path).map_err(|e| AudioError::FileNotFound(format!("{path}: {e}")))?;

        // Cache for future use
        self.audio_cache
            .insert(path.to_string(), audio_data.clone());

        debug!(path = path, "Loaded and cached audio file");
        Ok(audio_data)
    }
}

impl Default for AudioCache {
    /// Creates an empty cache without pre-loading sounds.
    fn default() -> Self {
        Self {
            audio_cache: HashMap::new(),
            ui_sound_cache: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("create temp file");
        file.write_all(bytes).expect("write temp file");
        file
    }

    #[test]
    fn cached_audio_survives_file_deletion() {
        let mut cache = AudioCache::default();
        let file = temp_file(b"audio-bytes");
        let path = file.path().to_str().expect("utf-8 path").to_owned();
        assert_eq!(cache.get_audio(&path).unwrap(), b"audio-bytes");

        file.close().expect("delete temp file");

        assert_eq!(cache.get_audio(&path).unwrap(), b"audio-bytes");
    }

    #[test]
    fn cached_ui_sound_survives_file_deletion() {
        let mut cache = AudioCache::default();
        let file = temp_file(b"ui-bytes");
        let path = file.path().to_str().expect("utf-8 path").to_owned();
        assert_eq!(cache.get_ui_sound(&path).unwrap(), b"ui-bytes");

        file.close().expect("delete temp file");

        assert_eq!(cache.get_ui_sound(&path).unwrap(), b"ui-bytes");
    }

    #[test]
    fn missing_file_returns_file_not_found() {
        let mut cache = AudioCache::default();
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("absent.mp3");
        let path = missing.to_str().expect("utf-8 path");

        let audio = cache.get_audio(path);
        let ui = cache.get_ui_sound(path);

        assert!(
            matches!(audio, Err(AudioError::FileNotFound(_))),
            "{audio:?}"
        );
        assert!(matches!(ui, Err(AudioError::FileNotFound(_))), "{ui:?}");
    }
}
