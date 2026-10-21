//! The engine's UI shell: the developer console every game opens and adds commands to, and the
//! egui host that draws it. The only engine crate that names egui.
//!
//! Console commands are hidden debug commands: they appear only in the console and its `help`,
//! never in a Settings screen.

pub mod apply;
pub mod console;
pub mod host;
pub mod layers;
pub mod paint_map;
pub mod text;

pub use apply::apply_engine_command;
pub use console::registry::{CommandSpec, EngineCommand, Owner, RegisterError};
pub use console::{CONSOLE_BINDING, Console, Routing};
pub use host::UiShell;
pub use layers::UiLayers;
pub use text::{Anchor, ScreenText};
