use winit::event::WindowEvent;

/// Simple prioritized input dispatcher for WindowEvent.
/// Subscribers are called in descending priority order until one consumes the event.
pub struct InputDispatcher {
    subscribers: Vec<(i32, Box<dyn Fn(&WindowEvent) -> bool + Send + Sync>)>,
}

impl InputDispatcher {
    pub fn new() -> Self {
        Self { subscribers: Vec::new() }
    }

    /// Register a subscriber with a priority. Higher priority subscribers run first.
    pub fn register<F>(&mut self, priority: i32, handler: F)
    where
        F: Fn(&WindowEvent) -> bool + Send + Sync + 'static,
    {
        // Insert keeping descending priority order
        let pos = self
            .subscribers
            .iter()
            .position(|(p, _)| *p < priority)
            .unwrap_or(self.subscribers.len());
        self.subscribers.insert(pos, (priority, Box::new(handler)));
    }

    /// Dispatch an event to subscribers. Returns true if consumed.
    pub fn dispatch(&self, event: &WindowEvent) -> bool {
        for (_p, handler) in &self.subscribers {
            if handler(event) {
                return true;
            }
        }
        false
    }
}
