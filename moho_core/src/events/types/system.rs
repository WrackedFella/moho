use crate::events::Event;
use std::any::Any;

/// System lifecycle and frame events
#[derive(Clone, Debug)]
pub enum SystemEvent {
    /// Application startup complete
    Started,

    /// Application shutting down
    Shutdown,

    /// Frame start with timing info
    FrameStart {
        frame_number: u64,
        delta_time: f32,
    },

    /// Frame end
    FrameEnd { frame_number: u64 },

    /// Error occurred in system
    ErrorOccurred { system: String, message: String },

    /// Warning message
    Warning { system: String, message: String },
}

impl Event for SystemEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn should_record(&self) -> bool {
        // Don't record frame events (too frequent)
        !matches!(
            self,
            SystemEvent::FrameStart { .. } | SystemEvent::FrameEnd { .. }
        )
    }
}
