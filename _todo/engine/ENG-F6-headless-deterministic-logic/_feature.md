# ENG-F6 — Game logic runs and is tested headless and deterministically

**Status:** parked (no current defect; reopen when a feature's rules need it, see Notes)

## Summary

Agents can only verify what tests can observe. Game and engine logic should
build without windowing/GPU crates and produce the same results for the same
inputs, so behavior is assertable in plain `cargo test` and reproducible for
replays and future lockstep multiplayer.

## Exit criteria

- Simulation advances on a fixed timestep; render frame rate doesn't change
  simulation results.
- Simulation time comes only from the fixed tick ([ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md)): a test advances
  time by stepping ticks, without sleeping, and `GameClock` advances only
  through the tick. No `Instant::now()` in domain rules (instrumentation and
  profiling excepted).
- Any randomness in logic comes from an explicitly seeded RNG.

## Scope

- In: decoupling the fixed tick from rendering (one seam, [ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md));
  seeded randomness.
- Out: dependency cleanup (gate G2, [ENG-F8](../ENG-F8-foundation-gate/_feature.md)); `proptest`/`insta` adoption
  ([ENG-F9](../ENG-F9-tests-prove-behaviour/_feature.md)); refactoring rendering or UI for testability; physics determinism
  (rapier) beyond fixed-step stepping.

## Items

| Item | Status |
|---|---|

## Notes

Parked 2026-10-04: a check against the code found no current defect. The
frame loop already advances simulation by a constant 1/60 s step (fixed
timestep, coupled to the frame loop); wall-clock reads feed only fields no
logic reads, or timing instrumentation; the process-wide counters (`JobId`,
renderer light IDs) aren't simulation state; there is no RNG. The `winit`
criterion is gate G2 ([ENG-F8](../ENG-F8-foundation-gate/_feature.md)); `proptest`/`insta` adoption is [ENG-F9](../ENG-F9-tests-prove-behaviour/_feature.md). Reopen when
a feature needs rules that read elapsed time from a test-controlled clock,
or when render and simulation rates must decouple. The design for that is
[ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md) (accepted).

Clock accessors on `moho_game::simulation::SimulationController` (found while
moving it, 2026-10-04): `with_clock` and `game_clock_mut` have no production
caller, only tests. The `game_clock` field is `pub`, so the clock can be reached
directly as well as through `game_clock()`. `set_time_of_day` is a write path
outside the tick, used by scene load (`src/app/scene_loader.rs`) and the debug
console (`src/app/event_loop/event_processor.rs`). When this reopens and
`GameClock` advances only through the tick, drop the unused accessors, make the
field private, and decide whether those two writes stay as explicit resets or go
through the tick. `controller_input` has the same `pub`-field-vs-accessor split.

Original survey (2026-10): `moho_core` declares `winit` but no source uses it.
`GameClock::tick(dt)` takes `dt`. `Instant::now()` appears in `moho_core` input timestamps and voxel
job/state code. No unseeded RNG. Global mutable state: four atomics —
`moho_ui::UI_OVERLAY_VISIBLE` (cross-crate flag read by the binary),
`FRAME_COUNTER` in the frame processor, and process-wide ID counters in
`moho_core` voxel state and `moho_renderer` lights (IDs depend on process
history, which breaks replay determinism).
