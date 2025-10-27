use super::handler::{Handler, HandlerFn, HandlerList};
use super::metrics::{EventMetrics, MetricsTracker};
use super::Event;
use std::any::TypeId;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};

/// Deferred event wrapper for processing later
struct DeferredEvent {
    event: Box<dyn Any + Send>,
    type_id: TypeId,
    type_name: String,
}

use std::any::Any;

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

    /// Subscribe with custom priority (lower = higher priority)
    ///
    /// When multiple handlers subscribe to the same event type,
    /// they are executed in priority order (lowest number first).
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
        handlers
            .entry(type_id)
            .or_insert_with(HandlerList::new)
            .add(handler);
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
        let type_id = TypeId::of::<E>();

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

        // Execute synchronous handlers
        let handlers = self.sync_handlers.read().unwrap();
        if let Some(handler_list) = handlers.get(&type_id) {
            if !handler_list.is_empty() {
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
                            handler_fn(&event);
                        }
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
        let type_id = TypeId::of::<E>();
        let type_name = std::any::type_name::<E>().to_string();

        // Record metrics
        self.metrics_tracker.increment_published();

        let deferred = DeferredEvent {
            event: Box::new(event),
            type_id,
            type_name,
        };

        let mut queue = self.deferred_queue.lock().unwrap();
        queue.push_back(deferred);
    }

    /// Process all deferred events (call once per frame)
    ///
    /// Drains the deferred event queue and processes each event
    /// in FIFO order. Call this at the end of each game frame.
    ///
    /// # Note
    ///
    /// Current implementation has limited type-safe deferred processing.
    /// Events are logged but handlers may not execute properly.
    /// This will be improved in future versions.
    pub fn process_deferred(&self) {
        let mut queue = self.deferred_queue.lock().unwrap();
        let events: Vec<_> = queue.drain(..).collect();
        drop(queue);

        for deferred in events {
            self.metrics_tracker.increment_processed();

            // Note: Deferred event processing is simplified for now
            // Full type-safe processing requires more complex trait bounds
            log::debug!(
                "Processing deferred event: {} (full type-safe processing pending)",
                deferred.type_name
            );
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

// EventBus is Send + Sync for cross-thread usage
unsafe impl Send for EventBus {}
unsafe impl Sync for EventBus {}
