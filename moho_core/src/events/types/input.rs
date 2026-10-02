use crate::events::Event;
use std::any::Any;

/// Input events from keyboard/mouse
#[derive(Clone, Debug)]
pub enum InputEvent {
    /// Key pressed
    KeyPressed { code: u32, mods: u8 },

    /// Key released
    KeyReleased { code: u32, mods: u8 },

    /// Mouse moved
    MouseMoved { delta_x: f32, delta_y: f32 },

    /// Mouse wheel scrolled
    MouseWheel { delta: f32 },

    /// Mouse button pressed
    MouseButtonPressed { button: u8 },

    /// Mouse button released
    MouseButtonReleased { button: u8 },
}

impl Event for InputEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn should_record(&self) -> bool {
        // Don't record mouse movement (too frequent)
        !matches!(self, InputEvent::MouseMoved { .. })
    }
}
