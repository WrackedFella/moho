use super::Event;
use super::handler::{Handler, HandlerFn, HandlerList};
use super::metrics::{EventMetrics, MetricsTracker};
use std::any::TypeId;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

/// Deferred event wrapper that captures its own dispatch logic.
///
/// The closure retains the concrete event type, avoiding the need
/// to recover type information from a `Box<dyn Any>` at dispatch time.
struct DeferredEvent {
    dispatch: Box<dyn FnOnce(&EventBus) + Send>,
    type_name: String,
}

/// Central event bus for application-wide event distribution
///
/// # Example
///
/// ```no_run
/// use moho_core::events::EventBus;
/// use moho_core::events::Event;
/// use std::any::Any;
///
/// #[derive(Clone, Debug)]
/// struct PlayerMoved {
///     position: glam::Vec3,
/// }
///
/// impl Event for PlayerMoved {
///     fn as_any(&self) -> &dyn Any { self }
/// }
///
/// let bus = EventBus::new();
///
/// // Subscribe to events
/// bus.subscribe(|event: &PlayerMoved| {
///     println!("Player moved to: {:?}", event.position);
/// });
///
/// // Publish events
/// bus.publish(PlayerMoved {
///     position: glam::Vec3::new(1.0, 2.0, 3.0),
/// });
/// ```
pub struct EventBus {
    /// Synchronous handlers (called immediately on publish)
    sync_handlers: Arc<RwLock<HashMap<TypeId, HandlerList>>>,

    /// Deferred event queue (processed at end of frame)
    deferred_queue: Arc<Mutex<VecDeque<DeferredEvent>>>,

    /// Event history for debugging/replay (limited size)
    history: Arc<Mutex<VecDeque<String>>>,
    history_enabled: bool,
    max_history: usize,

    /// Performance metrics
    metrics_tracker: Arc<MetricsTracker>,
}

impl EventBus {
    /// Create a new event bus with default settings
    ///
    /// Default configuration:
    /// - Event history enabled
    /// - Maximum 1000 events in history
    pub fn new() -> Self {
        Self {
            sync_handlers: Arc::new(RwLock::new(HashMap::new())),
            deferred_queue: Arc::new(Mutex::new(VecDeque::new())),
            history: Arc::new(Mutex::new(VecDeque::new())),
            history_enabled: true,
            max_history: 1000,
            metrics_tracker: Arc::new(MetricsTracker::new()),
        }
    }

    /// Create event bus with custom history settings
    pub fn with_history(history_enabled: bool, max_history: usize) -> Self {
        Self {
            history_enabled,
            max_history,
            ..Self::new()
        }
    }

    /// Subscribe to events with immediate (synchronous) handling
    ///
    /// Handlers are called immediately when an event is published.
    /// Use this for time-critical event handling.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use moho_core::events::EventBus;
    /// # use moho_core::events::Event;
    /// # use std::any::Any;
    /// # #[derive(Clone, Debug)]
    /// # struct PlayerMoved { position: glam::Vec3 }
    /// # impl Event for PlayerMoved { fn as_any(&self) -> &dyn Any { self } }
    /// let bus = EventBus::new();
    ///
    /// bus.subscribe(|event: &PlayerMoved| {
    ///     println!("Player at: {:?}", event.position);
    /// });
    /// ```
    pub fn subscribe<E, F>(&self, handler: F)
    where
        E: Event,
        F: Fn(&E) + Send + Sync + 'static,
    {
        self.subscribe_with_priority(handler, 0)
    }

    /// Subscribe with custom priority (lower = higher priority).
    ///
    /// When multiple handlers subscribe to the same event type,
    /// they are executed in priority order (lowest number first).
    ///
    /// Note: this is the **opposite** convention from `InputDispatcher`, which uses
    /// higher numbers for higher priority. The two systems are independent.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use moho_core::events::EventBus;
    /// # use moho_core::events::Event;
    /// # use std::any::Any;
    /// # #[derive(Clone, Debug)]
    /// # struct GameEvent { message: String }
    /// # impl Event for GameEvent { fn as_any(&self) -> &dyn Any { self } }
    /// let bus = EventBus::new();
    ///
    /// // This runs first (priority 0)
    /// bus.subscribe_with_priority(|e: &GameEvent| {
    ///     println!("High priority: {}", e.message);
    /// }, 0);
    ///
    /// // This runs second (priority 10)
    /// bus.subscribe_with_priority(|e: &GameEvent| {
    ///     println!("Low priority: {}", e.message);
    /// }, 10);
    /// ```
    pub fn subscribe_with_priority<E, F>(&self, handler: F, priority: i32)
    where
        E: Event,
        F: Fn(&E) + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<E>();
        let handler_fn = Arc::new(handler) as HandlerFn<E>;
        let handler = Handler::new(handler_fn, priority);

        let mut handlers = self.sync_handlers.write().unwrap();
        handlers.entry(type_id).or_default().add(handler);
    }

    /// Publish event immediately (synchronous handlers called now)
    ///
    /// All subscribed handlers for this event type are called
    /// immediately in priority order.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use moho_core::events::EventBus;
    /// # use moho_core::events::Event;
    /// # use std::any::Any;
    /// # #[derive(Clone, Debug)]
    /// # struct PlayerDamaged { amount: u32 }
    /// # impl Event for PlayerDamaged { fn as_any(&self) -> &dyn Any { self } }
    /// let bus = EventBus::new();
    ///
    /// bus.publish(PlayerDamaged { amount: 10 });
    /// ```
    pub fn publish<E>(&self, event: E)
    where
        E: Event,
    {
        // Record metrics
        self.metrics_tracker.increment_published();

        // Add to history if enabled
        if self.history_enabled && event.should_record() {
            let mut history = self.history.lock().unwrap();
            history.push_back(format!("{:?}", event));

            // Limit history size
            while history.len() > self.max_history {
                history.pop_front();
            }
        }

        self.dispatch_to_handlers(&event);
    }

    /// Dispatch an event to all registered synchronous handlers.
    ///
    /// Shared by both `publish` (immediate) and `process_deferred` (end-of-frame).
    fn dispatch_to_handlers<E: Event>(&self, event: &E) {
        let type_id = TypeId::of::<E>();

        let handlers = self.sync_handlers.read().unwrap();
        if let Some(handler_list) = handlers.get(&type_id)
            && !handler_list.is_empty()
        {
            // Ensure handlers are sorted by priority
            drop(handlers);
            let mut handlers = self.sync_handlers.write().unwrap();
            if let Some(handler_list) = handlers.get_mut(&type_id) {
                handler_list.sort_by_priority();
            }
            drop(handlers);

            // Re-acquire read lock and execute
            let handlers = self.sync_handlers.read().unwrap();
            if let Some(handler_list) = handlers.get(&type_id) {
                for handler in handler_list.handlers() {
                    if let Some(handler_fn) = handler.downcast::<E>() {
                        handler_fn(event);
                        self.metrics_tracker.increment_processed();
                    }
                }
            }
        }
    }

    /// Publish event for deferred processing (processed at end of frame)
    ///
    /// Events are queued and processed when `process_deferred()` is called.
    /// Useful for events that don't need immediate handling.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use moho_core::events::EventBus;
    /// # use moho_core::events::Event;
    /// # use std::any::Any;
    /// # #[derive(Clone, Debug)]
    /// # struct StatUpdate { value: i32 }
    /// # impl Event for StatUpdate { fn as_any(&self) -> &dyn Any { self } }
    /// let bus = EventBus::new();
    ///
    /// // Queue for later processing
    /// bus.publish_deferred(StatUpdate { value: 100 });
    ///
    /// // Process at end of frame
    /// bus.process_deferred();
    /// ```
    pub fn publish_deferred<E>(&self, event: E)
    where
        E: Event,
    {
        let type_name = std::any::type_name::<E>().to_string();

        // Record metrics
        self.metrics_tracker.increment_published();

        // Record history at defer time so the event is visible even before
        // process_deferred() runs.
        if self.history_enabled && event.should_record() {
            let mut history = self.history.lock().unwrap();
            history.push_back(format!("{:?}", event));
            while history.len() > self.max_history {
                history.pop_front();
            }
        }

        let deferred = DeferredEvent {
            dispatch: Box::new(move |bus: &EventBus| {
                bus.dispatch_to_handlers(&event);
            }),
            type_name,
        };

        let mut queue = self.deferred_queue.lock().unwrap();
        queue.push_back(deferred);
    }

    /// Process all deferred events (call once per frame).
    ///
    /// Drains the deferred event queue and dispatches each event
    /// to registered handlers in FIFO order.
    pub fn process_deferred(&self) {
        let events: Vec<_> = {
            let mut queue = self.deferred_queue.lock().unwrap();
            queue.drain(..).collect()
        };

        for deferred in events {
            log::debug!("Processing deferred event: {}", deferred.type_name);
            (deferred.dispatch)(self);
        }
    }

    /// Get current metrics snapshot
    ///
    /// Returns a snapshot of event bus performance metrics including
    /// total events published/processed and queue sizes.
    pub fn metrics(&self) -> EventMetrics {
        let queue_size = self.deferred_queue.lock().unwrap().len();

        EventMetrics {
            total_published: self.metrics_tracker.get_published(),
            total_processed: self.metrics_tracker.get_processed(),
            by_type: HashMap::new(), // TODO: Track per-type stats
            avg_processing_time: HashMap::new(),
            peak_queue_size: queue_size,
            current_queue_size: queue_size,
        }
    }

    /// Clear event history
    ///
    /// Removes all events from the debug history buffer.
    pub fn clear_history(&self) {
        let mut history = self.history.lock().unwrap();
        history.clear();
    }

    /// Get event history (for debugging)
    ///
    /// Returns all events currently in the history buffer.
    pub fn history(&self) -> Vec<String> {
        let history = self.history.lock().unwrap();
        history.iter().cloned().collect()
    }

    /// Get last N events from history
    ///
    /// Returns up to N most recent events from the history buffer.
    pub fn recent_history(&self, n: usize) -> Vec<String> {
        let history = self.history.lock().unwrap();
        history.iter().rev().take(n).cloned().collect()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for EventBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let handler_count = self.sync_handlers.read().map(|h| h.len()).unwrap_or(0);
        let queue_size = self.deferred_queue.lock().map(|q| q.len()).unwrap_or(0);
        f.debug_struct("EventBus")
            .field("handler_types", &handler_count)
            .field("deferred_queue_size", &queue_size)
            .field("history_enabled", &self.history_enabled)
            .finish()
    }
}
