use crate::events::Event;
use std::any::Any;
use std::path::PathBuf;

/// UI interaction and state events
#[derive(Clone, Debug)]
pub enum UiEvent {
    /// Menu shown
    MenuShown { name: String },

    /// Menu hidden
    MenuHidden { name: String },

    /// Overlay toggled
    OverlayToggled { name: String, visible: bool },

    /// Settings saved
    SettingsSaved,

    /// Scene load requested
    LoadSceneRequested { path: PathBuf },

    /// New world generation requested
    NewWorldRequested {
        name: String,
        seed: Option<u64>,
        size: u32,
    },

    /// Exit requested
    ExitRequested,
}

impl Event for UiEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
