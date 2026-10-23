//! Action-keyed bindings: what a game declares, and how they persist.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use crate::key::{Key, MouseButton};
use crate::pad::PadInput;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Key(Key),
    Mouse(MouseButton),
    Pad(PadInput),
}

impl Input {
    pub fn is_pad(self) -> bool {
        matches!(self, Input::Pad(_))
    }

    /// Persisted name, e.g. `W` or `Mouse Left`.
    pub fn name(self) -> &'static str {
        match self {
            Input::Key(key) => key.name(),
            Input::Mouse(button) => button.name(),
            Input::Pad(input) => input.name(),
        }
    }

    /// Text shown to the player, e.g. `Spacebar` or `Mouse Left`.
    pub fn label(self) -> &'static str {
        match self {
            Input::Key(key) => key.label(),
            Input::Mouse(button) => button.name(),
            Input::Pad(input) => input.name(),
        }
    }
}

/// When a binding fires: on press, or once its input has been down for the action's hold threshold.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Trigger {
    Press,
    Hold,
}

/// A set of Ctrl, Shift and Alt that must be held with a binding's input.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1);
    pub const SHIFT: Self = Self(2);
    pub const ALT: Self = Self(4);

    /// Every modifier in `self` or `other`.
    pub const fn and(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Every modifier of `other` is in `self`.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// The modifier `key` is, or `NONE` for any other key.
    pub(crate) const fn of_key(key: Key) -> Self {
        match key {
            Key::Ctrl => Self::CTRL,
            Key::Shift => Self::SHIFT,
            Key::Alt => Self::ALT,
            _ => Self::NONE,
        }
    }

    /// Orders sets for chord resolution: more modifiers first, then Ctrl before Shift before
    /// Alt. The greater rank wins; Ctrl is the lowest bit, so inverted bits rank it first.
    pub(crate) const fn rank(self) -> (u32, u8) {
        (self.0.count_ones(), !self.0)
    }
}

/// Modifier parts in canonical text order.
const MODIFIER_NAMES: [(Modifiers, &str); 3] = [
    (Modifiers::CTRL, "Ctrl"),
    (Modifiers::SHIFT, "Shift"),
    (Modifiers::ALT, "Alt"),
];

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub input: Input,
    pub trigger: Trigger,
    pub modifiers: Modifiers,
}

impl Binding {
    pub const fn key(key: Key) -> Self {
        Self::press(Input::Key(key))
    }

    pub const fn mouse(button: MouseButton) -> Self {
        Self::press(Input::Mouse(button))
    }

    pub const fn pad(input: PadInput) -> Self {
        Self::press(Input::Pad(input))
    }

    const fn press(input: Input) -> Self {
        Self {
            input,
            trigger: Trigger::Press,
            modifiers: Modifiers::NONE,
        }
    }

    /// The same input, firing only while `modifiers` are held with it.
    pub const fn with(self, modifiers: Modifiers) -> Self {
        Self { modifiers, ..self }
    }

    /// The same input, firing once it has been down for the action's hold threshold.
    pub const fn held(self) -> Self {
        Self {
            trigger: Trigger::Hold,
            ..self
        }
    }

    pub fn is_pad(self) -> bool {
        self.input.is_pad()
    }

    /// Fires on press with no modifiers: the only kind a raw key event can match.
    pub fn is_plain(self) -> bool {
        self == Self::press(self.input)
    }

    /// Enter, Escape or an arrow key with no modifiers, on any trigger: menus and dialogs
    /// read these directly, so no action may sit on one alone. A chord on them is allowed.
    pub fn is_protected(self) -> bool {
        self.modifiers == Modifiers::NONE
            && matches!(
                self.input,
                Input::Key(
                    Key::Enter
                        | Key::Escape
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                )
            )
    }

    /// Persisted name: the input's, prefixed by `Hold ` for a hold binding.
    pub fn name(self) -> String {
        self.decorate(self.input.name())
    }

    /// Text shown to the player: the input's label, prefixed by `Hold ` for a hold binding.
    pub fn label(self) -> String {
        self.decorate(self.input.label())
    }

    fn decorate(self, text: &str) -> String {
        let mut out = String::new();
        if self.trigger == Trigger::Hold {
            out.push_str(HOLD_PREFIX);
        }
        for (modifier, name) in MODIFIER_NAMES {
            if self.modifiers.contains(modifier) {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(text);
        out
    }
}

impl From<Input> for Binding {
    fn from(input: Input) -> Self {
        Self::press(input)
    }
}

/// Ticks an input must stay down before a `Hold` binding fires, unless the game overrides it.
pub const DEFAULT_HOLD_TICKS: u32 = 18;

/// A game's action enum: stable names and default bindings, as data.
pub trait Action: Copy + Eq + Hash + Debug + 'static {
    const ALL: &'static [Self];

    /// Stable id used as the persisted key; never renamed once shipped. Lowercase
    /// `snake_case`: the INI reader lowercases keys, so other ids would not load back.
    fn name(self) -> &'static str;

    fn default_bindings(self) -> &'static [Binding];

    /// Ticks an input must stay down before a `Hold` binding fires, one value per game; at least 1.
    const HOLD_TICKS: u32 = DEFAULT_HOLD_TICKS;

    /// Whether the player may choose between holding the input and toggling the action.
    fn toggleable(self) -> bool {
        false
    }
}

/// How the player drives a toggleable action: held while the input is down, or latched
/// on and off by presses.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ActionMode {
    #[default]
    Hold,
    Toggle,
}

impl ActionMode {
    /// The name written to prefs.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Toggle => "toggle",
        }
    }

    /// Reads a prefs value, ignoring case and surrounding whitespace.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        [Self::Hold, Self::Toggle]
            .into_iter()
            .find(|mode| value.eq_ignore_ascii_case(mode.name()))
    }

    pub const fn is_toggle(self) -> bool {
        matches!(self, Self::Toggle)
    }

    pub const fn from_toggle(toggle: bool) -> Self {
        if toggle { Self::Toggle } else { Self::Hold }
    }
}

/// A line of the `[bindings]` section that could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingWarning {
    /// The line's key as written: a declared action with a bad value, or an unknown action.
    pub name: String,
    pub value: String,
}

/// Every action's current bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionBindings<A: Action> {
    /// Holds every action in `A::ALL`; `defaults` fills it and `set` only replaces.
    map: HashMap<A, Vec<Binding>>,
    /// Only toggleable actions set to `Toggle` appear; every other action reads as `Hold`.
    toggled: HashSet<A>,
}

impl<A: Action> ActionBindings<A> {
    fn defaults() -> Self {
        Self {
            map: A::ALL
                .iter()
                .map(|&a| (a, a.default_bindings().to_vec()))
                .collect(),
            toggled: HashSet::new(),
        }
    }

    /// Reads the raw `[bindings]` section. Absent actions and unusable lines keep
    /// their defaults; each unusable line, including one holding a protected binding, yields
    /// a warning.
    pub fn load(section: &BTreeMap<String, String>) -> (Self, Vec<BindingWarning>) {
        let mut loaded = Self::defaults();
        let mut warnings = Vec::new();
        for (name, value) in section {
            let action = A::ALL.iter().copied().find(|a| a.name() == name);
            let parsed = action.and_then(|_| parse_list(value));
            match (action, parsed) {
                (Some(action), Some(bindings)) if !bindings.iter().any(|b| b.is_protected()) => {
                    loaded.map.insert(action, bindings);
                }
                _ => warnings.push(BindingWarning {
                    name: name.clone(),
                    value: value.clone(),
                }),
            }
        }
        (loaded, warnings)
    }

    /// Writes every action, including those left at their defaults.
    pub fn to_section(&self) -> BTreeMap<String, String> {
        self.map
            .iter()
            .map(|(action, bindings)| (action.name().to_string(), format_list(bindings)))
            .collect()
    }

    /// An action missing from `A::ALL` reads as unbound.
    pub fn get(&self, action: A) -> &[Binding] {
        self.map.get(&action).map_or(&[], Vec::as_slice)
    }

    pub fn set(&mut self, action: A, bindings: Vec<Binding>) {
        self.map.insert(action, bindings);
    }

    /// `Hold` unless the action is toggleable and set to `Toggle`.
    pub fn mode(&self, action: A) -> ActionMode {
        ActionMode::from_toggle(self.toggled.contains(&action))
    }

    /// Setting `Toggle` on a non-toggleable action is ignored: such an action has no toggle.
    pub fn set_mode(&mut self, action: A, mode: ActionMode) {
        if !action.toggleable() {
            return;
        }
        match mode {
            ActionMode::Toggle => self.toggled.insert(action),
            ActionMode::Hold => self.toggled.remove(&action),
        };
    }

    /// Reads the raw `[action_modes]` section. Each unusable line (unknown value, unknown
    /// action, or an action that is not toggleable) yields a warning and leaves that action
    /// in `Hold`.
    pub fn load_modes(&mut self, section: &BTreeMap<String, String>) -> Vec<BindingWarning> {
        let mut warnings = Vec::new();
        for (name, value) in section {
            let action = A::ALL
                .iter()
                .copied()
                .find(|a| a.name() == name && a.toggleable());
            let mode = ActionMode::parse(value);
            match (action, mode) {
                (Some(action), Some(mode)) => self.set_mode(action, mode),
                _ => warnings.push(BindingWarning {
                    name: name.clone(),
                    value: value.clone(),
                }),
            }
        }
        warnings
    }

    /// Writes every toggleable action, including those left at `Hold`.
    pub fn modes_to_section(&self) -> BTreeMap<String, String> {
        A::ALL
            .iter()
            .filter(|a| a.toggleable())
            .map(|&a| (a.name().to_string(), self.mode(a).name().to_string()))
            .collect()
    }
}

const UNBOUND: &str = "Unbound";

/// `None` if any item is not a known input, optionally after `Hold `; empty or `Unbound` is an empty list.
fn parse_list(value: &str) -> Option<Vec<Binding>> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case(UNBOUND) {
        return Some(Vec::new());
    }
    value
        .split(',')
        .map(|item| parse_binding(item.trim()))
        .collect()
}

const HOLD_PREFIX: &str = "Hold ";

fn parse_binding(item: &str) -> Option<Binding> {
    if let Some(prefix) = item.get(..HOLD_PREFIX.len())
        && prefix.eq_ignore_ascii_case(HOLD_PREFIX)
    {
        return parse_chord(item[HOLD_PREFIX.len()..].trim_start()).map(Binding::held);
    }
    parse_chord(item)
}

/// `{Ctrl+}{Shift+}{Alt+}<input>` in any order and case. No input name contains `+`.
fn parse_chord(item: &str) -> Option<Binding> {
    let Some((prefix, input)) = item.rsplit_once('+') else {
        return parse_input(item);
    };
    let modifiers = parse_modifiers(prefix)?;
    let binding = parse_input(input)?;
    let own = match binding.input {
        Input::Key(key) => Modifiers::of_key(key),
        Input::Mouse(_) => Modifiers::NONE,
        Input::Pad(_) => return None,
    };
    (!modifiers.intersects(own)).then(|| binding.with(modifiers))
}

fn parse_modifiers(prefix: &str) -> Option<Modifiers> {
    prefix.split('+').try_fold(Modifiers::NONE, |set, part| {
        let (modifier, _) = MODIFIER_NAMES
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(part))?;
        (!set.contains(*modifier)).then(|| set.and(*modifier))
    })
}

fn parse_input(item: &str) -> Option<Binding> {
    Key::parse(item)
        .map(Binding::key)
        .or_else(|| MouseButton::parse(item).map(Binding::mouse))
        .or_else(|| PadInput::parse(item).map(Binding::pad))
}

fn format_list(bindings: &[Binding]) -> String {
    if bindings.is_empty() {
        return UNBOUND.to_string();
    }
    bindings
        .iter()
        .map(|binding| binding.name())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::{PadButton, Stick, StickDir};
    use moho_core::prefs::Prefs;

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum TestAction {
        MoveForward,
        Jump,
    }

    impl Action for TestAction {
        const ALL: &'static [Self] = &[TestAction::MoveForward, TestAction::Jump];

        fn name(self) -> &'static str {
            match self {
                TestAction::MoveForward => "move_forward",
                TestAction::Jump => "jump",
            }
        }

        fn default_bindings(self) -> &'static [Binding] {
            match self {
                TestAction::MoveForward => const { &[Binding::key(Key::W)] },
                TestAction::Jump => const { &[Binding::key(Key::Space)] },
            }
        }

        fn toggleable(self) -> bool {
            self == TestAction::Jump
        }
    }

    fn section(lines: &[(&str, &str)]) -> BTreeMap<String, String> {
        lines
            .iter()
            .map(|&(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn assert_defaults_except(bindings: &ActionBindings<TestAction>, skip: &[TestAction]) {
        for &action in TestAction::ALL.iter().filter(|a| !skip.contains(a)) {
            assert_eq!(
                bindings.get(action),
                action.default_bindings(),
                "{}",
                action.name()
            );
        }
    }

    fn keys(keys: &[Key]) -> Vec<Binding> {
        keys.iter().map(|&k| Binding::key(k)).collect()
    }

    #[test]
    fn rebinding_survives_save_and_reload() {
        let cases: [Vec<Key>; 8] = [
            vec![Key::F],
            vec![],
            vec![Key::F, Key::Space],
            vec![Key::Comma],
            vec![Key::Semicolon],
            vec![Key::Equals],
            vec![Key::LeftBracket],
            vec![Key::Backslash],
        ];

        for rebound in cases {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("prefs.ini");
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            bindings.set(TestAction::Jump, keys(&rebound));
            let mut prefs = Prefs::default();
            prefs.set_bindings(bindings.to_section());
            std::fs::write(&path, prefs.to_ini_string()).unwrap();

            let (reloaded, prefs_warnings) = Prefs::load_from(&path);
            let (restored, binding_warnings) =
                ActionBindings::<TestAction>::load(reloaded.bindings());

            assert_eq!(prefs_warnings, vec![], "{rebound:?}");
            assert_eq!(binding_warnings, vec![], "{rebound:?}");
            assert_eq!(
                restored.get(TestAction::Jump),
                keys(&rebound),
                "{rebound:?}"
            );
            assert_defaults_except(&restored, &[TestAction::Jump]);
        }
    }

    #[test]
    fn only_pad_inputs_are_pad_bindings() {
        let pad = Binding::pad(PadInput::Button(PadButton::West));
        assert!(pad.is_pad());
        assert!(pad.held().is_pad());
        assert!(!Binding::key(Key::R).is_pad());
        assert!(!Binding::mouse(MouseButton::Right).held().is_pad());
    }

    #[test]
    fn label_is_the_key_label_or_the_mouse_button_name() {
        assert_eq!(Binding::key(Key::Space).label(), "Spacebar");
        assert_eq!(Binding::mouse(MouseButton::Left).label(), "Mouse Left");
    }

    #[test]
    fn mouse_bindings_round_trip_by_name() {
        let cases = [
            (MouseButton::Left, "Mouse Left"),
            (MouseButton::Right, "Mouse Right"),
            (MouseButton::Middle, "Mouse Middle"),
        ];

        for (button, name) in cases {
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            bindings.set(
                TestAction::Jump,
                vec![Binding::key(Key::F), Binding::mouse(button)],
            );

            let written = bindings.to_section();
            let (restored, warnings) = ActionBindings::<TestAction>::load(&written);

            assert_eq!(written["jump"], format!("F, {name}"));
            assert_eq!(warnings, vec![], "{name}");
            assert_eq!(
                restored.get(TestAction::Jump),
                [Binding::key(Key::F), Binding::mouse(button)],
                "{name}"
            );
        }
    }

    #[test]
    fn pad_bindings_round_trip_through_names() {
        let mut cases: Vec<(PadInput, String)> = PadButton::ALL
            .iter()
            .map(|&b| (PadInput::Button(b), format!("Pad {b:?}")))
            .collect();
        for &stick in Stick::ALL {
            for &dir in StickDir::ALL {
                cases.push((
                    PadInput::Stick(stick, dir),
                    format!("Pad {stick:?} {dir:?}"),
                ));
            }
        }
        assert_eq!(cases.len(), 17 + 8);

        for (input, name) in cases {
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            bindings.set(
                TestAction::Jump,
                vec![Binding::key(Key::F), Binding::pad(input)],
            );

            let written = bindings.to_section();
            let (restored, warnings) = ActionBindings::<TestAction>::load(&written);

            assert_eq!(written["jump"], format!("F, {name}"));
            assert_eq!(warnings, vec![], "{name}");
            assert_eq!(
                restored.get(TestAction::Jump),
                [Binding::key(Key::F), Binding::pad(input)],
                "{name}"
            );
            assert_eq!(Binding::pad(input).name(), name);
        }
    }

    #[test]
    fn pad_names_are_pinned_and_unknown_pad_names_warn() {
        let (bindings, warnings) = ActionBindings::<TestAction>::load(&section(&[
            ("jump", "Pad South, Pad DPadUp"),
            ("move_forward", "Pad LeftStick Up"),
        ]));

        assert_eq!(warnings, vec![]);
        assert_eq!(
            bindings.get(TestAction::Jump),
            [
                Binding::pad(PadInput::Button(PadButton::South)),
                Binding::pad(PadInput::Button(PadButton::DPadUp)),
            ]
        );
        assert_eq!(
            bindings.get(TestAction::MoveForward),
            [Binding::pad(PadInput::Stick(
                Stick::LeftStick,
                StickDir::Up
            ))]
        );
        for bad in [
            "Pad Banana",
            "Pad LeftStick",
            "Pad LeftStick Sideways",
            "Pad",
        ] {
            let (_, warnings) = ActionBindings::<TestAction>::load(&section(&[("jump", bad)]));
            assert_eq!(warnings.len(), 1, "{bad}");
        }
    }

    #[test]
    fn to_section_writes_every_action_by_name() {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
        bindings.set(TestAction::Jump, keys(&[Key::F]));

        let written = bindings.to_section();

        assert_eq!(written, section(&[("move_forward", "W"), ("jump", "F")]));
    }

    #[test]
    fn missing_action_gets_its_default() {
        let cases = [section(&[]), section(&[("move_forward", "Z")])];

        for raw in cases {
            let (bindings, warnings) = ActionBindings::<TestAction>::load(&raw);

            assert_eq!(warnings, vec![], "{raw:?}");
            assert_eq!(
                bindings.get(TestAction::Jump),
                [Binding::key(Key::Space)],
                "{raw:?}"
            );
        }
        let (bindings, _) = ActionBindings::<TestAction>::load(&section(&[("jump", "F")]));
        assert_eq!(
            bindings.get(TestAction::MoveForward),
            [Binding::key(Key::W)]
        );
    }

    #[test]
    fn multi_key_value_tolerates_whitespace() {
        let (bindings, warnings) =
            ActionBindings::<TestAction>::load(&section(&[("jump", "F ,  Space")]));

        assert_eq!(warnings, vec![]);
        assert_eq!(bindings.get(TestAction::Jump), keys(&[Key::F, Key::Space]));
    }

    type BadLineCase<'a> = (
        &'a [(&'a str, &'a str)],
        &'a [&'a str],
        &'a [Key],
        &'a [Key],
    );

    #[test]
    fn bad_line_is_reported_and_default_kept() {
        // (lines, reported names, move_forward after load, jump after load)
        let cases: [BadLineCase; 4] = [
            (&[("jump", "Banana")], &["jump"], &[Key::W], &[Key::Space]),
            (
                &[("teleport", "T")],
                &["teleport"],
                &[Key::W],
                &[Key::Space],
            ),
            (
                &[("move_forward", "Z"), ("jump", "Banana")],
                &["jump"],
                &[Key::Z],
                &[Key::Space],
            ),
            (
                &[("jump", "F, Banana")],
                &["jump"],
                &[Key::W],
                &[Key::Space],
            ),
        ];

        for (lines, reported, forward, jump) in cases {
            let (bindings, warnings) = ActionBindings::<TestAction>::load(&section(lines));

            let names: Vec<_> = warnings.iter().map(|w| w.name.as_str()).collect();
            assert_eq!(names, reported, "{lines:?}");
            assert_eq!(
                bindings.get(TestAction::MoveForward),
                keys(forward),
                "{lines:?}"
            );
            assert_eq!(bindings.get(TestAction::Jump), keys(jump), "{lines:?}");
        }
    }

    #[test]
    fn unknown_action_line_does_not_disturb_valid_lines() {
        let raw = section(&[("jump", "F"), ("teleport", "T")]);

        let (bindings, warnings) = ActionBindings::<TestAction>::load(&raw);

        let names: Vec<_> = warnings.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["teleport"]);
        assert_eq!(bindings.get(TestAction::Jump), keys(&[Key::F]));
        assert_defaults_except(&bindings, &[TestAction::Jump]);
    }

    #[test]
    fn hold_bindings_round_trip() {
        let held = [
            (Binding::key(Key::R).held(), "Hold R"),
            (
                Binding::mouse(MouseButton::Right).held(),
                "Hold Mouse Right",
            ),
            (
                Binding::pad(PadInput::Button(PadButton::West)).held(),
                "Hold Pad West",
            ),
        ];

        for (binding, name) in held {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("prefs.ini");
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            bindings.set(TestAction::Jump, vec![Binding::key(Key::F), binding]);
            let mut prefs = Prefs::default();
            prefs.set_bindings(bindings.to_section());
            std::fs::write(&path, prefs.to_ini_string()).unwrap();

            let (reloaded, prefs_warnings) = Prefs::load_from(&path);
            let (restored, binding_warnings) =
                ActionBindings::<TestAction>::load(reloaded.bindings());

            assert_eq!(binding.name(), name);
            assert_eq!(bindings.to_section()["jump"], format!("F, {name}"));
            assert_eq!(prefs_warnings, vec![], "{name}");
            assert_eq!(binding_warnings, vec![], "{name}");
            assert_eq!(
                restored.get(TestAction::Jump),
                [Binding::key(Key::F), binding],
                "{name}"
            );
        }
    }

    #[test]
    fn hold_prefix_is_case_insensitive_and_distinct_from_the_plain_input() {
        let (bindings, warnings) =
            ActionBindings::<TestAction>::load(&section(&[("jump", "hold r, HOLD Space, R")]));

        assert_eq!(warnings, vec![]);
        assert_eq!(
            bindings.get(TestAction::Jump),
            [
                Binding::key(Key::R).held(),
                Binding::key(Key::Space).held(),
                Binding::key(Key::R),
            ]
        );
        assert_ne!(Binding::key(Key::R), Binding::key(Key::R).held());
    }

    #[test]
    fn hold_label_prefixes_the_input_label() {
        assert_eq!(Binding::key(Key::Space).held().label(), "Hold Spacebar");
        assert_eq!(
            Binding::mouse(MouseButton::Left).held().label(),
            "Hold Mouse Left"
        );
    }

    #[test]
    fn bad_hold_value_warns_and_keeps_default() {
        for bad in ["Hold Nonsense", "Hold", "Hold Hold R"] {
            let (bindings, warnings) =
                ActionBindings::<TestAction>::load(&section(&[("jump", bad)]));

            let names: Vec<_> = warnings.iter().map(|w| w.name.as_str()).collect();
            assert_eq!(names, ["jump"], "{bad}");
            assert_defaults_except(&bindings, &[]);
        }
    }

    #[test]
    fn unbound_leaves_action_without_binding() {
        for spelling in ["Unbound", "unbound", "UNBOUND", ""] {
            let (bindings, warnings) =
                ActionBindings::<TestAction>::load(&section(&[("jump", spelling)]));

            assert_eq!(warnings, vec![], "{spelling:?}");
            assert_eq!(bindings.get(TestAction::Jump), [], "{spelling:?}");
            assert_defaults_except(&bindings, &[TestAction::Jump]);
        }
    }

    #[test]
    fn chord_bindings_round_trip() {
        let chords = [
            (Binding::key(Key::R).with(Modifiers::CTRL), "Ctrl+R"),
            (
                Binding::mouse(MouseButton::Left).with(Modifiers::SHIFT),
                "Shift+Mouse Left",
            ),
            (
                Binding::key(Key::R).with(Modifiers::CTRL).held(),
                "Hold Ctrl+R",
            ),
            (Binding::key(Key::Ctrl).with(Modifiers::SHIFT), "Shift+Ctrl"),
            (
                Binding::key(Key::Q)
                    .with(Modifiers::CTRL.and(Modifiers::SHIFT).and(Modifiers::ALT)),
                "Ctrl+Shift+Alt+Q",
            ),
        ];

        for (binding, name) in chords {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("prefs.ini");
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            bindings.set(TestAction::Jump, vec![Binding::key(Key::F), binding]);
            let mut prefs = Prefs::default();
            prefs.set_bindings(bindings.to_section());
            std::fs::write(&path, prefs.to_ini_string()).unwrap();

            let (reloaded, prefs_warnings) = Prefs::load_from(&path);
            let (restored, binding_warnings) =
                ActionBindings::<TestAction>::load(reloaded.bindings());

            assert_eq!(binding.name(), name);
            assert_eq!(bindings.to_section()["jump"], format!("F, {name}"));
            assert_eq!(prefs_warnings, vec![], "{name}");
            assert_eq!(binding_warnings, vec![], "{name}");
            assert_eq!(
                restored.get(TestAction::Jump),
                [Binding::key(Key::F), binding],
                "{name}"
            );
        }
    }

    #[test]
    fn chord_modifiers_parse_in_any_order_and_case_and_read_back_canonical() {
        let ctrl_alt = Modifiers::CTRL.and(Modifiers::ALT);
        let all_three = ctrl_alt.and(Modifiers::SHIFT);
        let cases = [
            (
                "ctrl+alt+r",
                Binding::key(Key::R).with(ctrl_alt),
                "Ctrl+Alt+R",
            ),
            (
                "alt+ctrl+r",
                Binding::key(Key::R).with(ctrl_alt),
                "Ctrl+Alt+R",
            ),
            (
                "ALT+SHIFT+CTRL+R",
                Binding::key(Key::R).with(all_three),
                "Ctrl+Shift+Alt+R",
            ),
            (
                "shift+mouse left",
                Binding::mouse(MouseButton::Left).with(Modifiers::SHIFT),
                "Shift+Mouse Left",
            ),
            (
                "Alt+Ctrl+Mouse Right",
                Binding::mouse(MouseButton::Right).with(ctrl_alt),
                "Ctrl+Alt+Mouse Right",
            ),
            (
                "hold alt+ctrl+r",
                Binding::key(Key::R).with(ctrl_alt).held(),
                "Hold Ctrl+Alt+R",
            ),
            (
                "Hold Shift+Space",
                Binding::key(Key::Space).with(Modifiers::SHIFT).held(),
                "Hold Shift+Spacebar",
            ),
        ];

        for (written, binding, canonical) in cases {
            let (bindings, warnings) =
                ActionBindings::<TestAction>::load(&section(&[("jump", written)]));

            assert_eq!(warnings, vec![], "{written}");
            assert_eq!(bindings.get(TestAction::Jump), [binding], "{written}");
            assert_eq!(bindings.to_section()["jump"], canonical, "{written}");
        }
    }

    #[test]
    fn combining_a_modifier_already_in_the_set_keeps_it() {
        let ctrl_shift = Modifiers::CTRL.and(Modifiers::SHIFT);

        assert_eq!(ctrl_shift.and(Modifiers::CTRL), ctrl_shift);
        assert_eq!(Modifiers::ALT.and(Modifiers::ALT), Modifiers::ALT);
    }

    #[test]
    fn only_a_press_without_modifiers_is_plain() {
        let r = Binding::key(Key::R);
        let cases = [
            (r, true),
            (Binding::mouse(MouseButton::Left), true),
            (Binding::pad(PadInput::Button(PadButton::West)), true),
            (r.held(), false),
            (r.with(Modifiers::CTRL), false),
            (r.with(Modifiers::SHIFT).held(), false),
        ];

        for (binding, plain) in cases {
            assert_eq!(binding.is_plain(), plain, "{}", binding.name());
        }
    }

    #[test]
    fn chord_label_prefixes_the_modifiers_in_canonical_order() {
        let ctrl_alt = Modifiers::ALT.and(Modifiers::CTRL);

        assert_eq!(Binding::key(Key::R).with(Modifiers::CTRL).label(), "Ctrl+R");
        assert_eq!(
            Binding::key(Key::Space).with(Modifiers::SHIFT).label(),
            "Shift+Spacebar"
        );
        assert_eq!(Binding::key(Key::R).with(ctrl_alt).label(), "Ctrl+Alt+R");
        assert_eq!(
            Binding::mouse(MouseButton::Left)
                .with(Modifiers::ALT)
                .label(),
            "Alt+Mouse Left"
        );
        assert_eq!(
            Binding::key(Key::R).with(Modifiers::CTRL).held().label(),
            "Hold Ctrl+R"
        );
        assert_eq!(Binding::key(Key::R).label(), "R");
    }

    fn warned_names(warnings: &[BindingWarning]) -> Vec<&str> {
        warnings.iter().map(|w| w.name.as_str()).collect()
    }

    #[test]
    fn mode_on_a_non_toggleable_action_warns() {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());

        let warnings = bindings.load_modes(&section(&[("move_forward", "toggle")]));

        assert_eq!(
            warnings,
            vec![BindingWarning {
                name: "move_forward".to_string(),
                value: "toggle".to_string(),
            }]
        );
        assert_eq!(bindings.mode(TestAction::MoveForward), ActionMode::Hold);
        assert!(!bindings.modes_to_section().contains_key("move_forward"));
    }

    #[test]
    fn modes_round_trip() {
        for mode in [ActionMode::Toggle, ActionMode::Hold] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("prefs.ini");
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            bindings.set_mode(TestAction::Jump, mode);
            let mut prefs = Prefs::default();
            prefs.set_action_modes(bindings.modes_to_section());
            std::fs::write(&path, prefs.to_ini_string()).unwrap();

            let (reloaded, prefs_warnings) = Prefs::load_from(&path);
            let (mut restored, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
            let mode_warnings = restored.load_modes(reloaded.action_modes());

            assert_eq!(prefs_warnings, vec![], "{mode:?}");
            assert_eq!(mode_warnings, vec![], "{mode:?}");
            assert_eq!(restored.mode(TestAction::Jump), mode, "{mode:?}");
            assert_eq!(restored.mode(TestAction::MoveForward), ActionMode::Hold);
        }
    }

    #[test]
    fn actions_are_not_toggleable_unless_the_game_says_so() {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
        enum PlainAction {
            Fire,
            Reload,
        }

        impl Action for PlainAction {
            const ALL: &'static [Self] = &[PlainAction::Fire, PlainAction::Reload];

            fn name(self) -> &'static str {
                match self {
                    PlainAction::Fire => "fire",
                    PlainAction::Reload => "reload",
                }
            }

            fn default_bindings(self) -> &'static [Binding] {
                &[]
            }
        }

        for &action in PlainAction::ALL {
            assert!(!action.toggleable(), "{}", action.name());
        }
    }

    #[test]
    fn mode_converts_to_and_from_the_toggle_flag() {
        assert!(!ActionMode::Hold.is_toggle());
        assert!(ActionMode::Toggle.is_toggle());
        for toggle in [false, true] {
            assert_eq!(ActionMode::from_toggle(toggle).is_toggle(), toggle);
        }
    }

    #[test]
    fn toggle_is_written_by_name() {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
        bindings.set_mode(TestAction::Jump, ActionMode::Toggle);

        let written = bindings.modes_to_section();

        assert_eq!(written["jump"], "toggle");
    }

    #[test]
    fn set_mode_toggle_on_a_non_toggleable_action_is_ignored() {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());

        bindings.set_mode(TestAction::MoveForward, ActionMode::Toggle);

        assert_eq!(bindings.mode(TestAction::MoveForward), ActionMode::Hold);
    }

    #[test]
    fn mode_values_are_case_insensitive() {
        for (value, mode) in [
            ("toggle", ActionMode::Toggle),
            ("Toggle", ActionMode::Toggle),
            ("TOGGLE", ActionMode::Toggle),
            ("hold", ActionMode::Hold),
            ("Hold", ActionMode::Hold),
        ] {
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());

            let warnings = bindings.load_modes(&section(&[("jump", value)]));

            assert_eq!(warnings, vec![], "{value}");
            assert_eq!(bindings.mode(TestAction::Jump), mode, "{value}");
        }
    }

    #[test]
    fn bad_mode_warns_and_keeps_hold() {
        let cases = [
            ("jump", "sometimes", "jump"),
            ("teleport", "toggle", "teleport"),
        ];

        for (action, value, reported) in cases {
            let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());

            let warnings = bindings.load_modes(&section(&[(action, value)]));

            assert_eq!(warned_names(&warnings), [reported], "{action}={value}");
            assert_eq!(warnings[0].value, value, "{action}={value}");
            for &a in TestAction::ALL {
                assert_eq!(bindings.mode(a), ActionMode::Hold, "{action}={value}");
            }
        }
    }

    #[test]
    fn a_bad_mode_line_does_not_disturb_a_valid_one() {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());

        let warnings = bindings.load_modes(&section(&[("aardvark", "hold"), ("jump", "toggle")]));

        assert_eq!(warned_names(&warnings), ["aardvark"]);
        assert_eq!(bindings.mode(TestAction::Jump), ActionMode::Toggle);
    }

    #[test]
    fn bad_chord_warns_and_keeps_default() {
        let bad_values = [
            "Q+R",
            "Ctrl+Ctrl",
            "Ctrl+Ctrl+R",
            "Ctrl+Pad West",
            "Ctrl+",
            "Control+R",
            "Ctrl+Hold R",
            "Hold Q+R",
            "F, Ctrl+R, Q+R",
        ];

        for bad in bad_values {
            let (bindings, warnings) =
                ActionBindings::<TestAction>::load(&section(&[("jump", bad)]));

            let names: Vec<_> = warnings.iter().map(|w| w.name.as_str()).collect();
            assert_eq!(names, ["jump"], "{bad}");
            assert_defaults_except(&bindings, &[]);
        }
    }

    const PROTECTED_KEYS: [Key; 6] = [
        Key::Enter,
        Key::Escape,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
    ];

    #[test]
    fn a_protected_key_alone_is_refused_with_a_warning_and_the_default_kept() {
        let mut values: Vec<String> = PROTECTED_KEYS
            .iter()
            .map(|&key| Binding::key(key).name())
            .collect();
        values.push("Hold Enter".to_owned());
        values.push(format!("F, {}", Binding::key(Key::Escape).name()));

        for value in values {
            let (bindings, warnings) =
                ActionBindings::<TestAction>::load(&section(&[("jump", &value)]));

            assert_eq!(
                warnings,
                vec![BindingWarning {
                    name: "jump".to_owned(),
                    value: value.clone(),
                }],
                "{value}"
            );
            assert_defaults_except(&bindings, &[]);
        }
    }

    #[test]
    fn a_chord_on_a_protected_key_is_accepted() {
        let (bindings, warnings) =
            ActionBindings::<TestAction>::load(&section(&[("jump", "Ctrl+Enter")]));

        assert_eq!(warnings, vec![]);
        assert_eq!(
            bindings.get(TestAction::Jump),
            [Binding::key(Key::Enter).with(Modifiers::CTRL)]
        );
    }

    #[test]
    fn only_an_unmodified_enter_escape_or_arrow_is_protected() {
        for key in PROTECTED_KEYS {
            let plain = Binding::key(key);

            assert!(plain.is_protected(), "{}", plain.name());
            assert!(plain.held().is_protected(), "{}", plain.held().name());
            assert!(
                !plain.with(Modifiers::SHIFT).is_protected(),
                "Shift+{}",
                plain.name()
            );
            assert!(
                !plain.with(Modifiers::ALT).held().is_protected(),
                "Hold Alt+{}",
                plain.name()
            );
        }
        for free in [Binding::key(Key::F8), Binding::key(Key::Space)] {
            assert!(!free.is_protected(), "{}", free.name());
        }
        assert!(!Binding::mouse(MouseButton::Left).is_protected());
    }

    #[test]
    fn a_function_key_can_be_bound() {
        for key in [Key::F1, Key::F8, Key::F11, Key::F12] {
            let name = Binding::key(key).name();
            let (bindings, warnings) =
                ActionBindings::<TestAction>::load(&section(&[("jump", &name)]));
            let mut map = crate::action_map::ActionMap::new(bindings, 1.0);
            map.key(key, true);

            let frame = map.end_tick();

            assert_eq!(warnings, vec![], "{name}");
            assert!(frame.pressed(TestAction::Jump), "{name}");
            assert!(!frame.pressed(TestAction::MoveForward), "{name}");
        }
    }
}
