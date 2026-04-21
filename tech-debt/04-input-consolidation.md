# TD-04: Consolidate InputDispatcher and InputRouter

**Priority:** High — two parallel input systems causes confusion and dead code.

## Problem

There are two independent input routing systems:

| System | File | Status |
|---|---|---|
| `InputDispatcher` | `src/input_dispatcher.rs` | Active — registered in `setup_renderer_and_ui`, dispatching events in `window_event` |
| `InputRouter` | `src/input_routing.rs` | Designed and tested, **not wired in** — every public API is `#[allow(dead_code)] // TODO: integrate with InputDispatcher` |

`InputRouter` has a richer layered model (Modal → Console → Menu → GameUI → Game)
and good test coverage. `InputDispatcher` is a simpler priority-ordered list of
closures with raw integers for priorities (0, 100, 200 used in practice).

Running both in parallel means `InputRouter::update_for_state` is called on state
transitions but its `dispatch` is never invoked — the layer tracking is wasted work.

There is also a **priority convention mismatch**: `InputDispatcher` uses higher
numbers for higher priority, while `EventBus::subscribe_with_priority` uses lower
numbers for higher priority. Both exist in the same codebase with the same word
"priority" meaning opposite things.

## Acceptance Criteria

1. One input routing system remains. Either:
   - **Keep `InputRouter`:** Wire its `dispatch` into `window_event`, replace the
     `InputDispatcher` closure registrations with `InputRouter::register_handler`
     calls, delete `InputDispatcher`.
   - **Keep `InputDispatcher`:** Delete `InputRouter` and its tests, update
     `update_for_state` to only track state without a dead dispatch path.
   
   Recommendation: keep `InputRouter` — its layered model is better and will age
   better as more UI states are added.

2. Priority conventions are documented and consistent. If `InputRouter` wins, add a
   module-level doc comment explaining the layer ordering. If they must coexist with
   `EventBus`, add a comment in `EventBus` noting that *its* priority uses a
   lower-is-higher convention (opposite to `InputRouter`).

3. `src/input_routing.rs` has no `#[allow(dead_code)]` annotations.

## Files

- `src/input_dispatcher.rs`
- `src/input_routing.rs`
- `src/main.rs` — `setup_renderer_and_ui` (dispatcher registration)
- `src/app/event_loop/window_event_handler.rs` — event dispatch call site
