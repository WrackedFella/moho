//! Command registration: data and lookup, no traits.

/// Turns the arguments after a command name into the game's command and a reply, or an error reply.
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

    const DEBUG_VIEW_USAGE: &str = "Usage: r_debug_view <mode> (0=None, 1=Normals, 2=Bias, 3=Shadows, 4=LightLevel, 5=PointLights)";
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
        assert_eq!(help.len(), 8, "{help:?}");
        for prefix in [
            "  r_debug_view <0-5> - ",
            "  r_shadow_quality <0-4> - ",
            "  r_ssao_quality <0-4> - ",
            "  audio_cue <path> <x> <y> <z> [volume] - ",
        ] {
            let line = help.iter().find(|l| l.starts_with(prefix));
            assert!(
                line.is_some_and(|l| l.len() > prefix.len()),
                "{prefix}: {help:?}"
            );
        }
        assert_eq!(
            help[7], "  tgm [0|1] - toggle god mode",
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
        ] {
            rows.push((
                format!("r_debug_view {mode}"),
                Some(EngineCommand::DebugView(mode)),
                vec![format!("Debug view set to: {word} ({mode})")],
            ));
        }
        for (arg, reply) in [
            ("6", "Debug view must be between 0 and 5"),
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
        assert_eq!(help.len(), 4, "{help:?}");
        assert_eq!(help.iter().filter(|l| l.starts_with("  quit")).count(), 1);
        assert_eq!(help[3], "  tgm [0|1] - toggle god mode");
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
