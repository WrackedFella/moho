//! Input state — groups keyboard tracking, the input smoothing system,
//! and the uncaptured-input channel used to forward events to the game loop.

use std::collections::HashSet;

pub struct InputState {
    /// Currently held physical keys, stored as (binding_code, modifier_mask).
    pub active_keys: HashSet<(u32, u8)>,

    /// Smoothing / accumulation for mouse deltas.
    pub system: moho_core::input::InputSystem,

    /// Sender side of the channel for input events not consumed by the UI.
    /// `None` before the UI is set up.
    pub unconsumed_tx: Option<crossbeam_channel::Sender<crate::input_event::InputEvent>>,

    /// Receiver side of the same channel.
    pub unconsumed_rx: Option<crossbeam_channel::Receiver<crate::input_event::InputEvent>>,
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
