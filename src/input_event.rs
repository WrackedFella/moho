/// Small cloneable input event enum used for cross-thread decisions
#[derive(Clone, Debug)]
pub enum InputEvent {
    MouseWheel { delta_x: f32, delta_y: f32 },
}
