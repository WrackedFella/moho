//! The strategy line's console commands, added to the engine console with [`register`].
//!
//! Every command here is a hidden debug command: it appears only in the console and its
//! `help`, never in Settings or a menu, and none of it persists across a launch.

use glam::Vec3;
use moho_render_api::MarkerStyle;
use moho_ui_shell::console::registry::ParseFn;
use moho_ui_shell::{CommandSpec, Console, RegisterError};

/// What a strategy console line asks the game to do on its next tick. Plain data: the console
/// never touches game state.
#[derive(Debug, Clone, PartialEq)]
pub enum StrategyConsoleCommand {
    ToggleGodMode,
    ToggleNoclip,
    /// Hours, `0.0..=24.0`.
    SetTimeOfDay(f32),
    SetSunDirection {
        yaw_deg: f32,
        pitch_deg: f32,
    },
    Spawn {
        kind: String,
        args: Option<String>,
    },
    /// Corners in world metres.
    ShowMarker {
        id: u64,
        min: Vec3,
        max: Vec3,
        style: MarkerStyle,
    },
    /// Every marker when `None`.
    ClearMarkers(Option<u64>),
    /// Metres; `None` clears the cut.
    SetViewHeight(Option<f32>),
}

/// Adds the strategy commands to `console`.
///
/// # Errors
/// A [`RegisterError`] if a name is already taken; that is a bug in the command table.
pub fn register(console: &mut Console<StrategyConsoleCommand>) -> Result<(), RegisterError> {
    let table: [(&str, &str, ParseFn<StrategyConsoleCommand>); 8] = [
        ("god", "Toggle god mode (invincibility)", parse_god),
        (
            "noclip",
            "Toggle noclip mode (fly through walls)",
            parse_noclip,
        ),
        (
            "sun <yaw> <pitch>",
            "Set sun direction (degrees)",
            parse_sun,
        ),
        (
            "time <0-24>",
            "Set time of day (0=midnight, 6=dawn, 12=noon, 18=dusk)",
            parse_time,
        ),
        (
            "spawn <type> [material/args]",
            "Spawn an entity or block",
            parse_spawn,
        ),
        (
            "marker <id> <x> <y> <z> [size] [fill|outline]",
            "Show a box marker at a block corner",
            parse_marker,
        ),
        (
            "marker_clear [id]",
            "Remove one marker, or all markers",
            parse_marker_clear,
        ),
        (
            "r_view_height <metres|off>",
            "Draw nothing in the world above a height",
            parse_view_height,
        ),
    ];
    for (usage, help, parse) in table {
        let name = usage.split(' ').next().unwrap_or(usage);
        console.register(CommandSpec {
            name: name.to_owned(),
            usage: usage.to_owned(),
            help: help.to_owned(),
            parse,
        })?;
    }
    Ok(())
}

type Parsed = Result<(StrategyConsoleCommand, String), String>;

fn parse_god(_args: &[&str]) -> Parsed {
    Ok((
        StrategyConsoleCommand::ToggleGodMode,
        "Toggling god mode...".to_owned(),
    ))
}

fn parse_noclip(_args: &[&str]) -> Parsed {
    Ok((
        StrategyConsoleCommand::ToggleNoclip,
        "Toggling noclip mode...".to_owned(),
    ))
}

fn parse_sun(args: &[&str]) -> Parsed {
    let [yaw, pitch] = args else {
        return Err("Usage: sun <yaw> <pitch>\n  Example: sun 45 60".to_owned());
    };
    let (Ok(yaw_deg), Ok(pitch_deg)) = (yaw.parse::<f32>(), pitch.parse::<f32>()) else {
        return Err("Error: yaw and pitch must be numbers".to_owned());
    };
    Ok((
        StrategyConsoleCommand::SetSunDirection { yaw_deg, pitch_deg },
        format!("Setting sun direction: yaw={yaw_deg}, pitch={pitch_deg}"),
    ))
}

fn parse_time(args: &[&str]) -> Parsed {
    let [hours] = args else {
        return Err(
            "Usage: time <0-24>\n  0 = midnight, 6 = dawn, 12 = noon, 18 = dusk, 24 = midnight"
                .to_owned(),
        );
    };
    let Ok(time) = hours.parse::<f32>() else {
        return Err("Error: time must be a number between 0 and 24".to_owned());
    };
    let clamped = time.clamp(0.0, 24.0);
    let whole = clamped.floor() as u32;
    let minutes = ((clamped.fract() * 60.0) as u32).min(59);
    Ok((
        StrategyConsoleCommand::SetTimeOfDay(clamped),
        format!("Setting time to {whole:02}:{minutes:02}"),
    ))
}

fn parse_spawn(args: &[&str]) -> Parsed {
    let [kind, rest @ ..] = args else {
        return Err("Usage: spawn <type> [material/args]".to_owned());
    };
    let extra = (!rest.is_empty()).then(|| rest.join(" "));
    Ok((
        StrategyConsoleCommand::Spawn {
            kind: (*kind).to_owned(),
            args: extra,
        },
        format!("Spawning {kind}..."),
    ))
}

/// A console argument as a finite `f32`; `None` for text, NaN or infinity.
fn parse_finite(arg: &str) -> Option<f32> {
    arg.parse::<f32>().ok().filter(|n| n.is_finite())
}

const ID_ERROR: &str = "Error: id must be a non-negative integer";

fn parse_marker(args: &[&str]) -> Parsed {
    if !(4..=6).contains(&args.len()) {
        return Err(
            "Usage: marker <id> <x> <y> <z> [size] [fill|outline]\n  Example: marker 1 10 4 -3 2 outline"
                .to_owned(),
        );
    }
    let id = args[0].parse::<u64>().map_err(|_| ID_ERROR.to_owned())?;
    let (Some(x), Some(y), Some(z), Some(size)) = (
        parse_finite(args[1]),
        parse_finite(args[2]),
        parse_finite(args[3]),
        args.get(4).map_or(Some(1.0), |part| parse_finite(part)),
    ) else {
        return Err("Error: position and size must be finite numbers".to_owned());
    };
    if size <= 0.0 {
        return Err("Error: size must be greater than zero".to_owned());
    }
    let style = match args.get(5).copied() {
        None | Some("fill") => MarkerStyle::Fill,
        Some("outline") => MarkerStyle::Outline,
        Some(other) => return Err(format!("Error: style must be fill or outline, not {other}")),
    };
    let min = Vec3::new(x, y, z);
    let max = min + Vec3::splat(size);
    if !max.is_finite() {
        return Err("Error: the box's far corner is out of range".to_owned());
    }
    Ok((
        StrategyConsoleCommand::ShowMarker {
            id,
            min,
            max,
            style,
        },
        format!("Marker {id} from {min} to {max}"),
    ))
}

fn parse_marker_clear(args: &[&str]) -> Parsed {
    match args {
        [] => Ok((
            StrategyConsoleCommand::ClearMarkers(None),
            "Clearing all markers".to_owned(),
        )),
        [id] => {
            let id = id.parse::<u64>().map_err(|_| ID_ERROR.to_owned())?;
            Ok((
                StrategyConsoleCommand::ClearMarkers(Some(id)),
                format!("Clearing marker {id}"),
            ))
        }
        _ => Err("Usage: marker_clear [id]".to_owned()),
    }
}

fn parse_view_height(args: &[&str]) -> Parsed {
    let [arg] = args else {
        return Err("Usage: r_view_height <metres|off>".to_owned());
    };
    if *arg == "off" {
        return Ok((
            StrategyConsoleCommand::SetViewHeight(None),
            "View height cleared".to_owned(),
        ));
    }
    match parse_finite(arg) {
        Some(height) => Ok((
            StrategyConsoleCommand::SetViewHeight(Some(height)),
            format!("View height set to {height} m"),
        )),
        None => Err("Invalid height. Must be a finite number of metres, or off.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn console() -> Console<StrategyConsoleCommand> {
        let (mut console, warnings) = Console::new(&BTreeMap::new());
        assert_eq!(warnings, vec![]);
        assert_eq!(register(&mut console), Ok(()));
        console
    }

    /// What one typed line queued and the reply lines printed after its echo.
    fn run(line: &str) -> (Vec<StrategyConsoleCommand>, Vec<String>) {
        let mut console = console();
        console.submit(line);
        let printed: Vec<String> = console.output().lines().iter().cloned().collect();
        let echo = printed
            .iter()
            .position(|l| *l == format!("> {line}"))
            .unwrap_or_else(|| panic!("no echo of {line:?} in {printed:?}"));
        (console.take_commands(), printed[echo + 1..].to_vec())
    }

    fn assert_runs(line: &str, command: Option<StrategyConsoleCommand>, reply: &[&str]) {
        let (commands, printed) = run(line);

        assert_eq!(commands, command.into_iter().collect::<Vec<_>>(), "{line}");
        assert_eq!(printed, reply, "{line}");
    }

    fn v(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new(x, y, z)
    }

    #[test]
    fn strategy_commands_register_without_clash() {
        let (mut console, _) = Console::<StrategyConsoleCommand>::new(&BTreeMap::new());

        let registered = register(&mut console);

        assert_eq!(registered, Ok(()));
    }

    #[test]
    fn strategy_commands_parse_with_todays_replies() {
        use StrategyConsoleCommand::{SetSunDirection, SetTimeOfDay, Spawn};
        let spawn = |kind: &str, args: Option<&str>| Spawn {
            kind: kind.to_owned(),
            args: args.map(str::to_owned),
        };
        let rows: Vec<(&str, Option<StrategyConsoleCommand>, Vec<&str>)> = vec![
            (
                "god",
                Some(StrategyConsoleCommand::ToggleGodMode),
                vec!["Toggling god mode..."],
            ),
            (
                "noclip",
                Some(StrategyConsoleCommand::ToggleNoclip),
                vec!["Toggling noclip mode..."],
            ),
            (
                "sun 45 60",
                Some(SetSunDirection {
                    yaw_deg: 45.0,
                    pitch_deg: 60.0,
                }),
                vec!["Setting sun direction: yaw=45, pitch=60"],
            ),
            (
                "sun 45 30",
                Some(SetSunDirection {
                    yaw_deg: 45.0,
                    pitch_deg: 30.0,
                }),
                vec!["Setting sun direction: yaw=45, pitch=30"],
            ),
            (
                "sun 45",
                None,
                vec!["Usage: sun <yaw> <pitch>", "  Example: sun 45 60"],
            ),
            (
                "sun",
                None,
                vec!["Usage: sun <yaw> <pitch>", "  Example: sun 45 60"],
            ),
            (
                "sun abc def",
                None,
                vec!["Error: yaw and pitch must be numbers"],
            ),
            (
                "time 12.5",
                Some(SetTimeOfDay(12.5)),
                vec!["Setting time to 12:30"],
            ),
            (
                "time 18",
                Some(SetTimeOfDay(18.0)),
                vec!["Setting time to 18:00"],
            ),
            (
                "time 25",
                Some(SetTimeOfDay(24.0)),
                vec!["Setting time to 24:00"],
            ),
            (
                "time -5",
                Some(SetTimeOfDay(0.0)),
                vec!["Setting time to 00:00"],
            ),
            (
                "time",
                None,
                vec![
                    "Usage: time <0-24>",
                    "  0 = midnight, 6 = dawn, 12 = noon, 18 = dusk, 24 = midnight",
                ],
            ),
            (
                "time 1 2",
                None,
                vec![
                    "Usage: time <0-24>",
                    "  0 = midnight, 6 = dawn, 12 = noon, 18 = dusk, 24 = midnight",
                ],
            ),
            (
                "time abc",
                None,
                vec!["Error: time must be a number between 0 and 24"],
            ),
            (
                "spawn tree oak wood",
                Some(spawn("tree", Some("oak wood"))),
                vec!["Spawning tree..."],
            ),
            (
                "spawn sphere",
                Some(spawn("sphere", None)),
                vec!["Spawning sphere..."],
            ),
            (
                "spawn 123",
                Some(spawn("123", None)),
                vec!["Spawning 123..."],
            ),
            ("spawn", None, vec!["Usage: spawn <type> [material/args]"]),
        ];

        for (line, command, reply) in rows {
            assert_runs(line, command, &reply);
        }
    }

    #[test]
    fn marker_parses_id_position_size_and_style() {
        let show = |id, min, max, style| StrategyConsoleCommand::ShowMarker {
            id,
            min,
            max,
            style,
        };
        let reply = |id: u64, min: Vec3, max: Vec3| format!("Marker {id} from {min} to {max}");
        let (min, max) = (v(1.0, 2.0, 3.0), v(3.0, 4.0, 5.0));
        let rows = [
            (
                "marker 7 1.5 -2 30 2.5",
                show(7, v(1.5, -2.0, 30.0), v(4.0, 0.5, 32.5), MarkerStyle::Fill),
                reply(7, v(1.5, -2.0, 30.0), v(4.0, 0.5, 32.5)),
            ),
            (
                "marker 8 1 2 3",
                show(8, v(1.0, 2.0, 3.0), v(2.0, 3.0, 4.0), MarkerStyle::Fill),
                reply(8, v(1.0, 2.0, 3.0), v(2.0, 3.0, 4.0)),
            ),
            (
                "marker 5 1 2 3 2 outline",
                show(5, min, max, MarkerStyle::Outline),
                reply(5, min, max),
            ),
            (
                "marker 5 1 2 3 2 fill",
                show(5, min, max, MarkerStyle::Fill),
                reply(5, min, max),
            ),
            (
                "marker 5 1 2 3 2",
                show(5, min, max, MarkerStyle::Fill),
                reply(5, min, max),
            ),
        ];

        for (line, command, reply) in rows {
            assert_runs(line, Some(command), &[&reply]);
        }
    }

    #[test]
    fn marker_refuses_bad_lines_with_todays_replies() {
        let usage = [
            "Usage: marker <id> <x> <y> <z> [size] [fill|outline]",
            "  Example: marker 1 10 4 -3 2 outline",
        ];
        let id = "Error: id must be a non-negative integer";
        let number = "Error: position and size must be finite numbers";
        let size = "Error: size must be greater than zero";
        let rows: [(&str, &[&str]); 19] = [
            ("marker x 1 2 3", &[id]),
            ("marker -1 1 2 3", &[id]),
            ("marker 1.5 1 2 3", &[id]),
            ("marker 1 a 2 3", &[number]),
            ("marker 1 1 2 3 big", &[number]),
            ("marker 1 1 nan 3", &[number]),
            ("marker 1 1 2 3 inf", &[number]),
            ("marker 1 1 2 3 0", &[size]),
            ("marker 1 1 2 3 -2", &[size]),
            (
                "marker 1 3e38 0 0 3e38",
                &["Error: the box's far corner is out of range"],
            ),
            (
                "marker 5 1 2 3 2 dotted",
                &["Error: style must be fill or outline, not dotted"],
            ),
            ("marker 1 1 2", &usage),
            ("marker 1", &usage),
            ("marker", &usage),
            ("marker 1 1 2 3 4 fill 5", &usage),
            ("marker 5 1 2 3 2 fill 9", &usage),
            ("marker 1 2 3 4 5 6 7 8", &usage),
            ("marker 1 1 2 3 4 outline x", &usage),
            ("marker 1 1 2 3 4 outline 1 2", &usage),
        ];

        for (line, reply) in rows {
            assert_runs(line, None, reply);
        }
    }

    #[test]
    fn marker_clear_with_and_without_id() {
        use StrategyConsoleCommand::ClearMarkers;
        let rows: [(&str, Option<StrategyConsoleCommand>, &[&str]); 4] = [
            (
                "marker_clear",
                Some(ClearMarkers(None)),
                &["Clearing all markers"],
            ),
            (
                "marker_clear 7",
                Some(ClearMarkers(Some(7))),
                &["Clearing marker 7"],
            ),
            (
                "marker_clear x",
                None,
                &["Error: id must be a non-negative integer"],
            ),
            ("marker_clear 7 8", None, &["Usage: marker_clear [id]"]),
        ];

        for (line, command, reply) in rows {
            assert_runs(line, command, reply);
        }
    }

    #[test]
    fn r_view_height_sets_clears_and_refuses_with_todays_replies() {
        use StrategyConsoleCommand::SetViewHeight;
        let invalid = "Invalid height. Must be a finite number of metres, or off.";
        let usage = "Usage: r_view_height <metres|off>";
        let rows: [(&str, Option<StrategyConsoleCommand>, &[&str]); 8] = [
            (
                "r_view_height 12.5",
                Some(SetViewHeight(Some(12.5))),
                &["View height set to 12.5 m"],
            ),
            (
                "r_view_height off",
                Some(SetViewHeight(None)),
                &["View height cleared"],
            ),
            ("r_view_height x", None, &[invalid]),
            ("r_view_height NaN", None, &[invalid]),
            ("r_view_height inf", None, &[invalid]),
            ("r_view_height", None, &[usage]),
            ("r_view_height 1 2", None, &[usage]),
            ("r_view_height off 2", None, &[usage]),
        ];

        for (line, command, reply) in rows {
            assert_runs(line, command, reply);
        }
    }

    #[test]
    fn help_shows_the_debug_view_range_as_zero_to_seven() {
        let mut console = console();

        console.submit("help");

        let printed: Vec<String> = console.output().lines().iter().cloned().collect();
        assert!(
            printed
                .iter()
                .any(|l| l.starts_with("  r_debug_view <0-7> - ")),
            "{printed:?}"
        );
    }

    #[test]
    fn help_lists_engine_and_strategy_commands() {
        let mut console = console();

        console.submit("help");

        let printed: Vec<String> = console.output().lines().iter().cloned().collect();
        let echo = printed.iter().position(|l| l == "> help");
        let help = &printed[echo.expect("help is echoed") + 1..];
        assert_eq!(help.len(), 15, "{help:?}");
        for prefix in [
            "  help - ",
            "  clear - ",
            "  quit - ",
            "  r_debug_view <0-6> - ",
            "  r_shadow_quality <0-4> - ",
            "  r_ssao_quality <0-4> - ",
            "  audio_cue <path> <x> <y> <z> [volume] - ",
            "  god - ",
            "  noclip - ",
            "  spawn <type> [material/args] - ",
            "  time <0-24> - ",
            "  sun <yaw> <pitch> - ",
            "  marker <id> <x> <y> <z> [size] [fill|outline] - ",
            "  marker_clear [id] - ",
            "  r_view_height <metres|off> - ",
        ] {
            let line = help.iter().find(|l| l.starts_with(prefix));
            assert!(
                line.is_some_and(|l| l.len() > prefix.len()),
                "{prefix}: {help:?}"
            );
        }
        assert!(
            !help.iter().any(|l| l.contains("snd_play_at")),
            "the old sound name is gone: {help:?}"
        );
    }
}
