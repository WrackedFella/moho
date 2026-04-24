use crate::PlayerInput;

/// Small struct representing a sampled continuous input state for a single frame/tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuousState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    /// action (e.g. mouse click / gamepad button)
    pub action: bool,
    /// pointer movement accumulated during a frame (optional)
    pub mouse_dx: i32,
    pub mouse_dy: i32,
}

/// A small wrapper for a time-stamped input. We keep this serializable so
/// tests and simple network packets can reuse it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedInput {
    pub tick: u64,
    pub input: PlayerInput,
}

/// Map a pair of sampled continuous states (previous, current) into a small
/// vector of discrete `PlayerInput` values representing actions that should
/// apply on this tick. This is intentionally simple and deterministic.
pub fn map_to_player_inputs(prev: &ContinuousState, curr: &ContinuousState) -> Vec<PlayerInput> {
    let mut out = Vec::new();

    // Directional movement: produce a single integer move representing the
    // net direction during this tick. This maps held keys into one Move per
    // tick which is simpler for deterministic simulation.
    let dx = (curr.right as i32) - (curr.left as i32);
    let dy = (curr.down as i32) - (curr.up as i32);
    if dx != 0 || dy != 0 {
        out.push(PlayerInput::Move { dx, dy });
    }

    // Action button: produce a single Action when the button transitions
    // from not-pressed -> pressed. This avoids duplicate actions for held buttons.
    if curr.action && !prev.action {
        out.push(PlayerInput::Action(1));
    }

    // Optional: pointer movement can be converted to actions or movement as
    // desired. For now we ignore mouse deltas in discrete mapping, but the
    // field exists for future mapping strategies.

    out
}

/// Utility to stamp a list of inputs with a tick index.
pub fn stamp_inputs(tick: u64, inputs: Vec<PlayerInput>) -> Vec<TimedInput> {
    inputs
        .into_iter()
        .map(|input| TimedInput { tick, input })
        .collect()
}
