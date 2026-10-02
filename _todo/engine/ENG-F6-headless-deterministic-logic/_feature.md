# ENG-F6 — Game logic runs and is tested headless and deterministically

**Status:** proposed

## Summary

Agents can only verify what tests can observe. Game and engine logic should
build without windowing/GPU crates and produce the same results for the same
inputs, so behavior is assertable in plain `cargo test` and reproducible for
replays and future lockstep multiplayer.

## Exit criteria

- `cargo tree -p moho_core -p moho_game -p moho_sim -e normal` shows no
  `winit`, `wgpu` or `egui`.
- Simulation advances on a fixed timestep; render frame rate doesn't change
  simulation results.
- Logic that reads time takes it from an injected clock; no `Instant::now()`
  in domain rules (instrumentation and profiling excepted).
- Any randomness in logic comes from an explicitly seeded RNG.
- At least one property test (`proptest`) guards a core invariant, and one
  snapshot test (`insta`) guards a serialized format.

## Scope

- In: dependency cleanup, timestep and clock seams, test tooling adoption.
- Out: refactoring rendering or UI for testability; physics determinism
  (rapier) beyond fixed-step stepping.

## Items

| Item | Status |
|---|---|

## Notes

Survey (2026-10): `moho_core` declares `winit` but no source uses it.
`GameClock::tick(dt)` already takes `dt`, but the frame loop feeds it variable
frame time. `Instant::now()` appears in `moho_core` input timestamps and voxel
job/state code. No unseeded RNG. Global mutable state: four atomics —
`moho_ui::UI_OVERLAY_VISIBLE` (cross-crate flag read by the binary),
`FRAME_COUNTER` in the frame processor, and process-wide ID counters in
`moho_core` voxel state and `moho_renderer` lights (IDs depend on process
history, which breaks replay determinism).
