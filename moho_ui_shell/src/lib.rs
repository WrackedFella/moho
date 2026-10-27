//! The engine's UI shell: the developer console every game opens and adds commands to, and the
//! egui host that draws it, along with the screen text a game sets each frame. The only engine
//! crate that names egui.
//!
//! Console commands are hidden debug commands: they appear only in the console and its `help`,
//! never in a Settings screen.

pub mod apply;
pub mod console;
pub mod host;
pub mod layers;
pub mod menu;
pub mod paint_map;
pub mod settings;
pub mod text;

pub use apply::{
    apply_engine_command, apply_engine_settings, audio_settings_for, runtime_window_mode,
};
pub use console::registry::{CommandSpec, EngineCommand, Owner, RegisterError};
pub use console::{CONSOLE_BINDING, Console, Routing};
pub use host::UiShell;
pub use layers::UiLayers;
pub use menu::{MenuEvent, MenuItem, MenuScreen, MenuStack};
pub use settings::{Control, EngineSetting, EngineSettings};
pub use text::{Anchor, ScreenText, ScreenTextError};
