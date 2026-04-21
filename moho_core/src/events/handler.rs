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

/// Container for handlers of a specific event type
pub struct HandlerList {
    handlers: Vec<Handler>,
    sorted: bool,
}

impl fmt::Debug for HandlerList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerList")
            .field("handler_count", &self.handlers.len())
            .field("sorted", &self.sorted)
            .finish()
    }
}

impl HandlerList {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
            sorted: true,
        }
    }

    pub fn add(&mut self, handler: Handler) {
        self.handlers.push(handler);
        self.sorted = false;
    }

    pub fn sort_by_priority(&mut self) {
        if !self.sorted {
            self.handlers.sort_by_key(|h| h.priority());
            self.sorted = true;
        }
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
