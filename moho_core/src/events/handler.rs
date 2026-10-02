use super::Event;
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// Type alias for event handlers
pub type HandlerFn<E> = Arc<dyn Fn(&E) + Send + Sync>;

/// Handler storage with priority support
pub struct Handler {
    handler: Arc<dyn Any + Send + Sync>,
    priority: i32,
}

impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handler")
            .field("priority", &self.priority)
            .finish_non_exhaustive()
    }
}

impl Handler {
    pub fn new<E: Event>(handler: HandlerFn<E>, priority: i32) -> Self {
        Self {
            handler: Arc::new(handler) as Arc<dyn Any + Send + Sync>,
            priority,
        }
    }

    pub fn priority(&self) -> i32 {
        self.priority
    }

    pub fn downcast<E: Event>(&self) -> Option<&HandlerFn<E>> {
        self.handler.downcast_ref::<HandlerFn<E>>()
    }
}

/// Container for handlers of a specific event type, kept in priority order.
pub struct HandlerList {
    handlers: Vec<Handler>,
}

impl fmt::Debug for HandlerList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerList")
            .field("handler_count", &self.handlers.len())
            .finish()
    }
}

impl HandlerList {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Insert the handler in priority order so dispatch never needs to sort.
    pub fn add(&mut self, handler: Handler) {
        let pos = self
            .handlers
            .partition_point(|h| h.priority() <= handler.priority());
        self.handlers.insert(pos, handler);
    }

    pub fn handlers(&self) -> &[Handler] {
        &self.handlers
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

impl Default for HandlerList {
    fn default() -> Self {
        Self::new()
    }
}
