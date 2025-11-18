///! Audio Caching System
///!
///! Manages loading and caching of audio files for performance optimization.
///! Pre-loads UI sounds for low latency and caches other audio on-demand.

use crate::error::{AudioError, AudioResult};
use log::debug;
use std::collections::HashMap;

/// Audio cache manager for performance-optimized audio loading.
///
/// This module handles two types of caching:
/// - **UI Sound Cache**: Pre-loaded for ultra-low latency (<1ms access)
/// - **General Audio Cache**: On-demand loading with caching for reuse
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
        let audio_data = std::fs::read(path)
            .map_err(|e| AudioError::FileNotFound(format!("{}: {}", path, e)))?;

        // Cache in UI cache for future low-latency access
        self.ui_sound_cache
            .insert(path.to_string(), audio_data.clone());

        debug!("Loaded and cached UI sound: {}", path);
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
        let audio_data = std::fs::read(path)
            .map_err(|e| AudioError::FileNotFound(format!("{}: {}", path, e)))?;

        // Cache for future use
        self.audio_cache
            .insert(path.to_string(), audio_data.clone());

        debug!("Loaded and cached audio file: {}", path);
        Ok(audio_data)
    }
    
    /// Clear all caches to free memory.
    pub fn clear(&mut self) {
        self.audio_cache.clear();
        self.ui_sound_cache.clear();
        debug!("Audio caches cleared");
    }
    
    /// Clear only the general audio cache (keeps UI sounds cached).
    pub fn clear_audio_cache(&mut self) {
        self.audio_cache.clear();
        debug!("Audio cache cleared (UI sounds retained)");
    }
    
    /// Get the number of cached audio files (excluding UI sounds).
    pub fn audio_cache_size(&self) -> usize {
        self.audio_cache.len()
    }
    
    /// Get the number of cached UI sounds.
    pub fn ui_cache_size(&self) -> usize {
        self.ui_sound_cache.len()
    }
    
    /// Get total number of cached items.
    pub fn total_cache_size(&self) -> usize {
        self.audio_cache.len() + self.ui_sound_cache.len()
    }
}

impl Default for AudioCache {
    fn default() -> Self {
        Self::new().expect("Failed to initialize audio cache")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_cache_creation() {
        let cache = AudioCache {
            audio_cache: HashMap::new(),
            ui_sound_cache: HashMap::new(),
        };
        assert_eq!(cache.audio_cache_size(), 0);
        assert_eq!(cache.ui_cache_size(), 0);
    }

    #[test]
    fn test_cache_size_tracking() {
        let mut cache = AudioCache {
            audio_cache: HashMap::new(),
            ui_sound_cache: HashMap::new(),
        };
        
        cache.audio_cache.insert("test1.mp3".to_string(), vec![1, 2, 3]);
        cache.audio_cache.insert("test2.mp3".to_string(), vec![4, 5, 6]);
        cache.ui_sound_cache.insert("ui_sound.mp3".to_string(), vec![7, 8, 9]);
        
        assert_eq!(cache.audio_cache_size(), 2);
        assert_eq!(cache.ui_cache_size(), 1);
        assert_eq!(cache.total_cache_size(), 3);
    }

    #[test]
    fn test_clear_audio_cache() {
        let mut cache = AudioCache {
            audio_cache: HashMap::new(),
            ui_sound_cache: HashMap::new(),
        };
        
        cache.audio_cache.insert("test.mp3".to_string(), vec![1, 2, 3]);
        cache.ui_sound_cache.insert("ui.mp3".to_string(), vec![4, 5, 6]);
        
        cache.clear_audio_cache();
        
        assert_eq!(cache.audio_cache_size(), 0);
        assert_eq!(cache.ui_cache_size(), 1); // UI cache retained
    }

    #[test]
    fn test_clear_all_caches() {
        let mut cache = AudioCache {
            audio_cache: HashMap::new(),
            ui_sound_cache: HashMap::new(),
        };
        
        cache.audio_cache.insert("test.mp3".to_string(), vec![1, 2, 3]);
        cache.ui_sound_cache.insert("ui.mp3".to_string(), vec![4, 5, 6]);
        
        cache.clear();
        
        assert_eq!(cache.audio_cache_size(), 0);
        assert_eq!(cache.ui_cache_size(), 0);
    }
}
