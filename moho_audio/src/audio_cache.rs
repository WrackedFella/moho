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
    /// Create a new audio cache and pre-load the UI click, when one is given.
    ///
    /// # Errors
    ///
    /// [`AudioError::FileNotFound`] naming the click when it cannot be read.
    pub fn new(ui_click: Option<&std::path::Path>) -> AudioResult<Self> {
        let mut cache = Self {
            audio_cache: HashMap::new(),
            ui_sound_cache: HashMap::new(),
        };

        if let Some(click) = ui_click {
            let key = click.to_string_lossy().into_owned();
            let bytes = std::fs::read(click)
                .map_err(|e| AudioError::FileNotFound(format!("{key}: {e}")))?;
            cache.ui_sound_cache.insert(key, bytes);
            debug!(path = %click.display(), "Pre-loaded UI click");
        }
        Ok(cache)
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

#[cfg(test)]
impl AudioCache {
    /// Cache holding exactly the given entries and never touching the filesystem for
    /// them. `general` backs [`Self::get_audio`], `ui` backs [`Self::get_ui_sound`].
    pub(crate) fn seeded(general: &[(&str, Vec<u8>)], ui: &[(&str, Vec<u8>)]) -> Self {
        let own = |entries: &[(&str, Vec<u8>)]| {
            entries
                .iter()
                .map(|(path, bytes)| ((*path).to_owned(), bytes.clone()))
                .collect()
        };
        Self {
            audio_cache: own(general),
            ui_sound_cache: own(ui),
        }
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

    #[test]
    fn preloads_only_the_given_click() {
        let click = temp_file(b"click-bytes");
        let given = AudioCache::new(Some(click.path())).expect("a readable click preloads");
        let none = AudioCache::new(None).expect("no click is allowed");
        let mut given = given;
        let mut none = none;

        let preloaded: Vec<&String> = given.ui_sound_cache.keys().collect();
        let click_path = click.path().to_str().expect("utf-8 path").to_owned();
        assert_eq!(preloaded, vec![&click_path]);
        assert!(none.ui_sound_cache.is_empty(), "{:?}", none.ui_sound_cache);
        assert!(none.audio_cache.is_empty());
        click.close().expect("delete temp file");
        assert_eq!(given.get_ui_sound(&click_path).unwrap(), b"click-bytes");
        assert!(none.get_ui_sound(&click_path).is_err());
    }

    #[test]
    fn an_unreadable_given_click_is_an_error_naming_it() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("absent.mp3");

        let cache = AudioCache::new(Some(&missing));

        match cache {
            Err(AudioError::FileNotFound(message)) => {
                assert!(
                    message.contains(&missing.display().to_string()),
                    "{message}"
                );
            }
            other => panic!("expected FileNotFound, got {other:?}"),
        }
    }
}
