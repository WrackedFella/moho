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
        todo!()
    }

    pub fn parse(_s: &str) -> Option<PadInput> {
        todo!()
    }
}
