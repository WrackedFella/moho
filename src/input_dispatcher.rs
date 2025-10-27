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

    #[cfg(feature = "ui-egui")]
    #[test]
    fn wheel_forwarding_respects_ui_visibility() {
        use crossbeam_channel::unbounded;
        use std::sync::{Arc, Mutex};

        let (adapter, _rx) = moho_ui::build_adapter(None);
        let adapter = Arc::new(Mutex::new(adapter));

        // Case 1: UI hidden -> forward
        adapter.lock().unwrap().set_visible(false);
        let (tx, rx) = unbounded::<crate::input_event::InputEvent>();
        let forwarded = crate::forward_wheel_if_allowed(&adapter, &tx, 1.0);
        assert!(forwarded);
        assert!(rx.try_recv().is_ok());

        // Case 2: UI visible -> do not forward
        adapter.lock().unwrap().set_visible(true);
        let (tx2, rx2) = unbounded::<crate::input_event::InputEvent>();
        let forwarded2 = crate::forward_wheel_if_allowed(&adapter, &tx2, 1.0);
        assert!(!forwarded2);
        assert!(rx2.try_recv().is_err());

        // Case 3: lock failure (simulate by holding the lock) -> do not forward
        let guard = adapter.lock().unwrap();
        let (tx3, rx3) = unbounded::<crate::input_event::InputEvent>();
        let forwarded3 = crate::forward_wheel_if_allowed(&adapter, &tx3, 1.0);
        assert!(!forwarded3);
        assert!(rx3.try_recv().is_err());
        drop(guard);
    }
}
