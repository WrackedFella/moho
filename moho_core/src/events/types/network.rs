use crate::events::Event;
use std::any::Any;

/// Network/multiplayer events (future implementation)
#[derive(Clone, Debug)]
pub enum NetworkEvent {
    /// Player connected
    PlayerJoined { player_id: u64, name: String },

    /// Player disconnected
    PlayerLeft { player_id: u64, reason: String },

    /// State synchronization
    StateSync { frame: u64, checksum: u64 },

    /// Network error
    NetworkError { error: String },
}

impl Event for NetworkEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
