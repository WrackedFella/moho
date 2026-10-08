//! Input state — groups keyboard tracking, the input smoothing system,
//! and the uncaptured-input channel used to forward events to the game loop.

use std::collections::HashSet;

use moho_input::key::Key;

pub struct InputState {
    /// Currently held physical keys.
    pub active_keys: HashSet<Key>,

    /// Smoothing / accumulation for mouse deltas.
    pub system: moho_core::input::InputSystem,

    /// Sender side of the channel for input events not consumed by the UI.
    /// `None` before the UI is set up.
    pub unconsumed_tx: Option<std::sync::mpsc::Sender<crate::input_event::InputEvent>>,

    /// Receiver side of the same channel.
    pub unconsumed_rx: Option<std::sync::mpsc::Receiver<crate::input_event::InputEvent>>,
}

impl InputState {
    pub fn new(system: moho_core::input::InputSystem) -> Self {
        Self {
            active_keys: HashSet::new(),
            system,
            unconsumed_tx: None,
            unconsumed_rx: None,
        }
    }
}
