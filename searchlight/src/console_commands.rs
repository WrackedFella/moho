//! The FPS's console commands. They are hidden debug commands: they appear only in the console
//! and its `help`, never in a Settings screen. Every registration lives in this module.

use std::collections::BTreeMap;

use moho_input::bindings::BindingWarning;
use moho_ui_shell::Console;
use moho_ui_shell::console::registry::CommandSpec;
use searchlight_rules::{Grade, GravityScale};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConsoleCommand {
    GodMode(GodModeArg),
    Fullscreen,
    /// Force the grade of new enemies, or with `None` clear the force.
    ForceGrade(Option<Grade>),
    Simple(SimpleArg),
    /// Set how hard gravity pulls on rounds.
    RoundGravity(GravityScale),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GodModeArg {
    Toggle,
    Set(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimpleArg {
    Toggle,
    Set(bool),
}

/// Parses `simple [0|1]`; no argument toggles.
pub fn parse_simple(_args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    todo!("parse_simple")
}

/// Parses `round_gravity <scale>`.
pub fn parse_round_gravity(_args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    todo!("parse_round_gravity")
}

/// Parses `tgm [0|1]`; no argument toggles.
pub fn parse_tgm(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    match args {
        [] => Ok((ConsoleCommand::GodMode(GodModeArg::Toggle), String::new())),
        ["1"] => Ok((
            ConsoleCommand::GodMode(GodModeArg::Set(true)),
            "God mode: on".to_owned(),
        )),
        ["0"] => Ok((
            ConsoleCommand::GodMode(GodModeArg::Set(false)),
            "God mode: off".to_owned(),
        )),
        _ => Err("Usage: tgm [0|1]".to_owned()),
    }
}

/// Parses `fullscreen`; it takes no arguments.
pub fn parse_fullscreen(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    match args {
        [] => Ok((ConsoleCommand::Fullscreen, String::new())),
        _ => Err("Usage: fullscreen".to_owned()),
    }
}

/// The `grade` usage line, listing every grade in ladder order.
fn grade_usage() -> String {
    let names: Vec<&str> = Grade::LADDER.into_iter().map(Grade::name).collect();
    format!("grade <{}|off>", names.join("|"))
}

/// Parses `grade <name>` and `grade off`; anything else prints the usage.
pub fn parse_grade(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    match args {
        ["off"] => Ok((
            ConsoleCommand::ForceGrade(None),
            "Grade force cleared".to_owned(),
        )),
        [name] => name
            .parse::<Grade>()
            .map(|grade| {
                (
                    ConsoleCommand::ForceGrade(Some(grade)),
                    format!("Grade forced: {}", grade.name()),
                )
            })
            .map_err(|_| format!("Usage: {}", grade_usage())),
        _ => Err(format!("Usage: {}", grade_usage())),
    }
}

/// The FPS console with the game's commands registered. Reads the `console` key from the
/// game's `[bindings]` section.
pub fn console(
    bindings: &BTreeMap<String, String>,
) -> (Console<ConsoleCommand>, Vec<BindingWarning>) {
    let (mut console, warnings) = Console::new(bindings);
    console
        .register(CommandSpec {
            name: "tgm".to_owned(),
            usage: "tgm [0|1]".to_owned(),
            help: "toggle god mode".to_owned(),
            parse: parse_tgm,
        })
        .expect("\"tgm\" is not an engine command and is registered once");
    console
        .register(CommandSpec {
            name: "fullscreen".to_owned(),
            usage: "fullscreen".to_owned(),
            help: "toggle fullscreen".to_owned(),
            parse: parse_fullscreen,
        })
        .expect("\"fullscreen\" is not an engine command and is registered once");
    console
        .register(CommandSpec {
            name: "grade".to_owned(),
            usage: grade_usage(),
            help: "force the grade of new enemies".to_owned(),
            parse: parse_grade,
        })
        .expect("\"grade\" is not an engine command and is registered once");
    console
        .register(CommandSpec {
            name: "simple".to_owned(),
            usage: "simple [0|1]".to_owned(),
            help: "toggle hitscan shots".to_owned(),
            parse: parse_simple,
        })
        .expect("\"simple\" is not an engine command and is registered once");
    console
        .register(CommandSpec {
            name: "round_gravity".to_owned(),
            usage: "round_gravity <scale>".to_owned(),
            help: "set how hard gravity pulls on rounds".to_owned(),
            parse: parse_round_gravity,
        })
        .expect("\"round_gravity\" is not an engine command and is registered once");
    (console, warnings)
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

    #[test]
    fn fullscreen_takes_no_arguments_and_rejects_extra_ones() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("fullscreen");
        let queued = console.take_commands();
        console.submit("fullscreen now");
        let refused = (console.take_commands(), last_line(&console));

        assert_eq!(queued, vec![ConsoleCommand::Fullscreen]);
        assert_eq!(refused, (vec![], "Usage: fullscreen".to_owned()));
    }

    const GRADE_USAGE: &str = "grade <rookie|novice|experienced|veteran|professional|off>";

    #[test]
    fn grade_with_a_name_queues_the_force_and_prints_it() {
        let (mut console, _) = console(&BTreeMap::new());

        for grade in Grade::LADDER {
            console.submit(&format!("grade {}", grade.name()));

            assert_eq!(
                console.take_commands(),
                vec![ConsoleCommand::ForceGrade(Some(grade))],
                "{grade:?}"
            );
            assert_eq!(
                last_line(&console),
                format!("Grade forced: {}", grade.name())
            );
        }
    }

    #[test]
    fn grade_off_queues_a_clear() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("grade off");

        assert_eq!(
            console.take_commands(),
            vec![ConsoleCommand::ForceGrade(None)]
        );
        assert_eq!(last_line(&console), "Grade force cleared");
    }

    #[test]
    fn grade_refuses_an_unknown_name_listing_the_five_grades() {
        for line in [
            "grade legend",
            "grade",
            "grade rookie novice",
            "grade Rookie",
        ] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert!(console.take_commands().is_empty(), "{line}");
            let reply = last_line(&console);
            assert!(reply.contains(GRADE_USAGE), "{line}: {reply:?}");
            for grade in Grade::LADDER {
                assert!(reply.contains(grade.name()), "{line}: {reply:?}");
            }
            assert!(!reply.starts_with("Grade"), "{line}: {reply:?}");
        }
    }

    #[test]
    fn help_lists_grade() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("help");

        let lines = printed(&console);
        let expected = format!("  {GRADE_USAGE} - force the grade of new enemies");
        assert!(lines.contains(&expected), "{lines:?}");
    }

    #[test]
    fn simple_toggles_and_sets() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("simple");
        let toggled = console.take_commands();
        console.submit("simple 1");
        let on = (console.take_commands(), last_line(&console));
        console.submit("simple 0");
        let off = (console.take_commands(), last_line(&console));

        assert_eq!(toggled, vec![ConsoleCommand::Simple(SimpleArg::Toggle)]);
        assert_eq!(
            on,
            (
                vec![ConsoleCommand::Simple(SimpleArg::Set(true))],
                "Simple: on".to_owned()
            )
        );
        assert_eq!(
            off,
            (
                vec![ConsoleCommand::Simple(SimpleArg::Set(false))],
                "Simple: off".to_owned()
            )
        );
    }

    #[test]
    fn simple_refuses_an_unusable_argument_and_queues_nothing() {
        for line in ["simple 2", "simple on", "simple 1 0", "simple -1"] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert!(console.take_commands().is_empty(), "{line}");
            let reply = last_line(&console);
            assert!(reply.contains("simple [0|1]"), "{line}: {reply:?}");
        }
    }

    #[test]
    fn round_gravity_sets_a_valid_scale() {
        for (line, scale) in [
            ("round_gravity 30", 30.0),
            ("round_gravity 0", 0.0),
            ("round_gravity 0.5", 0.5),
            ("round_gravity 1000", 1000.0),
        ] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert_eq!(
                console.take_commands(),
                vec![ConsoleCommand::RoundGravity(
                    GravityScale::new(scale).unwrap()
                )],
                "{line}"
            );
            assert!(last_line(&console).contains("Round gravity"), "{line}");
        }
    }

    #[test]
    fn round_gravity_refuses_negative_and_non_numeric_values() {
        for line in [
            "round_gravity",
            "round_gravity -1",
            "round_gravity heavy",
            "round_gravity nan",
            "round_gravity inf",
            "round_gravity 1000.5",
            "round_gravity 1 2",
        ] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert!(console.take_commands().is_empty(), "{line}");
            let reply = last_line(&console);
            assert!(reply.contains("round_gravity <scale>"), "{line}: {reply:?}");
            assert!(!reply.starts_with("Round gravity"), "{line}: {reply:?}");
        }
    }

    #[test]
    fn help_lists_fullscreen() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("help");

        let lines = printed(&console);
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("  fullscreen - toggle")),
            "{lines:?}"
        );
    }
}
