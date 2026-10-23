//! Command registration: data and lookup, no traits.

/// Turns the arguments after a command name into the game's command and a reply, or an error reply.
/// An empty reply prints nothing; the game then prints its own line with `Console::print`.
pub type ParseFn<G> = fn(&[&str]) -> Result<(G, String), String>;

type EngineFn = fn(&[&str]) -> Result<(EngineCommand, String), String>;

/// A command a game adds to the console.
pub struct CommandSpec<G> {
    pub name: String,
    pub usage: String,
    pub help: String,
    /// Gets the arguments after the name; returns the game's command and a reply, or an error
    /// reply. A plain `fn`, so it cannot capture or touch game state.
    pub parse: ParseFn<G>,
}

/// Who holds a registered name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Engine,
    Game,
}

impl Owner {
    fn describe(self) -> &'static str {
        match self {
            Owner::Engine => "an engine command",
            Owner::Game => "registered",
        }
    }
}

/// Why a registration was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegisterError {
    #[error("'{name}' is already {}", .owner.describe())]
    Taken { name: String, owner: Owner },
    #[error("'{name}' is not a valid command name: it must be non-empty with no whitespace")]
    InvalidName { name: String },
}

/// What the engine's own commands hand back to the game as values.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum EngineCommand {
    Quit,
    DebugView(u32),
    ShadowQuality(u8),
    SsaoQuality(u8),
    /// Tone-map curve on or off, for this session only.
    ToneMap(bool),
    PlaySoundAt {
        path: std::path::PathBuf,
        position: glam::Vec3,
        volume: f32,
    },
}

/// What running a command does.
pub(super) enum Handler<G> {
    Help,
    Clear,
    Engine(EngineFn),
    Game(ParseFn<G>),
}

pub(super) struct Entry<G> {
    name: String,
    pub(super) usage: String,
    pub(super) help: String,
    pub(super) handler: Handler<G>,
}

fn parse_quit(_args: &[&str]) -> Result<(EngineCommand, String), String> {
    Ok((EngineCommand::Quit, "Quitting".to_owned()))
}

const DEBUG_VIEW_NAMES: [&str; 8] = [
    "None",
    "Normals",
    "Bias",
    "Shadows",
    "Light Level",
    "Point Lights",
    "Sky Ambient",
    "Sky Reflection",
];
const QUALITY_NAMES: [&str; 5] = ["Off", "Low", "Medium", "High", "Ultra"];

fn parse_debug_view(args: &[&str]) -> Result<(EngineCommand, String), String> {
    let Some(arg) = args.first() else {
        return Err("Usage: r_debug_view <mode> (0=None, 1=Normals, 2=Bias, 3=Shadows, 4=LightLevel, 5=PointLights, 6=SkyAmbient, 7=SkyReflection)".to_owned());
    };
    let mode = arg
        .parse::<u32>()
        .map_err(|_| "Invalid mode. Must be a number.".to_owned())?;
    let name = usize::try_from(mode)
        .ok()
        .and_then(|i| DEBUG_VIEW_NAMES.get(i))
        .ok_or_else(|| "Debug view must be between 0 and 7".to_owned())?;
    Ok((
        EngineCommand::DebugView(mode),
        format!("Debug view set to: {name} ({mode})"),
    ))
}

/// The level in `args` as a `u8` in `0..=4`, with its name, or the reply that refuses it.
fn parse_quality(args: &[&str], usage: &str) -> Result<(u8, &'static str), String> {
    let Some(arg) = args.first() else {
        return Err(usage.to_owned());
    };
    let level = arg
        .parse::<u32>()
        .map_err(|_| "Invalid quality. Must be a number.".to_owned())?;
    usize::try_from(level)
        .ok()
        .and_then(|i| QUALITY_NAMES.get(i))
        .zip(u8::try_from(level).ok())
        .map(|(name, quality)| (quality, *name))
        .ok_or_else(|| "Quality must be between 0 and 4".to_owned())
}

fn parse_shadow_quality(args: &[&str]) -> Result<(EngineCommand, String), String> {
    let (quality, name) = parse_quality(
        args,
        "Usage: r_shadow_quality <0-4> (0=Off, 1=Low, 2=Med, 3=High, 4=Ultra)",
    )?;
    Ok((
        EngineCommand::ShadowQuality(quality),
        format!("Shadow quality set to: {name} ({quality})"),
    ))
}

fn parse_ssao_quality(args: &[&str]) -> Result<(EngineCommand, String), String> {
    let (quality, name) = parse_quality(
        args,
        "Usage: r_ssao_quality <0-4> (0=Off, 1=Low, 2=Med, 3=High, 4=Ultra)",
    )?;
    Ok((
        EngineCommand::SsaoQuality(quality),
        format!("SSAO quality set to: {name} ({quality})"),
    ))
}

fn parse_tone_map(args: &[&str]) -> Result<(EngineCommand, String), String> {
    let (enabled, reply) = match args.first().copied() {
        None => return Err("Usage: r_tonemap <0|1> (0=Off, 1=On)".to_owned()),
        Some("0") => (false, "Tone map off"),
        Some("1") => (true, "Tone map on"),
        Some(_) => return Err("Tone map must be 0 or 1".to_owned()),
    };
    Ok((EngineCommand::ToneMap(enabled), reply.to_owned()))
}

/// A console argument as a finite `f32`; `None` for text, NaN or infinity.
fn parse_finite(arg: &str) -> Option<f32> {
    arg.parse::<f32>().ok().filter(|n| n.is_finite())
}

fn parse_audio_cue(args: &[&str]) -> Result<(EngineCommand, String), String> {
    let [path, x, y, z, volume @ ..] = args else {
        return Err(AUDIO_CUE_USAGE.to_owned());
    };
    if volume.len() > 1 {
        return Err(AUDIO_CUE_USAGE.to_owned());
    }
    let (Some(x), Some(y), Some(z), Some(volume)) = (
        parse_finite(x),
        parse_finite(y),
        parse_finite(z),
        volume.first().map_or(Some(1.0), |v| parse_finite(v)),
    ) else {
        return Err("Error: position and volume must be finite numbers".to_owned());
    };
    let position = glam::Vec3::new(x, y, z);
    Ok((
        EngineCommand::PlaySoundAt {
            path: (*path).into(),
            position,
            volume,
        },
        format!("Playing {path} at {position} (volume {volume})"),
    ))
}

const AUDIO_CUE_USAGE: &str = "Usage: audio_cue <path> <x> <y> <z> [volume]\n  Example: audio_cue searchlight/assets/rifle_shot.wav 1 0 -5 0.8";

/// The engine's commands first, then the game's, in registration order.
pub(super) struct Registry<G> {
    entries: Vec<Entry<G>>,
}

impl<G> Registry<G> {
    pub(super) fn new() -> Self {
        let engine = |name: &str, usage: &str, help: &str, handler| Entry {
            name: name.to_owned(),
            usage: usage.to_owned(),
            help: help.to_owned(),
            handler,
        };
        Self {
            entries: vec![
                engine("help", "help", "list the commands", Handler::Help),
                engine("clear", "clear", "clear the console", Handler::Clear),
                engine("quit", "quit", "exit the game", Handler::Engine(parse_quit)),
                engine(
                    "r_debug_view",
                    "r_debug_view <0-7>",
                    "set the debug view (0=None, 3=Shadows)",
                    Handler::Engine(parse_debug_view),
                ),
                engine(
                    "r_shadow_quality",
                    "r_shadow_quality <0-4>",
                    "set shadow quality (Off/Low/Med/High/Ultra)",
                    Handler::Engine(parse_shadow_quality),
                ),
                engine(
                    "r_ssao_quality",
                    "r_ssao_quality <0-4>",
                    "set SSAO quality (Off/Low/Med/High/Ultra)",
                    Handler::Engine(parse_ssao_quality),
                ),
                engine(
                    "r_tonemap",
                    "r_tonemap <0|1>",
                    "toggle the tone map for this session (0=Off, 1=On)",
                    Handler::Engine(parse_tone_map),
                ),
                engine(
                    "audio_cue",
                    "audio_cue <path> <x> <y> <z> [volume]",
                    "play a sound at a world position (metres)",
                    Handler::Engine(parse_audio_cue),
                ),
            ],
        }
    }

    pub(super) fn entries(&self) -> &[Entry<G>] {
        &self.entries
    }

    /// The handler for `name`, ignoring case.
    pub(super) fn find(&self, name: &str) -> Option<&Handler<G>> {
        let name = name.to_lowercase();
        self.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| &e.handler)
    }

    /// A refused registration changes nothing.
    pub(super) fn register(&mut self, spec: CommandSpec<G>) -> Result<(), RegisterError> {
        if spec.name.is_empty() || spec.name.chars().any(char::is_whitespace) {
            return Err(RegisterError::InvalidName { name: spec.name });
        }
        let name = spec.name.to_lowercase();
        if let Some(taken) = self.entries.iter().find(|e| e.name == name) {
            let owner = match taken.handler {
                Handler::Game(_) => Owner::Game,
                Handler::Help | Handler::Clear | Handler::Engine(_) => Owner::Engine,
            };
            return Err(RegisterError::Taken { name, owner });
        }
        self.entries.push(Entry {
            name,
            usage: spec.usage,
            help: spec.help,
            handler: Handler::Game(spec.parse),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::console::Console;
    use crate::console::tests::{Cmd, help_lines, parse_tgm, tgm};

    use super::*;

    const DEBUG_VIEW_USAGE: &str = "Usage: r_debug_view <mode> (0=None, 1=Normals, 2=Bias, 3=Shadows, 4=LightLevel, 5=PointLights, 6=SkyAmbient, 7=SkyReflection)";
    const SHADOW_USAGE: &str =
        "Usage: r_shadow_quality <0-4> (0=Off, 1=Low, 2=Med, 3=High, 4=Ultra)";
    const SSAO_USAGE: &str = "Usage: r_ssao_quality <0-4> (0=Off, 1=Low, 2=Med, 3=High, 4=Ultra)";
    const AUDIO_CUE_USAGE: [&str; 2] = [
        "Usage: audio_cue <path> <x> <y> <z> [volume]",
        "  Example: audio_cue searchlight/assets/rifle_shot.wav 1 0 -5 0.8",
    ];

    /// What one typed line queued and printed: the engine commands, the game commands, and the
    /// reply lines after the echo of the line.
    fn run_line(line: &str) -> (Vec<EngineCommand>, Vec<Cmd>, Vec<String>) {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        console.submit(line);
        let printed: Vec<String> = console.output().lines().iter().cloned().collect();
        let echo = printed.iter().position(|l| *l == format!("> {line}"));
        assert!(echo.is_some(), "{printed:?}");
        (
            console.take_engine_commands(),
            console.take_commands(),
            printed[echo.unwrap_or_default() + 1..].to_vec(),
        )
    }

    fn assert_reply(line: &str, expected: Option<EngineCommand>, reply: &[&str]) {
        let (engine, game, printed) = run_line(line);

        assert_eq!(engine, expected.into_iter().collect::<Vec<_>>(), "{line}");
        assert!(game.is_empty(), "{line}");
        assert_eq!(printed, reply, "{line}");
    }

    #[test]
    fn debug_view_parses_with_todays_reply() {
        assert_reply(
            "r_debug_view 3",
            Some(EngineCommand::DebugView(3)),
            &["Debug view set to: Shadows (3)"],
        );
    }

    #[test]
    fn r_tonemap_parses_0_and_1_and_refuses_2() {
        for (line, expected) in [
            ("r_tonemap 0", EngineCommand::ToneMap(false)),
            ("r_tonemap 1", EngineCommand::ToneMap(true)),
        ] {
            let (engine, game, printed) = run_line(line);

            assert_eq!(engine, [expected], "{line}");
            assert!(game.is_empty(), "{line}");
            assert_eq!(printed.len(), 1, "{line}: {printed:?}");
        }
        for line in ["r_tonemap 2", "r_tonemap x", "r_tonemap -1", "r_tonemap"] {
            let (engine, game, printed) = run_line(line);

            assert!(engine.is_empty(), "{line}: {engine:?}");
            assert!(game.is_empty(), "{line}");
            assert_eq!(printed.len(), 1, "{line}: {printed:?}");
        }
        let (_, _, usage) = run_line("r_tonemap");
        assert!(usage[0].starts_with("Usage: r_tonemap"), "{usage:?}");
    }

    #[test]
    fn debug_view_accepts_zero_through_seven_and_refuses_eight() {
        for mode in 0..=7u32 {
            let (engine, game, printed) = run_line(&format!("r_debug_view {mode}"));

            assert_eq!(engine, [EngineCommand::DebugView(mode)], "mode {mode}");
            assert!(game.is_empty(), "mode {mode}");
            assert_eq!(printed.len(), 1, "mode {mode}: {printed:?}");
        }

        let (engine, game, printed) = run_line("r_debug_view 8");

        assert!(engine.is_empty(), "{engine:?}");
        assert!(game.is_empty());
        assert_eq!(printed.len(), 1, "{printed:?}");
        assert!(
            printed[0].contains('7'),
            "the refusal names the range: {printed:?}"
        );
    }

    #[test]
    fn help_shows_the_debug_view_range_as_zero_to_seven() {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        console.submit("help");

        let help = help_lines(&console);

        assert!(
            help.iter()
                .any(|l| l.starts_with("  r_debug_view <0-7> - ")),
            "{help:?}"
        );
    }

    #[test]
    fn audio_cue_parses_position_and_default_volume() {
        assert_reply(
            "audio_cue searchlight/assets/rifle_shot.wav 2 0 0",
            Some(EngineCommand::PlaySoundAt {
                path: "searchlight/assets/rifle_shot.wav".into(),
                position: glam::Vec3::new(2.0, 0.0, 0.0),
                volume: 1.0,
            }),
            &[&format!(
                "Playing searchlight/assets/rifle_shot.wav at {} (volume 1)",
                glam::Vec3::new(2.0, 0.0, 0.0)
            )],
        );
    }

    #[test]
    fn refused_engine_lines_print_todays_reply_and_queue_nothing() {
        assert_reply(
            "r_shadow_quality 9",
            None,
            &["Quality must be between 0 and 4"],
        );
        assert_reply("r_ssao_quality", None, &[SSAO_USAGE]);
        assert_reply("audio_cue missing.wav 1", None, &AUDIO_CUE_USAGE);
    }

    #[test]
    fn help_lists_the_renderer_and_sound_commands() {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        assert_eq!(console.register(tgm()), Ok(()));

        console.submit("help");

        let help = help_lines(&console);
        assert_eq!(help.len(), 9, "{help:?}");
        for prefix in [
            "  r_debug_view <0-7> - ",
            "  r_shadow_quality <0-4> - ",
            "  r_ssao_quality <0-4> - ",
            "  r_tonemap <0|1> - ",
            "  audio_cue <path> <x> <y> <z> [volume] - ",
        ] {
            let line = help.iter().find(|l| l.starts_with(prefix));
            assert!(
                line.is_some_and(|l| l.len() > prefix.len()),
                "{prefix}: {help:?}"
            );
        }
        assert_eq!(
            help[8], "  tgm [0|1] - toggle god mode",
            "game rows stay last"
        );
    }

    #[test]
    fn renderer_and_sound_names_are_engine_owned() {
        for name in [
            "r_debug_view",
            "r_shadow_quality",
            "r_ssao_quality",
            "audio_cue",
        ] {
            let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
            let spec = CommandSpec {
                name: name.to_owned(),
                ..tgm()
            };

            let refused = console.register(spec);

            assert_eq!(
                refused,
                Err(RegisterError::Taken {
                    name: name.to_owned(),
                    owner: Owner::Engine
                }),
                "{name}"
            );
        }
    }

    #[test]
    fn engine_parse_table_matches_strategy() {
        let quality = |name: &str, label: &str, make: fn(u8) -> EngineCommand, usage: &str| {
            let mut rows: Vec<(String, Option<EngineCommand>, Vec<String>)> = Vec::new();
            for (n, word) in [
                (0u8, "Off"),
                (1, "Low"),
                (2, "Medium"),
                (3, "High"),
                (4, "Ultra"),
            ] {
                rows.push((
                    format!("{name} {n}"),
                    Some(make(n)),
                    vec![format!("{label} quality set to: {word} ({n})")],
                ));
            }
            for (arg, reply) in [
                ("5", "Quality must be between 0 and 4"),
                ("x", "Invalid quality. Must be a number."),
                ("-1", "Invalid quality. Must be a number."),
                ("2.5", "Invalid quality. Must be a number."),
            ] {
                rows.push((format!("{name} {arg}"), None, vec![reply.to_owned()]));
            }
            rows.push((name.to_owned(), None, vec![usage.to_owned()]));
            rows
        };
        let mut rows = quality(
            "r_shadow_quality",
            "Shadow",
            EngineCommand::ShadowQuality,
            SHADOW_USAGE,
        );
        rows.extend(quality(
            "r_ssao_quality",
            "SSAO",
            EngineCommand::SsaoQuality,
            SSAO_USAGE,
        ));
        for (mode, word) in [
            (0u32, "None"),
            (1, "Normals"),
            (2, "Bias"),
            (3, "Shadows"),
            (4, "Light Level"),
            (5, "Point Lights"),
            (6, "Sky Ambient"),
            (7, "Sky Reflection"),
        ] {
            rows.push((
                format!("r_debug_view {mode}"),
                Some(EngineCommand::DebugView(mode)),
                vec![format!("Debug view set to: {word} ({mode})")],
            ));
        }
        for (arg, reply) in [
            ("8", "Debug view must be between 0 and 7"),
            ("x", "Invalid mode. Must be a number."),
        ] {
            rows.push((format!("r_debug_view {arg}"), None, vec![reply.to_owned()]));
        }
        rows.push((
            "r_debug_view".to_owned(),
            None,
            vec![DEBUG_VIEW_USAGE.to_owned()],
        ));
        let finite = "Error: position and volume must be finite numbers";
        for line in [
            "audio_cue b.wav a 2 3",
            "audio_cue b.wav 1 2 3 loud",
            "audio_cue b.wav 1 nan 3",
            "audio_cue b.wav 1 2 3 inf",
            "audio_cue b.wav -inf 2 3",
        ] {
            rows.push((line.to_owned(), None, vec![finite.to_owned()]));
        }
        for line in [
            "audio_cue",
            "audio_cue b.wav",
            "audio_cue b.wav 1 2",
            "audio_cue b.wav 1 2 3 0.5 9",
        ] {
            rows.push((
                line.to_owned(),
                None,
                AUDIO_CUE_USAGE.iter().map(|&l| l.to_owned()).collect(),
            ));
        }
        rows.push((
            "audio_cue sounds/boom.wav 1.5 -2 30 0.4".to_owned(),
            Some(EngineCommand::PlaySoundAt {
                path: "sounds/boom.wav".into(),
                position: glam::Vec3::new(1.5, -2.0, 30.0),
                volume: 0.4,
            }),
            vec![format!(
                "Playing sounds/boom.wav at {} (volume 0.4)",
                glam::Vec3::new(1.5, -2.0, 30.0)
            )],
        ));

        for (line, expected, reply) in rows {
            let reply: Vec<&str> = reply.iter().map(String::as_str).collect();
            assert_reply(&line, expected, &reply);
        }
    }

    #[test]
    fn registering_an_engine_name_is_refused_and_the_engine_command_stays() {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        let hijack = CommandSpec {
            name: "quit".to_owned(),
            usage: "quit".to_owned(),
            help: "hijacked".to_owned(),
            parse: parse_tgm,
        };

        let refused = console.register(hijack);
        console.submit("quit");
        console.submit("help");

        assert_eq!(
            refused,
            Err(RegisterError::Taken {
                name: "quit".to_owned(),
                owner: Owner::Engine
            })
        );
        let message = refused.unwrap_err().to_string();
        assert_eq!(message, "'quit' is already an engine command");
        assert_eq!(console.take_engine_commands(), vec![EngineCommand::Quit]);
        assert!(console.take_commands().is_empty());
        let help = help_lines(&console);
        assert_eq!(help.iter().filter(|l| l.starts_with("  quit")).count(), 1);
        assert!(help.iter().all(|l| !l.contains("hijacked")), "{help:?}");
    }

    #[test]
    fn registering_a_taken_game_name_is_refused_and_the_first_stays() {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        assert_eq!(console.register(tgm()), Ok(()));
        let again = CommandSpec {
            help: "second".to_owned(),
            ..tgm()
        };

        let refused = console.register(again);
        console.submit("tgm 1");
        console.submit("help");

        assert_eq!(
            refused,
            Err(RegisterError::Taken {
                name: "tgm".to_owned(),
                owner: Owner::Game
            })
        );
        let message = refused.unwrap_err().to_string();
        assert_eq!(message, "'tgm' is already registered");
        assert_eq!(console.take_commands(), vec![Cmd::God(true)]);
        let help = help_lines(&console);
        assert_eq!(help.iter().filter(|l| l.starts_with("  tgm")).count(), 1);
        assert!(help.iter().all(|l| !l.contains("second")), "{help:?}");
    }

    #[test]
    fn a_name_differing_only_in_case_is_taken() {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        assert_eq!(console.register(tgm()), Ok(()));
        let shouting = CommandSpec {
            name: "TGM".to_owned(),
            ..tgm()
        };
        let shouting_engine = CommandSpec {
            name: "QuIt".to_owned(),
            ..tgm()
        };

        let against_game = console.register(shouting);
        let against_engine = console.register(shouting_engine);

        assert!(matches!(
            against_game,
            Err(RegisterError::Taken {
                owner: Owner::Game,
                ..
            })
        ));
        assert!(matches!(
            against_engine,
            Err(RegisterError::Taken {
                owner: Owner::Engine,
                ..
            })
        ));
    }

    #[test]
    fn a_refused_upper_case_engine_name_adds_no_help_entry() {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        assert_eq!(console.register(tgm()), Ok(()));
        let shouting = CommandSpec {
            name: "QUIT".to_owned(),
            ..tgm()
        };

        let refused = console.register(shouting);
        console.submit("help");

        assert_eq!(
            refused,
            Err(RegisterError::Taken {
                name: "quit".to_owned(),
                owner: Owner::Engine
            })
        );
        let help = help_lines(&console);
        assert_eq!(help.len(), 9, "{help:?}");
        assert_eq!(help.iter().filter(|l| l.starts_with("  quit")).count(), 1);
        assert_eq!(help[8], "  tgm [0|1] - toggle god mode");
    }

    #[test]
    fn invalid_names_are_refused() {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        console.submit("help");
        let before = help_lines(&console);

        for bad in ["", "a b", "a\tb", " "] {
            let spec = CommandSpec {
                name: bad.to_owned(),
                ..tgm()
            };
            let refused = console.register(spec);
            assert_eq!(
                refused,
                Err(RegisterError::InvalidName {
                    name: bad.to_owned()
                }),
                "{bad:?}"
            );
        }

        console.submit("help");
        assert_eq!(help_lines(&console), before);
    }

    #[test]
    fn names_with_surrounding_whitespace_are_refused_and_register_nothing() {
        let (mut console, _) = Console::<Cmd>::new(&std::collections::BTreeMap::new());
        console.submit("help");
        let before = help_lines(&console);

        for bad in [" tgm", "tgm\t", " tgm "] {
            let spec = CommandSpec {
                name: bad.to_owned(),
                ..tgm()
            };
            let refused = console.register(spec);
            assert_eq!(
                refused,
                Err(RegisterError::InvalidName {
                    name: bad.to_owned()
                }),
                "{bad:?}"
            );
        }

        console.submit("help");
        assert_eq!(help_lines(&console), before);
        console.submit("tgm 1");
        assert!(console.take_commands().is_empty());
    }
}
