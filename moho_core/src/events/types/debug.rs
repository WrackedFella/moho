use crate::events::Event;
use std::any::Any;

/// Debug console and development events
#[derive(Clone, Debug)]
pub enum DebugEvent {
    /// Console command entered
    ConsoleCommand { command: String, args: Vec<String> },

    /// Console output
    ConsoleOutput {
        message: String,
        level: ConsoleLevel,
    },

    /// Toggle collision detection
    ToggleCollision { enabled: bool },

    /// Toggle god mode
    ToggleGodMode { enabled: bool },

    /// Spawn entity command
    SpawnEntity {
        entity_type: String,
        position: Option<glam::Vec3>,
    },

    /// Teleport player
    TeleportPlayer { position: glam::Vec3 },
}

#[derive(Clone, Debug)]
pub enum ConsoleLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl Event for DebugEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
