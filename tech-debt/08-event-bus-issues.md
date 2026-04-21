# TD-08: Event Bus — Performance and Convention Issues

**Priority:** Medium — the bus is the central nervous system; quality matters here.

## Problem

### A. Triple-lock dispatch pattern

`EventBus::dispatch_to_handlers` (called on every `publish`) acquires three locks per
call even when handlers are already sorted:

```rust
let handlers = self.sync_handlers.read().unwrap();  // lock 1
// ... check if list is non-empty ...
drop(handlers);
let mut handlers = self.sync_handlers.write().unwrap(); // lock 2 (sort)
// ... sort if needed ...
drop(handlers);
let handlers = self.sync_handlers.read().unwrap();  // lock 3 (execute)
```

This is three full lock round-trips for every event dispatch, even after the first
sort stabilizes. The claim of "11.4M events/sec" in docs is hard to reconcile with
this pattern — that benchmark was almost certainly single-threaded and without
contention.

Fix: sort on `subscribe`, not on dispatch. The `sorted` flag on `HandlerList` is the
right idea but is checked too late. A subscribe-time sort means `dispatch_to_handlers`
only ever needs one read-lock.

### B. History allocates a String per event

```rust
history.push_back(format!("{:?}", event));
```

This is called on every `publish` when `history_enabled` is true, and history is
enabled by default. For a 60 FPS game loop publishing dozens of events per frame,
this is tens of allocations per frame that end up in a `VecDeque` that is cleared
with `max_history = 1000`. In practice, this is only a debug feature — it should
default to disabled or be behind a `cfg(debug_assertions)` gate.

### C. Priority convention is inverted vs. InputDispatcher

`EventBus::subscribe_with_priority` docs say "lower = higher priority."
`InputDispatcher::register` (src/input_dispatcher.rs) uses higher numbers for higher
priority (priority 200 runs before 100 before 0). Same project, same word, opposite
meanings. At minimum this needs clear documentation; ideally one convention is chosen.

### D. `metrics()` returns empty per-type stats

```rust
by_type: HashMap::new(), // TODO: Track per-type stats
```

The `EventMetrics` type advertises per-type stats in its API but always returns empty
maps. Either implement it or remove the fields from `EventMetrics` to avoid a
misleading API surface.

## Acceptance Criteria

1. `dispatch_to_handlers` acquires at most one lock per call under normal conditions
   (handlers already sorted after first subscribe).
2. History is disabled by default (`history_enabled: false` in `EventBus::new()`),
   or gated on `cfg(debug_assertions)`.
3. Priority convention is documented in the `EventBus` module doc comment and either
   reconciled with `InputDispatcher` or the difference explicitly noted.
4. `EventMetrics::by_type` and `avg_processing_time` are either populated or removed.

## Files

- `moho_core/src/events/bus.rs` — dispatch, history, metrics
- `moho_core/src/events/handler.rs` — `HandlerList::add` / `sort_by_priority`
- `moho_core/src/events/metrics.rs` — `EventMetrics`
- `src/input_dispatcher.rs` — priority convention reference
