//! The developer console: a command registry, an output buffer and the open/closed state.
//!
//! Commands are hidden debug commands: they appear only here and in `help`, never in a
//! Settings screen.

pub mod output;
pub mod registry;
mod view;

use std::collections::BTreeMap;

use moho_input::bindings::{Action, ActionBindings, Binding, BindingWarning};
use moho_input::key::Key;

use output::ConsoleOutput;
use registry::{CommandSpec, EngineCommand, Handler, RegisterError, Registry};

/// The reserved `[bindings]` entry that names the console key.
pub const CONSOLE_BINDING: &str = "console";

/// The console key as an action, so it loads like any other binding. It is no game's action:
/// no keybind screen lists it, and it is rebound only in the prefs file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ConsoleKey;

impl Action for ConsoleKey {
    const ALL: &'static [Self] = &[ConsoleKey];

    fn name(self) -> &'static str {
        CONSOLE_BINDING
    }

    fn default_bindings(self) -> &'static [Binding] {
        &[Binding::Key(Key::Backtick)]
    }
}

const ESCAPE: Binding = Binding::Key(Key::Escape);

/// Where an input should go after the console has seen it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Routing {
    /// The console is closed and did not take the input: the game gets it.
    Game,
    /// The console is open and takes the input.
    Console,
    /// The input opened the console.
    Opened,
    /// The input closed the console.
    Closed,
}

/// The console for a game whose own commands are `G`.
pub struct Console<G> {
    open: bool,
    toggle: Vec<Binding>,
    input: String,
    output: ConsoleOutput,
    registry: Registry<G>,
    commands: Vec<G>,
    engine_commands: Vec<EngineCommand>,
}

impl<G> Console<G> {
    /// Reads only the `console` entry of the game's `[bindings]` section. The console always has
    /// a key: an entry that is unbound or unreadable falls back to backtick, with a warning.
    pub fn new(bindings: &BTreeMap<String, String>) -> (Self, Vec<BindingWarning>) {
        let entry = bindings.get(CONSOLE_BINDING);
        let section: BTreeMap<String, String> = entry
            .map(|value| (CONSOLE_BINDING.to_owned(), value.clone()))
            .into_iter()
            .collect();
        let (loaded, mut warnings) = ActionBindings::<ConsoleKey>::load(&section);
        let mut toggle = loaded.get(ConsoleKey).to_vec();
        if toggle.is_empty() {
            toggle = ConsoleKey.default_bindings().to_vec();
            if let Some(value) = entry {
                warnings.push(BindingWarning {
                    name: CONSOLE_BINDING.to_owned(),
                    value: value.clone(),
                });
            }
        }
        let console = Self {
            open: false,
            toggle,
            input: String::new(),
            output: ConsoleOutput::new(),
            registry: Registry::new(),
            commands: Vec::new(),
            engine_commands: Vec::new(),
        };
        (console, warnings)
    }

    /// Adds a game command; a refused registration changes nothing.
    ///
    /// # Errors
    /// `Taken` if the name (ignoring case) is an engine or earlier game command; `InvalidName`
    /// if it is empty or holds whitespace.
    pub fn register(&mut self, spec: CommandSpec<G>) -> Result<(), RegisterError> {
        self.registry.register(spec)
    }

    /// Routes one key or button event: a non-repeat press of the console binding toggles; while
    /// open, Escape closes and everything else belongs to the console.
    pub fn on_key(&mut self, input: Binding, pressed: bool, repeat: bool) -> Routing {
        let press = pressed && !repeat;
        if press && self.open && (self.toggle.contains(&input) || input == ESCAPE) {
            self.close();
            Routing::Closed
        } else if press && !self.open && self.toggle.contains(&input) {
            self.open();
            Routing::Opened
        } else if self.open {
            Routing::Console
        } else {
            Routing::Game
        }
    }

    /// Runs one typed line: echoes it, then runs the command named by its first word. A game
    /// command that parses is queued for `take_commands`; its reply, or the parse error, prints.
    pub fn submit(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        self.output.add_line(format!("> {line}"));
        self.output.add_to_history(line.to_owned());
        let mut words = line.split_whitespace();
        let name = words.next().unwrap_or_default();
        let args: Vec<&str> = words.collect();
        match self.registry.find(name) {
            None => self.output.add_line(format!(
                "Unknown command '{name}'. Type 'help' for a list of commands."
            )),
            Some(Handler::Help) => {
                for entry in self.registry.entries() {
                    self.output
                        .add_line(format!("  {} - {}", entry.usage, entry.help));
                }
            }
            Some(Handler::Clear) => self.output.clear_output(),
            Some(Handler::Engine(parse)) => {
                run(parse(&args), &mut self.engine_commands, &mut self.output);
            }
            Some(Handler::Game(parse)) => run(parse(&args), &mut self.commands, &mut self.output),
        }
    }

    /// Drains the game's commands queued by `submit`. A game calls this in `Game::command()` and
    /// carries the values in its tick command; the console never mutates game state.
    pub fn take_commands(&mut self) -> Vec<G> {
        std::mem::take(&mut self.commands)
    }

    /// Prints text to the output, one line per line of text, as a command reply does. For a game
    /// to report a result it knows only after draining `take_commands`. Touches output only.
    pub fn print(&mut self, text: &str) {
        print_reply(&mut self.output, text);
    }

    /// Drains the engine's commands queued by `submit`.
    pub fn take_engine_commands(&mut self) -> Vec<EngineCommand> {
        std::mem::take(&mut self.engine_commands)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// The text on the input line.
    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn output(&self) -> &ConsoleOutput {
        &self.output
    }
}

/// Queues a parsed command and prints its reply, or prints only the error reply.
fn run<C>(parsed: Result<(C, String), String>, queue: &mut Vec<C>, output: &mut ConsoleOutput) {
    let reply = match parsed {
        Ok((command, reply)) => {
            queue.push(command);
            reply
        }
        Err(reply) => reply,
    };
    print_reply(output, &reply);
}

/// Prints a reply, one output line per line of text.
fn print_reply(output: &mut ConsoleOutput, reply: &str) {
    for line in reply.lines() {
        output.add_line(line.to_owned());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use moho_app::{Game, HeadlessLoop, LoopConfig, TickContext};
    use moho_input::bindings::Binding;
    use moho_input::key::{Key, MouseButton};

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) enum Cmd {
        God(bool),
        Args(Vec<String>),
    }

    pub(crate) fn parse_tgm(args: &[&str]) -> Result<(Cmd, String), String> {
        match args {
            ["1"] => Ok((Cmd::God(true), "god mode on".to_owned())),
            ["0"] => Ok((Cmd::God(false), "god mode off".to_owned())),
            _ => Err(format!("tgm: expected 0 or 1, got {args:?}")),
        }
    }

    fn parse_args(args: &[&str]) -> Result<(Cmd, String), String> {
        let args = args.iter().map(|&a| a.to_owned()).collect();
        Ok((Cmd::Args(args), "ok".to_owned()))
    }

    fn parse_multiline(_: &[&str]) -> Result<(Cmd, String), String> {
        Ok((Cmd::God(true), "first\nsecond\nthird".to_owned()))
    }

    fn parse_silent(_: &[&str]) -> Result<(Cmd, String), String> {
        Ok((Cmd::God(true), String::new()))
    }

    pub(crate) fn tgm() -> CommandSpec<Cmd> {
        CommandSpec {
            name: "tgm".to_owned(),
            usage: "tgm [0|1]".to_owned(),
            help: "toggle god mode".to_owned(),
            parse: parse_tgm,
        }
    }

    fn console_with(spec: CommandSpec<Cmd>) -> Console<Cmd> {
        let (mut console, _) = Console::new(&BTreeMap::new());
        console.register(spec).unwrap();
        console
    }

    fn lines(console: &Console<Cmd>) -> Vec<String> {
        console.output().lines().iter().cloned().collect()
    }

    fn last_lines(console: &Console<Cmd>, count: usize) -> Vec<String> {
        let all = lines(console);
        all[all.len().saturating_sub(count)..].to_vec()
    }

    /// The lines the last `help` printed, after its echo.
    pub(crate) fn help_lines(console: &Console<Cmd>) -> Vec<String> {
        let all = lines(console);
        all.iter()
            .rposition(|l| l == "> help")
            .map_or_else(Vec::new, |at| all[at + 1..].to_vec())
    }

    const BACKTICK: Binding = Binding::Key(Key::Backtick);
    const ESCAPE: Binding = Binding::Key(Key::Escape);

    fn bindings(entry: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(CONSOLE_BINDING.to_owned(), entry.to_owned())])
    }

    #[test]
    fn console_key_opens_then_closes() {
        let (mut console, warnings) = Console::<Cmd>::new(&BTreeMap::new());

        let opened = console.on_key(BACKTICK, true, false);
        let open_after = console.is_open();
        let closed = console.on_key(BACKTICK, true, false);

        assert_eq!(warnings, vec![]);
        assert_eq!(opened, Routing::Opened);
        assert!(open_after);
        assert_eq!(closed, Routing::Closed);
        assert!(!console.is_open());
    }

    #[test]
    fn a_closed_console_leaves_every_other_input_to_the_game() {
        let (mut console, _) = Console::<Cmd>::new(&BTreeMap::new());

        assert_eq!(
            console.on_key(Binding::Key(Key::W), true, false),
            Routing::Game
        );
        assert_eq!(console.on_key(ESCAPE, true, false), Routing::Game);
        assert_eq!(console.on_key(BACKTICK, false, false), Routing::Game);
        assert!(!console.is_open());
    }

    #[test]
    fn an_open_console_takes_every_other_input() {
        let (mut console, _) = Console::<Cmd>::new(&BTreeMap::new());
        console.on_key(BACKTICK, true, false);

        assert_eq!(
            console.on_key(Binding::Key(Key::W), true, false),
            Routing::Console
        );
        assert_eq!(
            console.on_key(Binding::Mouse(MouseButton::Left), true, false),
            Routing::Console
        );
        assert_eq!(console.on_key(BACKTICK, false, false), Routing::Console);
        assert!(console.is_open());
    }

    #[test]
    fn escape_closes_an_open_console() {
        let (mut console, _) = Console::<Cmd>::new(&BTreeMap::new());
        console.on_key(BACKTICK, true, false);

        let routing = console.on_key(ESCAPE, true, false);

        assert_eq!(routing, Routing::Closed);
        assert!(!console.is_open());
    }

    #[test]
    fn a_repeated_console_key_does_not_toggle() {
        let (mut console, _) = Console::<Cmd>::new(&BTreeMap::new());

        let while_closed = console.on_key(BACKTICK, true, true);
        let closed_state = console.is_open();
        console.on_key(BACKTICK, true, false);
        let while_open = console.on_key(BACKTICK, true, true);

        assert_eq!(while_closed, Routing::Game);
        assert!(!closed_state);
        assert_eq!(while_open, Routing::Console);
        assert!(console.is_open());
    }

    #[test]
    fn a_bound_key_toggles_and_the_default_no_longer_does() {
        let (mut console, warnings) = Console::<Cmd>::new(&bindings("Backslash"));
        let backslash = Binding::Key(Key::Backslash);

        let default_key = console.on_key(BACKTICK, true, false);
        let rebound = console.on_key(backslash, true, false);
        let closed = console.on_key(backslash, true, false);

        assert_eq!(warnings, vec![]);
        assert_eq!(default_key, Routing::Game);
        assert_eq!(rebound, Routing::Opened);
        assert_eq!(closed, Routing::Closed);
    }

    #[test]
    fn an_unbound_or_bad_console_entry_falls_back_to_backtick() {
        for entry in ["Unbound", "", "Nonsense"] {
            let (mut console, warnings) = Console::<Cmd>::new(&bindings(entry));

            let routing = console.on_key(BACKTICK, true, false);

            assert_eq!(routing, Routing::Opened, "{entry:?}");
            assert_eq!(warnings.len(), 1, "{entry:?}: {warnings:?}");
            assert_eq!(warnings[0].name, CONSOLE_BINDING, "{entry:?}");
            assert_eq!(warnings[0].value, entry, "{entry:?}");
        }
    }

    #[test]
    fn entries_other_than_console_are_the_games_and_are_not_read() {
        let section = BTreeMap::from([
            ("jump".to_owned(), "Nonsense".to_owned()),
            ("fire".to_owned(), "Backtick".to_owned()),
        ]);

        let (mut console, warnings) = Console::<Cmd>::new(&section);
        let routing = console.on_key(BACKTICK, true, false);

        assert_eq!(warnings, vec![]);
        assert_eq!(routing, Routing::Opened);
    }

    #[test]
    fn help_lists_engine_and_game_commands_with_usage_and_help() {
        let mut console = console_with(tgm());

        console.submit("help");

        let help = help_lines(&console);
        assert_eq!(help.len(), 8, "{help:?}");
        let names = [
            "help",
            "clear",
            "quit",
            "r_debug_view",
            "r_shadow_quality",
            "r_ssao_quality",
            "audio_cue",
            "tgm",
        ];
        for (line, name) in help.iter().zip(names) {
            assert!(line.starts_with(&format!("  {name}")), "{line:?}");
            assert!(line.contains(" - "), "{line:?}");
        }
        assert_eq!(help[7], "  tgm [0|1] - toggle god mode");
    }

    #[test]
    fn a_game_command_reaches_the_game_on_its_next_tick() {
        struct TestGame {
            console: Console<Cmd>,
            god: bool,
        }
        impl Game for TestGame {
            type Command = Vec<Cmd>;

            fn command(&mut self) -> Vec<Cmd> {
                self.console.take_commands()
            }

            fn tick(&mut self, _ctx: &mut TickContext<'_>, commands: &Vec<Cmd>) {
                for command in commands {
                    if let Cmd::God(on) = command {
                        self.god = *on;
                    }
                }
            }

            fn frame(&mut self, _ctx: &mut moho_app::FrameContext<'_>, _alpha: f32) {}
        }
        let mut game = TestGame {
            console: console_with(tgm()),
            god: false,
        };
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));

        game.console.submit("tgm 1");
        let printed = lines(&game.console);
        let before = game.god;
        headless.step(&mut game, 1);

        assert!(printed.contains(&"> tgm 1".to_owned()), "{printed:?}");
        assert!(printed.contains(&"god mode on".to_owned()), "{printed:?}");
        assert!(!before);
        assert!(game.god);
    }

    #[test]
    fn a_parse_error_prints_and_queues_nothing() {
        let mut console = console_with(tgm());

        console.submit("tgm banana");

        let printed = lines(&console);
        assert!(
            printed.contains(&"tgm: expected 0 or 1, got [\"banana\"]".to_owned()),
            "{printed:?}"
        );
        assert!(console.take_commands().is_empty());
        assert!(console.take_engine_commands().is_empty());
    }

    #[test]
    fn a_line_with_surrounding_whitespace_runs_the_command_with_its_arguments() {
        let mut console = console_with(tgm());

        console.submit("   tgm 1  ");

        assert_eq!(console.take_commands(), vec![Cmd::God(true)]);
        assert!(console.take_engine_commands().is_empty());
    }

    #[test]
    fn unknown_command_names_it_and_suggests_help() {
        let mut console = console_with(tgm());

        console.submit("frobnicate");

        let printed = lines(&console);
        assert!(
            printed.contains(
                &"Unknown command 'frobnicate'. Type 'help' for a list of commands.".to_owned()
            ),
            "{printed:?}"
        );
    }

    #[test]
    fn names_ignore_case_and_args_split_on_whitespace() {
        let table: [(&str, &[&str]); 3] = [("tgm 1", &["1"]), ("TGM   1", &["1"]), ("tgm", &[])];

        for (line, expected) in table {
            let mut console = console_with(CommandSpec {
                parse: parse_args,
                ..tgm()
            });

            console.submit(line);

            let expected: Vec<String> = expected.iter().map(|&a| a.to_owned()).collect();
            assert_eq!(
                console.take_commands(),
                vec![Cmd::Args(expected)],
                "{line:?}"
            );
        }
    }

    #[test]
    fn clear_empties_the_output() {
        let mut console = console_with(tgm());
        console.submit("tgm 1");
        assert!(!lines(&console).is_empty());

        console.submit("clear");

        assert_eq!(lines(&console), Vec::<String>::new());
    }

    #[test]
    fn quit_queues_the_engine_quit_command() {
        let mut console = console_with(tgm());

        console.submit("QUIT");

        assert_eq!(console.take_engine_commands(), vec![EngineCommand::Quit]);
        assert!(console.take_commands().is_empty());
    }

    #[test]
    fn a_blank_line_does_nothing() {
        let mut console = console_with(tgm());
        let before = lines(&console);

        console.submit("   \t ");
        console.submit("");

        assert_eq!(lines(&console), before);
        assert!(console.take_commands().is_empty());
    }

    #[test]
    fn a_reply_prints_one_line_per_line() {
        let mut console = console_with(CommandSpec {
            parse: parse_multiline,
            ..tgm()
        });

        console.submit("tgm");

        let printed = lines(&console);
        let at = printed.iter().position(|l| l == "first");
        assert!(at.is_some(), "{printed:?}");
        assert_eq!(
            &printed[at.unwrap_or_default()..],
            ["first", "second", "third"]
        );
    }

    #[test]
    fn a_game_prints_a_line_after_draining_its_command() {
        let mut console = console_with(CommandSpec {
            parse: parse_silent,
            ..tgm()
        });
        console.submit("tgm");

        let drained = console.take_commands();
        console.print("god mode applied");

        assert_eq!(drained, vec![Cmd::God(true)]);
        assert_eq!(last_lines(&console, 2), ["> tgm", "god mode applied"]);
    }

    #[test]
    fn a_printed_text_is_one_output_line_per_line() {
        let mut console = console_with(tgm());

        console.print("first\nsecond");

        assert_eq!(last_lines(&console, 2), ["first", "second"]);
    }

    #[test]
    fn an_empty_reply_prints_nothing_and_still_queues_the_command() {
        let mut console = console_with(CommandSpec {
            parse: parse_silent,
            ..tgm()
        });

        let before = lines(&console).len();

        console.submit("tgm");

        assert_eq!(lines(&console).len(), before + 1);
        assert_eq!(last_lines(&console, 1), ["> tgm"]);
        assert_eq!(console.take_commands(), vec![Cmd::God(true)]);
    }

    #[test]
    fn a_line_printed_while_closed_is_in_the_output_when_opened() {
        let mut console = console_with(tgm());
        assert!(!console.is_open());

        console.print("while closed");
        console.open();

        assert!(console.is_open());
        assert_eq!(last_lines(&console, 1), ["while closed"]);
    }

    #[test]
    fn printed_lines_respect_the_output_cap() {
        let mut console = console_with(tgm());

        for i in 0..25 {
            console.print(&format!("line {i}"));
        }

        let expected: Vec<String> = (5..25).map(|i| format!("line {i}")).collect();
        assert_eq!(lines(&console), expected);
    }
}
