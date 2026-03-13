use crate::events::Event;
use std::any::Any;
use std::path::PathBuf;

/// Window display mode — mirrors `moho_ui::prefs::WindowMode` without a dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    Fullscreen,
    Borderless,
}

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

    /// Window/display settings changed (emitted after saving video prefs)
    WindowSettingsChanged { mode: WindowMode, width: u32, height: u32 },
}

impl Event for UiEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
