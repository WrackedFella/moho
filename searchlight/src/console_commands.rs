//! The FPS's console commands. They are hidden debug commands: they appear only in the console
//! and its `help`, never in a Settings screen. Every registration lives in this module.

use std::collections::BTreeMap;
use std::convert::Infallible;

use moho_input::bindings::BindingWarning;
use moho_ui_shell::Console;

/// The FPS console with the game's commands registered. It has none yet, so only the engine's
/// `help`, `clear` and `quit` run. Reads the `console` key from the game's `[bindings]` section.
pub fn console(bindings: &BTreeMap<String, String>) -> (Console<Infallible>, Vec<BindingWarning>) {
    Console::new(bindings)
}
