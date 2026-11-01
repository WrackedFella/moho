//! Input state: the action map and the uncaptured-input channel used to
//! forward events to the game loop.

use flagstone_ui::actions::StrategyAction;
use moho_input::action_map::ActionMap;
use moho_input::gamepad::Gamepads;

/// What a strategy click orders. Input state the local player holds, like the hotbar slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Designation {
    /// A click selects.
    #[default]
    None,
    /// A click marks or unmarks a block for mining.
    Mine,
}

impl Designation {
    /// Turns the mining designation on when it is off and off when it is on.
    pub fn toggle(&mut self) {
        *self = match self {
            Designation::None => Designation::Mine,
            Designation::Mine => Designation::None,
        };
    }
}

pub struct InputState {
    /// What a strategy click orders.
    pub designation: Designation,

    /// Bindings, held actions and mouse look, read once per update.
    pub actions: ActionMap<StrategyAction>,

    /// Connected pads; `None` when the backend is unavailable or not started.
    pub gamepads: Option<Gamepads>,

    /// Whether the window has focus; pads drive the game only while it does.
    pub focused: bool,

    /// Sender side of the channel for input events not consumed by the UI.
    /// `None` before the UI is set up.
    pub unconsumed_tx: Option<std::sync::mpsc::Sender<crate::input_event::InputEvent>>,

    /// Receiver side of the same channel.
    pub unconsumed_rx: Option<std::sync::mpsc::Receiver<crate::input_event::InputEvent>>,
}

impl InputState {
    pub fn new(actions: ActionMap<StrategyAction>) -> Self {
        Self {
            designation: Designation::None,
            actions,
            gamepads: None,
            focused: true,
            unconsumed_tx: None,
            unconsumed_rx: None,
        }
    }
}
