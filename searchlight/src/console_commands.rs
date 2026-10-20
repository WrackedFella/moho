//! The FPS's console commands. They are hidden debug commands: they appear only in the console
//! and its `help`, never in a Settings screen. Every registration lives in this module.

use std::collections::BTreeMap;

use moho_input::bindings::BindingWarning;
use moho_ui_shell::Console;

/// What a console command asks of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleCommand {
    GodMode(GodModeArg),
}

/// What `tgm` asks of god mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GodModeArg {
    Toggle,
    Set(bool),
}

/// The parse function registered as `tgm`; usage `tgm [0|1]`.
pub fn parse_tgm(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    let _ = args;
    todo!("parse tgm")
}

/// The FPS console with the game's commands registered. Reads the `console` key from the
/// game's `[bindings]` section.
pub fn console(
    bindings: &BTreeMap<String, String>,
) -> (Console<ConsoleCommand>, Vec<BindingWarning>) {
    Console::new(bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn last_line(console: &Console<ConsoleCommand>) -> String {
        console
            .output()
            .lines()
            .back()
            .cloned()
            .expect("the console printed something")
    }

    fn printed(console: &Console<ConsoleCommand>) -> Vec<String> {
        console.output().lines().iter().cloned().collect()
    }

    #[test]
    fn bare_tgm_queues_a_toggle_and_prints_nothing_itself() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("tgm");

        assert_eq!(
            console.take_commands(),
            vec![ConsoleCommand::GodMode(GodModeArg::Toggle)]
        );
        assert_eq!(last_line(&console), "> tgm");
    }

    #[test]
    fn tgm_with_0_or_1_queues_a_set_and_prints_the_value() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("tgm 1");
        let on = (console.take_commands(), last_line(&console));
        console.submit("tgm 0");
        let off = (console.take_commands(), last_line(&console));

        assert_eq!(
            on,
            (
                vec![ConsoleCommand::GodMode(GodModeArg::Set(true))],
                "God mode: on".to_owned()
            )
        );
        assert_eq!(
            off,
            (
                vec![ConsoleCommand::GodMode(GodModeArg::Set(false))],
                "God mode: off".to_owned()
            )
        );
    }

    #[test]
    fn tgm_refuses_an_unusable_argument_and_queues_nothing() {
        for line in ["tgm 2", "tgm on", "tgm 1 0", "tgm 01", "tgm -1"] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert!(console.take_commands().is_empty(), "{line}");
            let reply = last_line(&console);
            assert!(reply.contains("tgm [0|1]"), "{line}: {reply:?}");
            assert!(!reply.starts_with("God mode"), "{line}: {reply:?}");
        }
    }

    #[test]
    fn help_lists_tgm() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("help");

        let lines = printed(&console);
        assert!(
            lines.iter().any(|line| line.starts_with("  tgm [0|1] - ")),
            "{lines:?}"
        );
    }
}
