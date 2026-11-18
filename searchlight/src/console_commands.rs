//! The FPS's console commands. They are hidden debug commands: they appear only in the console
//! and its `help`, never in a Settings screen. Every registration lives in this module.

use std::collections::BTreeMap;

use moho_input::bindings::BindingWarning;
use moho_ui_shell::Console;
use moho_ui_shell::console::registry::{CommandSpec, ParseFn};
use searchlight_rules::{Grade, GravityScale, MAX_GRAVITY_SCALE};

use crate::arena::LevelMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConsoleCommand {
    GodMode(GodModeArg),
    Fullscreen,
    /// Force the grade of new enemies, or with `None` clear the force.
    ForceGrade(Option<Grade>),
    /// Switch Simple (hitscan for the player's shots) on, off or over.
    Simple(SimpleArg),
    /// Scale the gravity the player's rounds feel.
    RoundGravity(GravityScale),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimpleArg {
    Toggle,
    Set(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GodModeArg {
    Toggle,
    Set(bool),
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

/// The console line that confirms Simple is now `on`.
pub fn simple_reply(on: bool) -> &'static str {
    if on { "Simple: on" } else { "Simple: off" }
}

/// Parses `simple [0|1]`; no argument toggles.
pub fn parse_simple(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    match args {
        [] => Ok((ConsoleCommand::Simple(SimpleArg::Toggle), String::new())),
        ["1"] => Ok((
            ConsoleCommand::Simple(SimpleArg::Set(true)),
            simple_reply(true).to_owned(),
        )),
        ["0"] => Ok((
            ConsoleCommand::Simple(SimpleArg::Set(false)),
            simple_reply(false).to_owned(),
        )),
        _ => Err("Usage: simple [0|1]".to_owned()),
    }
}

/// Parses `round_gravity <scale>`; a missing, malformed or out-of-range scale prints the usage.
pub fn parse_round_gravity(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    let usage = || {
        format!("Usage: round_gravity <scale> (0 to {MAX_GRAVITY_SCALE}, 1 is standard gravity)")
    };
    let [text] = args else {
        return Err(usage());
    };
    let scale = text
        .parse::<f32>()
        .ok()
        .and_then(|value| GravityScale::new(value).ok())
        .ok_or_else(usage)?;
    Ok((
        ConsoleCommand::RoundGravity(scale),
        format!("Round gravity: {text}"),
    ))
}

/// The FPS console with the game's commands registered. Reads the `console` key from the
/// game's `[bindings]` section.
pub fn console(
    bindings: &BTreeMap<String, String>,
) -> (Console<ConsoleCommand>, Vec<BindingWarning>) {
    console_with_grade(bindings, parse_grade)
}

fn console_with_grade(
    bindings: &BTreeMap<String, String>,
    parse_grade: ParseFn<ConsoleCommand>,
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
            help: "toggle Simple: the player's shots hit on the tick they are fired".to_owned(),
            parse: parse_simple,
        })
        .expect("\"simple\" is not an engine command and is registered once");
    console
        .register(CommandSpec {
            name: "round_gravity".to_owned(),
            usage: "round_gravity <scale>".to_owned(),
            help: "scale the gravity the player's rounds feel".to_owned(),
            parse: parse_round_gravity,
        })
        .expect("\"round_gravity\" is not an engine command and is registered once");
    (console, warnings)
}

/// [`console`] for a level of `mode`: in a sandbox `grade` replies that there are no waves.
pub fn console_for(
    bindings: &BTreeMap<String, String>,
    mode: LevelMode,
) -> (Console<ConsoleCommand>, Vec<BindingWarning>) {
    let grade = match mode {
        LevelMode::Arena => parse_grade,
        LevelMode::Sandbox => parse_grade_in_sandbox,
    };
    console_with_grade(bindings, grade)
}

/// `grade` in a sandbox: the force acts through the wave step, which a sandbox does not have.
fn parse_grade_in_sandbox(_args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    Err("No waves in a sandbox".to_owned())
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
    fn grade_in_a_sandbox_says_there_are_no_waves() {
        let (mut console, _) = console_for(&BTreeMap::new(), LevelMode::Sandbox);

        console.submit("grade rookie");

        assert_eq!(last_line(&console), "No waves in a sandbox");
        assert!(
            !printed(&console)
                .iter()
                .any(|line| line.starts_with("Grade forced")),
            "{:?}",
            printed(&console)
        );
    }

    #[test]
    fn grade_in_an_arena_level_still_forces_the_grade() {
        let (mut console, _) = console_for(&BTreeMap::new(), LevelMode::Arena);

        console.submit("grade rookie");

        assert_eq!(last_line(&console), "Grade forced: rookie");
        assert_eq!(
            console.take_commands(),
            vec![ConsoleCommand::ForceGrade(Some(Grade::Rookie))]
        );
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
        assert_eq!(on.0, vec![ConsoleCommand::Simple(SimpleArg::Set(true))]);
        assert!(on.1.starts_with("Simple") && on.1.ends_with("on"), "{on:?}");
        assert_eq!(off.0, vec![ConsoleCommand::Simple(SimpleArg::Set(false))]);
        assert!(
            off.1.starts_with("Simple") && off.1.ends_with("off"),
            "{off:?}"
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
        let (mut console, _) = console(&BTreeMap::new());
        let scale = |value| GravityScale::new(value).expect("a valid scale");

        let mut queued = Vec::new();
        for line in [
            "round_gravity 30",
            "round_gravity 0",
            "round_gravity 0.5",
            "round_gravity 1000",
        ] {
            console.submit(line);
            queued.extend(console.take_commands());
        }

        assert_eq!(
            queued,
            vec![
                ConsoleCommand::RoundGravity(scale(30.0)),
                ConsoleCommand::RoundGravity(scale(0.0)),
                ConsoleCommand::RoundGravity(scale(0.5)),
                ConsoleCommand::RoundGravity(scale(1000.0)),
            ]
        );
    }

    #[test]
    fn round_gravity_refuses_negative_and_non_numeric_values() {
        for line in [
            "round_gravity",
            "round_gravity -1",
            "round_gravity fast",
            "round_gravity NaN",
            "round_gravity inf",
            "round_gravity 1000.5",
            "round_gravity 1 2",
        ] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert!(console.take_commands().is_empty(), "{line}");
            let reply = last_line(&console);
            assert!(reply.contains("round_gravity <scale>"), "{line}: {reply:?}");
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
