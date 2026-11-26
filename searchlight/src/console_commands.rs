//! The FPS's console commands. They are hidden debug commands: they appear only in the console
//! and its `help`, never in a Settings screen. Every registration lives in this module.

use std::collections::BTreeMap;

use moho_input::bindings::BindingWarning;
use moho_ui_shell::Console;
use moho_ui_shell::console::registry::{CommandSpec, ParseFn};
use searchlight_rules::{Grade, GravityScale, MAX_GRAVITY_SCALE};

use crate::aim::Magnification;
use crate::head_bob::HeadBobLevel;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConsoleCommand {
    GodMode(GodModeArg),
    Fullscreen,
    /// Force the grade of new enemies, or with `None` clear the force.
    ForceGrade(Option<Grade>),
    Simple(SimpleArg),
    /// Set how hard gravity pulls on rounds.
    RoundGravity(GravityScale),
    /// Show or set how much the view bobs while moving.
    HeadBob(HeadBobArg),
    /// Open the demo item lists.
    Items,
    /// Set how much a held Aim magnifies the view.
    Zoom(Magnification),
}

/// Parses `zoom <factor>`.
pub fn parse_zoom(_args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    todo!()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadBobArg {
    Show,
    Set(HeadBobLevel),
}

/// Parses `head_bob [percent]`; no argument shows the level.
pub fn parse_head_bob(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    let usage = || "Usage: head_bob [percent] (0 to 100, steps of 5)".to_owned();
    match args {
        [] => Ok((ConsoleCommand::HeadBob(HeadBobArg::Show), String::new())),
        [text] => {
            let level = text
                .parse::<i32>()
                .ok()
                .and_then(|percent| HeadBobLevel::new(percent).ok())
                .ok_or_else(usage)?;
            Ok((
                ConsoleCommand::HeadBob(HeadBobArg::Set(level)),
                format!("Head bob: {level}"),
            ))
        }
        _ => Err(usage()),
    }
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
pub fn parse_simple(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    match args {
        [] => Ok((ConsoleCommand::Simple(SimpleArg::Toggle), String::new())),
        ["1"] => Ok((
            ConsoleCommand::Simple(SimpleArg::Set(true)),
            "Simple: on".to_owned(),
        )),
        ["0"] => Ok((
            ConsoleCommand::Simple(SimpleArg::Set(false)),
            "Simple: off".to_owned(),
        )),
        _ => Err("Usage: simple [0|1]".to_owned()),
    }
}

/// Parses `round_gravity <scale>`.
pub fn parse_round_gravity(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    let usage = || format!("Usage: round_gravity <scale> (0 to {MAX_GRAVITY_SCALE})");
    let [text] = args else {
        return Err(usage());
    };
    let scale = text
        .parse::<f32>()
        .ok()
        .and_then(|scale| GravityScale::new(scale).ok())
        .ok_or_else(usage)?;
    Ok((
        ConsoleCommand::RoundGravity(scale),
        format!("Round gravity: {text}"),
    ))
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

/// Parses `items`; it takes no arguments.
pub fn parse_items(args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    match args {
        [] => Ok((ConsoleCommand::Items, String::new())),
        _ => Err("Usage: items".to_owned()),
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
    console
        .register(CommandSpec {
            name: "head_bob".to_owned(),
            usage: "head_bob [percent]".to_owned(),
            help: "view bob while moving (0 to 100 %)".to_owned(),
            parse: parse_head_bob,
        })
        .expect("\"head_bob\" is not an engine command and is registered once");
    console
        .register(CommandSpec {
            name: "items".to_owned(),
            usage: "items".to_owned(),
            help: "open the demo item lists".to_owned(),
            parse: parse_items,
        })
        .expect("\"items\" is not an engine command and is registered once");
    console
        .register(CommandSpec {
            name: "zoom".to_owned(),
            usage: "zoom <factor>".to_owned(),
            help: "magnification while aiming (test tool)".to_owned(),
            parse: parse_zoom,
        })
        .expect("\"zoom\" is not an engine command and is registered once");
    (console, warnings)
}

/// [`console`] for a level that has waves or not: without waves `grade` replies that there are
/// none.
pub fn console_for(
    bindings: &BTreeMap<String, String>,
    has_waves: bool,
) -> (Console<ConsoleCommand>, Vec<BindingWarning>) {
    let grade = if has_waves {
        parse_grade
    } else {
        parse_grade_without_waves
    };
    console_with_grade(bindings, grade)
}

/// `grade` on a level without waves: the force acts through the wave step, which it lacks.
fn parse_grade_without_waves(_args: &[&str]) -> Result<(ConsoleCommand, String), String> {
    Err("No waves on this level".to_owned())
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
    fn grade_without_waves_says_there_are_no_waves_on_this_level() {
        let (mut console, _) = console_for(&BTreeMap::new(), false);

        console.submit("grade rookie");

        assert_eq!(last_line(&console), "No waves on this level");
        assert!(console.take_commands().is_empty());
        assert!(
            !printed(&console)
                .iter()
                .any(|line| line.starts_with("Grade forced")),
            "{:?}",
            printed(&console)
        );
    }

    #[test]
    fn grade_with_waves_still_forces_the_grade() {
        let (mut console, _) = console_for(&BTreeMap::new(), true);

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

    const HEAD_BOB_USAGE: &str = "Usage: head_bob [percent] (0 to 100, steps of 5)";

    fn head_bob_level(percent: i32) -> HeadBobLevel {
        HeadBobLevel::new(percent).expect("a percentage on the grid")
    }

    #[test]
    fn bare_head_bob_shows_and_a_grid_value_sets() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("head_bob");
        let shown = (console.take_commands(), last_line(&console));
        for percent in [0, 5, 25, 50, 95, 100] {
            console.submit(&format!("head_bob {percent}"));

            assert_eq!(
                console.take_commands(),
                vec![ConsoleCommand::HeadBob(HeadBobArg::Set(head_bob_level(
                    percent
                )))],
                "{percent}"
            );
            assert_eq!(last_line(&console), format!("Head bob: {percent} %"));
        }

        assert_eq!(
            shown,
            (
                vec![ConsoleCommand::HeadBob(HeadBobArg::Show)],
                "> head_bob".to_owned()
            ),
            "a bare head_bob queues a show and prints nothing itself"
        );
    }

    #[test]
    fn head_bob_outside_zero_to_a_hundred_or_off_the_grid_is_refused_naming_the_range() {
        for line in [
            "head_bob 150",
            "head_bob 105",
            "head_bob -5",
            "head_bob -1",
            "head_bob 23",
            "head_bob 1",
            "head_bob 2.5",
            "head_bob x",
            "head_bob 50%",
            "head_bob 25 50",
            "head_bob 99999999999",
        ] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert!(console.take_commands().is_empty(), "{line}");
            assert_eq!(last_line(&console), HEAD_BOB_USAGE, "{line}");
        }
    }

    #[test]
    fn help_lists_head_bob() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("help");

        let lines = printed(&console);
        let expected = "  head_bob [percent] - view bob while moving (0 to 100 %)".to_owned();
        assert!(lines.contains(&expected), "{lines:?}");
    }

    #[test]
    fn items_takes_no_arguments_and_rejects_extra_ones() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("items");
        let queued = console.take_commands();
        console.submit("items now");
        let refused = (console.take_commands(), last_line(&console));

        assert_eq!(queued, vec![ConsoleCommand::Items]);
        assert_eq!(refused, (vec![], "Usage: items".to_owned()));
    }

    const ZOOM_USAGE: &str = "Usage: zoom <factor> (1 to 12)";

    fn magnification(factor: f32) -> Magnification {
        Magnification::new(factor).expect("a factor from 1 to 12")
    }

    #[test]
    fn zoom_takes_a_factor_from_one_to_twelve() {
        for (line, factor, printed) in [
            ("zoom 5", 5.0, "Zoom: 5x"),
            ("zoom 1", 1.0, "Zoom: 1x"),
            ("zoom 12", 12.0, "Zoom: 12x"),
            ("zoom 2.5", 2.5, "Zoom: 2.5x"),
            ("zoom 10", 10.0, "Zoom: 10x"),
        ] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert_eq!(
                console.take_commands(),
                vec![ConsoleCommand::Zoom(magnification(factor))],
                "{line}"
            );
            assert_eq!(last_line(&console), printed, "{line}");
        }
    }

    #[test]
    fn zoom_outside_one_to_twelve_is_refused_naming_the_range() {
        for line in [
            "zoom 0",
            "zoom 12.5",
            "zoom -1",
            "zoom NaN",
            "zoom inf",
            "zoom x",
            "zoom",
            "zoom 5 6",
        ] {
            let (mut console, _) = console(&BTreeMap::new());

            console.submit(line);

            assert!(console.take_commands().is_empty(), "{line}");
            assert_eq!(last_line(&console), ZOOM_USAGE, "{line}");
        }
    }

    #[test]
    fn help_lists_zoom() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("help");

        let lines = printed(&console);
        let expected = "  zoom <factor> - magnification while aiming (test tool)".to_owned();
        assert!(lines.contains(&expected), "{lines:?}");
    }

    #[test]
    fn help_lists_items() {
        let (mut console, _) = console(&BTreeMap::new());

        console.submit("help");

        let lines = printed(&console);
        assert!(
            lines.iter().any(|line| line.starts_with("  items - ")),
            "{lines:?}"
        );
    }
}
