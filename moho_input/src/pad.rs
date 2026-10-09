//! Platform-free gamepad identity and canonical text names.

/// A gamepad button the engine can name, bind and persist.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PadButton {
    South,
    East,
    North,
    West,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl PadButton {
    pub const ALL: &'static [PadButton] = &[
        PadButton::South,
        PadButton::East,
        PadButton::North,
        PadButton::West,
        PadButton::LeftBumper,
        PadButton::RightBumper,
        PadButton::LeftTrigger,
        PadButton::RightTrigger,
        PadButton::Select,
        PadButton::Start,
        PadButton::Mode,
        PadButton::LeftThumb,
        PadButton::RightThumb,
        PadButton::DPadUp,
        PadButton::DPadDown,
        PadButton::DPadLeft,
        PadButton::DPadRight,
    ];
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stick {
    LeftStick,
    RightStick,
}

impl Stick {
    pub const ALL: &'static [Stick] = &[Stick::LeftStick, Stick::RightStick];
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StickAxis {
    X,
    Y,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StickDir {
    Up,
    Down,
    Left,
    Right,
}

impl StickDir {
    pub const ALL: &'static [StickDir] = &[
        StickDir::Up,
        StickDir::Down,
        StickDir::Left,
        StickDir::Right,
    ];
}

/// One gamepad input that can be bound to an action.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PadInput {
    Button(PadButton),
    Stick(Stick, StickDir),
}

impl PadInput {
    /// Persisted name, e.g. `Pad South` or `Pad LeftStick Up`.
    pub fn name(self) -> &'static str {
        match self {
            PadInput::Button(button) => match button {
                PadButton::South => "Pad South",
                PadButton::East => "Pad East",
                PadButton::North => "Pad North",
                PadButton::West => "Pad West",
                PadButton::LeftBumper => "Pad LeftBumper",
                PadButton::RightBumper => "Pad RightBumper",
                PadButton::LeftTrigger => "Pad LeftTrigger",
                PadButton::RightTrigger => "Pad RightTrigger",
                PadButton::Select => "Pad Select",
                PadButton::Start => "Pad Start",
                PadButton::Mode => "Pad Mode",
                PadButton::LeftThumb => "Pad LeftThumb",
                PadButton::RightThumb => "Pad RightThumb",
                PadButton::DPadUp => "Pad DPadUp",
                PadButton::DPadDown => "Pad DPadDown",
                PadButton::DPadLeft => "Pad DPadLeft",
                PadButton::DPadRight => "Pad DPadRight",
            },
            PadInput::Stick(Stick::LeftStick, dir) => match dir {
                StickDir::Up => "Pad LeftStick Up",
                StickDir::Down => "Pad LeftStick Down",
                StickDir::Left => "Pad LeftStick Left",
                StickDir::Right => "Pad LeftStick Right",
            },
            PadInput::Stick(Stick::RightStick, dir) => match dir {
                StickDir::Up => "Pad RightStick Up",
                StickDir::Down => "Pad RightStick Down",
                StickDir::Left => "Pad RightStick Left",
                StickDir::Right => "Pad RightStick Right",
            },
        }
    }

    pub fn parse(s: &str) -> Option<PadInput> {
        let buttons = PadButton::ALL.iter().map(|&b| PadInput::Button(b));
        let sticks = Stick::ALL.iter().flat_map(|&stick| {
            StickDir::ALL
                .iter()
                .map(move |&dir| PadInput::Stick(stick, dir))
        });
        buttons.chain(sticks).find(|input| input.name() == s)
    }
}
