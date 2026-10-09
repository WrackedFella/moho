//! Input state: the action map and the uncaptured-input channel used to
//! forward events to the game loop.

use moho_input::action_map::ActionMap;
use moho_input::gamepad::Gamepads;
use moho_ui::actions::StrategyAction;

pub struct InputState {
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
            actions,
            gamepads: None,
            focused: true,
            unconsumed_tx: None,
            unconsumed_rx: None,
        }
    }
}
