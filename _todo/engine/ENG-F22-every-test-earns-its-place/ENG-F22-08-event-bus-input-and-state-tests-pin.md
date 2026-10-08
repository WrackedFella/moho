# Event bus, input and state tests pin their contracts

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#175](https://github.com/WrackedFella/moho/issues/175)
**Status:** Backlog
**Gate class:** domain
**Labels:** line:engine

## Summary

The event bus, input filtering, state coordinator and prefs warning tests pin their documented contracts: synchronous dispatch without sleeps, exact history, the deferred queue, the input filter, the full transition matrix and every warning's text. The event-bus tests that live in the binary move to `moho_core`. Each test is shown red against the wrong implementation it targets.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.
- `moho_core/src/events/tests.rs` has no `sleep`.
- `tests/event_bus_integration.rs` is deleted after its two unique tests move to `moho_core` (its other three duplicate `moho_core`'s tests).

## Acceptance criteria

| Test | Pins | Red against |
|---|---|---|
| `moho_core::events` `test_basic_publish_subscribe` | counter is 5 right after the first `publish` and 8 after the second, no sleep (handlers run synchronously on the publisher's thread) | dispatch on another thread |
| `moho_core::events` `test_event_history` | `history() == ["TestEvent { value: 1 }", "TestEvent { value: 2 }"]` | each event recorded twice |
| `moho_core::events` `test_no_subscribers` | after `process_deferred`: `total_published == 2`, `total_processed == 0` | processed counted per publish |
| `moho_core::events` `frame_and_mouse_move_events_are_not_recorded` (moved from the binary's `test_frame_lifecycle_events`) | publish `FrameStart`, `FrameEnd`, `SystemEvent::Started`, `InputEvent::MouseMoved` and one other input event → history holds exactly the two recordable ones | `should_record` always true |
| `moho_core::events` `test_metrics_tracking` (moved from the binary) | as today | as today |
| `moho_core::events` `history_is_off_by_default` | `EventBus::new()` records nothing | history always on |
| `moho_core::events` `history_cap_evicts_oldest` | `with_history(true, 3)` and 5 events, history holds events 3, 4, 5 | newest evicted |
| `moho_core::events` `deferred_events_dispatch_in_fifo_order_once` | three deferred events arrive in order on `process_deferred`; a second call dispatches nothing | LIFO; queue not drained |
| `moho_core::events` `deferred_event_is_recorded_when_deferred` | history holds the event before `process_deferred` runs | recorded at dispatch |
| `moho_core::input` `test_input_accumulation` | filter off: (1,0)+(1,1) → (2.0, 1.0), then (0.0, 0.0) | smoothing applied with the filter off |
| `moho_core::input` `test_sensitivity_scaling` | filter off, sensitivity 2, input (1,3) → (2.0, 6.0) | axes swapped |
| `moho_core::input` `filter_smooths_across_frames`, `filter_resets_on_idle_frame`, `deadzone_zeroes_tiny_output`, `clear_pending_input_discards_accumulated_delta` | smoothing uses the previous output; an idle frame resets it; output below 0.01 reads 0; after `clear_pending_input` the next sample equals a fresh `InputSystem`'s | previous output never updated; reset skipped; deadzone removed; clear is a no-op |
| `moho_types::state_coordinator` `test_enter_console` | the whole `StateTransitionActions` with `show_menu: None` | `menu_for_state(ConsoleOpen) == Some("start")` |
| `moho_types::app_state` `can_transition_to_matrix` | all 16 `GameState` pairs, expected per `can_transition_to`'s rustdoc | `Menu → Paused` allowed |
| `moho_core::prefs::reader` `warning_display_names_file_and_detail_for_each_issue` | each of the 7 `PrefsIssue` arms' text contains the file and its section, key, value or reason | `{section}` dropped from one arm |

## Tech spec

**Design:** tests only. Build `PrefsWarning` values directly (no file system).

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- Binding parse and save tests, the proptest's key-code domain, `moho_input`'s mapper: #149 (ENG-F12-02).
- `pause_game`, `resume_game` and the coordinator "valid from" docs: deferred.
- `recent_history`: deleted in #168 (ENG-F22-01).

**Test map:** the table above.

**Gate class:** domain (`moho_core` and `moho_types` contracts).

**Risks:** if a row of the transition matrix disagrees with the rustdoc, that is a defect to report, not to pin.
