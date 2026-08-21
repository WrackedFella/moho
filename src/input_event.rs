/// Small cloneable input event enum used for cross-thread decisions
#[derive(Clone, Debug)]
pub enum InputEvent {
    MouseWheel {
        delta_y: f32,
    },
    /// Left mouse button pressed while not captured by UI — a mine attempt.
    MineRequested,
}
