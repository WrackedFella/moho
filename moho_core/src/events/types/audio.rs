use crate::events::Event;
use std::any::Any;

/// Audio playback events
#[derive(Clone, Debug)]
pub enum AudioEvent {
    /// Play UI sound effect
    ButtonClick,
    MenuNavigate,
    Confirm,
    Cancel,
    Error,

    /// Play custom sound
    PlaySound { path: String, volume: f32 },

    /// Background music control
    MusicStart {
        path: String,
        volume: f32,
        looped: bool,
    },

    MusicStop,

    MusicVolumeChanged { volume: f32 },

    /// Stop all audio or category
    StopAll,
}

impl Event for AudioEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
