use winit::event::WindowEvent;

type InputHandler = Box<dyn Fn(&WindowEvent) -> bool + Send + Sync>;

/// Simple prioritized input dispatcher for WindowEvent.
/// Subscribers are called in descending priority order until one consumes the event.
pub struct InputDispatcher {
    subscribers: Vec<(i32, InputHandler)>,
}

impl InputDispatcher {
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use winit::event::WindowEvent;

    #[test]
    fn test_priority_ordering() {
        let mut disp = InputDispatcher::new();

        let order: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));

        let o1 = order.clone();
        disp.register(10, move |_ev: &WindowEvent| {
            o1.lock().unwrap().push(10);
            false
        });

        let o2 = order.clone();
        disp.register(5, move |_ev: &WindowEvent| {
            o2.lock().unwrap().push(5);
            false
        });

        let o3 = order.clone();
        disp.register(0, move |_ev: &WindowEvent| {
            o3.lock().unwrap().push(0);
            false
        });

        // Dispatch any simple event
        let ev = WindowEvent::Focused(true);
        assert!(!disp.dispatch(&ev));

        let got = order.lock().unwrap().clone();
        assert_eq!(got, vec![10, 5, 0]);
    }

    #[test]
    fn test_stop_on_consume() {
        let mut disp = InputDispatcher::new();

        let order: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));

        let o1 = order.clone();
        // High priority consumes
        disp.register(5, move |_ev: &WindowEvent| {
            o1.lock().unwrap().push(5);
            true
        });

        let o2 = order.clone();
        disp.register(3, move |_ev: &WindowEvent| {
            o2.lock().unwrap().push(3);
            false
        });

        let o3 = order.clone();
        disp.register(1, move |_ev: &WindowEvent| {
            o3.lock().unwrap().push(1);
            false
        });

        let ev = WindowEvent::Resized(winit::dpi::PhysicalSize::new(100u32, 100u32));
        assert!(disp.dispatch(&ev));

        let got = order.lock().unwrap().clone();
        // Only the consumer (priority 5) should have run
        assert_eq!(got, vec![5]);
    }
}
