use moho_core::events::Event;
use std::any::Any;
use std::path::PathBuf;

/// Core gameplay events
#[derive(Clone, Debug)]
pub enum GameEvent {
    /// Scene loaded successfully
    SceneLoaded { path: PathBuf },

    /// Scene load failed
    SceneLoadFailed { path: PathBuf, error: String },

    /// Save completed
    SaveCompleted {
        path: PathBuf,
        success: bool,
        message: String,
    },

    /// Player spawned
    PlayerSpawned { position: glam::Vec3 },

    /// Player damaged
    PlayerDamaged { amount: u32, source: String },

    /// Player health changed
    PlayerHealthChanged { old_health: u32, new_health: u32 },

    /// Entity spawned
    EntitySpawned {
        entity_id: u64,
        entity_type: String,
        position: glam::Vec3,
    },

    /// Entity destroyed
    EntityDestroyed { entity_id: u64 },
}

impl Event for GameEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
