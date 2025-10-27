use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Performance metrics for event bus
#[derive(Debug, Clone)]
pub struct EventMetrics {
    /// Total events published
    pub total_published: u64,

    /// Total events processed (deferred)
    pub total_processed: u64,

    /// Events published per type
    pub by_type: HashMap<String, u64>,

    /// Average processing time per event type
    pub avg_processing_time: HashMap<String, Duration>,

    /// Peak events in deferred queue
    pub peak_queue_size: usize,

    /// Current events in deferred queue
    pub current_queue_size: usize,
}

impl EventMetrics {
    pub fn new() -> Self {
        Self {
            total_published: 0,
            total_processed: 0,
            by_type: HashMap::new(),
            avg_processing_time: HashMap::new(),
            peak_queue_size: 0,
            current_queue_size: 0,
        }
    }

    pub fn record_publish(&mut self, event_type: &str) {
        self.total_published += 1;
        *self.by_type.entry(event_type.to_string()).or_insert(0) += 1;
    }

    pub fn record_process(&mut self, event_type: &str, duration: Duration) {
        self.total_processed += 1;

        // Calculate running average
        let current_avg = self
            .avg_processing_time
            .get(event_type)
            .copied()
            .unwrap_or(Duration::ZERO);

        let count = self.by_type.get(event_type).copied().unwrap_or(1) as u32;
        let new_avg = (current_avg * (count - 1) + duration) / count;

        self.avg_processing_time
            .insert(event_type.to_string(), new_avg);
    }

    pub fn update_queue_size(&mut self, size: usize) {
        self.current_queue_size = size;
        if size > self.peak_queue_size {
            self.peak_queue_size = size;
        }
    }
}

impl Default for EventMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe metrics tracker
pub struct MetricsTracker {
    total_published: AtomicU64,
    total_processed: AtomicU64,
}

impl MetricsTracker {
    pub fn new() -> Self {
        Self {
            total_published: AtomicU64::new(0),
            total_processed: AtomicU64::new(0),
        }
    }

    pub fn increment_published(&self) {
        self.total_published.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_processed(&self) {
        self.total_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_published(&self) -> u64 {
        self.total_published.load(Ordering::Relaxed)
    }

    pub fn get_processed(&self) -> u64 {
        self.total_processed.load(Ordering::Relaxed)
    }
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}
