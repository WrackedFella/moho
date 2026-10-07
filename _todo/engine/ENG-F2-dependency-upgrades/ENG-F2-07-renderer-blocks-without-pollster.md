# Renderer device setup blocks on async without pollster

**Feature:** [ENG-F2](_feature.md)
**Issue:** #100

## Summary

ENG-F13 call: replace `pollster`'s two `block_on` calls in renderer device
setup with an in-house `block_on` of about 25 lines
([audit](../ENG-F13-dependency-audit/audit.md)).

## Deliverables

- No workspace crate declares `pollster`.
- An engine-owned `block_on` drives the adapter and device requests.

## Acceptance criteria

- [ ] A future that is already ready completes on the first poll.
- [ ] A future that is pending until woken from another thread completes after the wake.
- [ ] The renderer initialises as before (existing renderer tests pass; app starts).

## Tech spec

**Design.**
- Owner: `moho_renderer`, as a private module `block_on` exposing `pub(crate) fn block_on<F: Future>(future: F) -> F::Output`. Its two calls in `device.rs` (`request_adapter`, `request_device`) are the only consumers. It moves to a shared engine crate when a second crate needs it.
- Implementation, std only and with no `unsafe` (the workspace denies `unsafe_code`):
  - `struct ThreadWaker(std::thread::Thread)` implements `std::task::Wake`, where `wake` calls `unpark`.
  - `block_on` pins the future with `std::pin::pin!`, builds a `Waker` from `Arc<ThreadWaker>`, and loops `poll` → `Pending => std::thread::park()`.
  - The park token makes a wake that arrives before the park safe. A spurious unpark just polls again.
- Drop `pollster` from `moho_renderer`, the root manifest (if [#96](https://github.com/WrackedFella/moho/issues/96) hasn't already removed it there) and `[workspace.dependencies]`.

**Out of scope.**
- An async runtime or executor, timers, and `block_on` for wasm.
- Restructuring device setup or its errors.

**Test map.**
| Criterion | Test | Gate class |
|---|---|---|
| A ready future completes on the first poll | `moho_renderer::block_on::tests::ready_future_returns_its_value` | glue |
| A future pending until woken from another thread completes after the wake | `moho_renderer::block_on::tests::future_woken_from_another_thread_completes` (a future that stores its waker, then a spawned thread sets the value and wakes) | glue |
| Edge: a wake before the park doesn't deadlock | `moho_renderer::block_on::tests::wake_before_park_does_not_deadlock` (the future wakes itself and then returns `Pending` once) | glue |
| Edge: a panic propagates | `moho_renderer::block_on::tests::panicking_future_propagates_panic` (`#[should_panic]`) | glue |
| The renderer initialises as before | existing renderer tests; Verification | glue |

**Gate class:** glue.

**Risks.**
- Low. If wgpu 30 ([#101](https://github.com/WrackedFella/moho/issues/101)) or a job pool brings in an executor, use that one and delete this module.

## Verification

1. `cargo run`: the window opens and the start menu and terrain render, as before.
