//! Chunk streaming configuration.

/// Chunk streaming radius and budget parameters.
///
/// Persisted in `config/prefs.ini` under `[world]` so players can tune them
/// without recompiling. Loaded via `moho_ui::Prefs` → `AppConfig` → `ChunkStreamer`.
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// XZ Chebyshev radius (in chunks) within which chunks are kept loaded.
    pub load_radius_chunks: u32,
    /// XZ Chebyshev radius (in chunks) beyond which chunks are evicted.
    /// Must be >= `load_radius_chunks`; enforced at construction.
    pub unload_radius_chunks: u32,
    /// Maximum number of new XZ columns generated per frame.
    pub chunks_per_frame: u32,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            load_radius_chunks: 8,
            unload_radius_chunks: 12,
            chunks_per_frame: 4,
        }
    }
}
