use std::sync::atomic::{AtomicU64, Ordering};

/// Performance metrics for event bus
#[derive(Debug, Clone)]
pub struct EventMetrics {
    /// Total events published
    pub total_published: u64,

    /// Total events processed by synchronous handlers
    pub total_processed: u64,

    /// Peak events in deferred queue
    pub peak_queue_size: usize,

    /// Current events in deferred queue
    pub current_queue_size: usize,
}

/// Thread-safe metrics tracker
#[derive(Debug)]
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
