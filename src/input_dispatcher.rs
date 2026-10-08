use winit::event::WindowEvent;

type InputHandler = Box<dyn Fn(&WindowEvent) -> bool + Send + Sync>;

/// Simple prioritized input dispatcher for WindowEvent.
/// Subscribers are called in descending priority order until one consumes the event.
///
/// # Priority convention
/// Higher numbers = higher priority = called first. This is the opposite of
/// `EventBus::subscribe_with_priority`, which uses lower numbers for higher priority.
/// Typical values used in this codebase: 200 (keybind capture), 100 (UI), 0 (game).
pub struct InputDispatcher {
    subscribers: Vec<(i32, InputHandler)>,
}

impl std::fmt::Debug for InputDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputDispatcher")
            .field("subscriber_count", &self.subscribers.len())
            .finish()
    }
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

        for priority in [0, 10, 5] {
            let o = order.clone();
            disp.register(priority, move |_ev: &WindowEvent| {
                o.lock().unwrap().push(priority);
                false
            });
        }

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

    fn ui_adapter(visible: bool) -> Arc<Mutex<moho_ui::EguiAdapter>> {
        let event_bus = Arc::new(moho_core::events::EventBus::new());
        let mut adapter = moho_ui::build_adapter(None, event_bus);
        adapter.set_visible(visible);
        Arc::new(Mutex::new(adapter))
    }

    #[test]
    fn wheel_forwarded_when_ui_hidden() {
        let adapter = ui_adapter(false);
        let (tx, rx) = std::sync::mpsc::channel();

        let forwarded = crate::forward_wheel_if_allowed(&adapter, &tx, 1.0);

        assert!(forwarded);
        let event = rx.try_recv().expect("wheel event forwarded");
        assert!(matches!(
            event,
            crate::input_event::InputEvent::MouseWheel { delta_y } if delta_y == 1.0
        ));
    }

    #[test]
    fn wheel_blocked_when_ui_visible() {
        let adapter = ui_adapter(true);
        let (tx, rx) = std::sync::mpsc::channel();

        let forwarded = crate::forward_wheel_if_allowed(&adapter, &tx, 1.0);

        assert!(!forwarded);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn wheel_blocked_when_ui_lock_contended() {
        let adapter = ui_adapter(false);
        let (tx, rx) = std::sync::mpsc::channel();

        let guard = adapter.lock().unwrap();
        let forwarded = crate::forward_wheel_if_allowed(&adapter, &tx, 1.0);
        drop(guard);

        assert!(!forwarded);
        assert!(rx.try_recv().is_err());
    }
}
