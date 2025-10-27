use std::any::Any;
use std::fmt::Debug;

/// Base trait for all events in the system.
///
/// Events must be:
/// - Clone: For event history and replay
/// - Debug: For logging and debugging
/// - Send + Sync: For thread-safe event bus
/// - 'static: For type erasure
///
/// # Example
///
/// ```
/// use moho_core::events::Event;
/// use std::any::Any;
///
/// #[derive(Clone, Debug)]
/// pub struct PlayerMoved {
///     pub position: glam::Vec3,
/// }
///
/// impl Event for PlayerMoved {
///     fn as_any(&self) -> &dyn Any {
///         self
///     }
/// }
/// ```
pub trait Event: Clone + Debug + Send + Sync + 'static {
    /// Optional event priority (lower = higher priority)
    /// Used for ordering when multiple events are processed
    fn priority(&self) -> i32 {
        0
    }

    /// Whether this event should be recorded in history
    /// Disable for high-frequency events to save memory
    fn should_record(&self) -> bool {
        true
    }

    /// Convert to Any for downcasting
    fn as_any(&self) -> &dyn Any;
}
