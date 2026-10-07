# Event and job channels use the standard library

**Feature:** [ENG-F2](_feature.md)
**Issue:** #99

## Summary

ENG-F13 call: drop `crossbeam-channel`. The workspace uses only unbounded
send/receive, `try_recv`, `try_iter` and `recv_timeout`, all of which
`std::sync::mpsc` provides ([audit](../ENG-F13-dependency-audit/audit.md)).

## Deliverables

- No workspace crate declares `crossbeam-channel`.

## Acceptance criteria

- [ ] Event routing, generation jobs and UI adapter channels behave as before: existing tests pass.
- [ ] `just check` passes with `crossbeam-channel` absent from every manifest.

## Tech spec

**Design.**
- Swap `crossbeam_channel::{unbounded, Sender, Receiver}` for `std::sync::mpsc::{channel, Sender, Receiver}` in the binary (`app/event_setup`, `app/generation_job`, `app/initializer`, `app/input_state`, `app/renderer_setup`, `app/world_generator`, `input_dispatcher`, `main`) and in `moho_ui` (`adapter::UiReceiver`, `adapter/event_routing` tests). Drop `crossbeam-channel` from the root and `moho_ui` manifests and from `[workspace.dependencies]`.
- `event_setup::tests::test_channels_are_initially_empty` uses `Receiver::is_empty`, which std doesn't have. Assert `matches!(rx.try_recv(), Err(TryRecvError::Empty))` instead.
- Checked on 2026-10-07 against a copy of `dev`: after the swap, `cargo check --workspace --all-targets --all-features` fails only on those three `is_empty` calls. No site needs `Receiver: Sync`, a cloned `Receiver`, `select!` or a bounded channel. The `EventBus` subscriber closures capture `Sender`, which has been `Sync` since Rust 1.72.

**Out of scope.**
- Changing channel topology (bounded channels, merging the five event channels), the event bus, and the generation job design.
- A hand-rolled `select`. If a later job pool needs to wait on several channels, that need reopens the ENG-F13 verdict.

**Test map.**
| Criterion | Test | Gate class |
|---|---|---|
| Event routing behaves as before | existing `moho::app::event_setup::tests::*`, `moho::input_dispatcher::tests::*` | glue |
| UI adapter channels behave as before | existing `moho_ui::adapter::event_routing::tests::*` (the `recv_timeout` test) | glue |
| Draining after the sender drops can't spin the frame loop | `moho::app::event_setup::tests::try_recv_after_bus_dropped_reports_disconnected` | glue |
| `crossbeam-channel` absent | `just check`; `cargo tree --workspace -i crossbeam-channel` lists no workspace crate as a direct dependent (it may stay transitive) | glue |

**Gate class:** glue.

**Risks.**
- Low. std's `Receiver` is `!Sync`. A future `Arc<Receiver>` won't compile, which is the right failure.
- Conflicts with [#98](https://github.com/WrackedFella/moho/issues/98) in the same files; whichever lands second rebases mechanically.

## Notes

If a later job pool needs to wait on several channels at once (`select!`),
that need reopens the verdict; don't hand-roll a select.
