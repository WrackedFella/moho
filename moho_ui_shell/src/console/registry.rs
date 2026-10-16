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
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EngineCommand {
    Quit,
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
