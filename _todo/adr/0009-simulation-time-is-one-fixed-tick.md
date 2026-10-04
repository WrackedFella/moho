# 0009 — Simulation time comes from one fixed tick

**Status:** Accepted

## Context

ENG-F6 asks for a fixed timestep and for logic to read time from an injected
clock. In 2026-10, time flows like this:

- The frame loop passes a constant `dt` (`frame_duration`, hard-coded to
  1/60 s) and advances `last_frame` by that amount, so the simulation already
  steps at 60 Hz. But each step is tied to one rendered frame. A machine that
  falls behind catches up by rendering extra frames, with no cap, and changing
  the frame cap would change the step size and therefore the results.
- `GameClock` is the in-game time of day. It advances by whatever `dt` it is
  given and reads no time itself.
- No domain rule reads wall-clock time. `Instant::now()` in `moho_core` only
  feeds instrumentation (light-job and generation timings, logged or counted)
  or fields nobody reads (`input` frame timestamps, voxel state
  `last_modified`). The light-job budget counts blocks, not microseconds.

## Decision

- **One seam.** A fixed simulation tick is the only source of simulation
  time. Domain code receives time only as tick input (the constant tick
  length, or the tick count). There is no separate clock trait.
- **Tick ownership.** The tick rate is a `moho_game` constant (60 Hz). The
  binary's frame loop runs an accumulator: zero or more fixed ticks per
  rendered frame, with a cap on catch-up ticks per frame, so rendering no
  longer sets the simulation rate. Physics steps on the same tick.
- **`GameClock`** stays the in-game calendar and advances only through the
  tick. It is a consumer of simulation time, not a source.
- **Wall-clock reads** in `moho_core` and `moho_game` are allowed only for
  instrumentation, meaning durations that are logged or emitted as metrics
  and never feed a decision. Dead timestamp fields are deleted, not injected.
- **Presentation** (camera interpolation between ticks, per-frame mouselook)
  may use frame time. It lives in the binary or renderer, never in a domain
  rule.

## Consequences

- Replays and determinism tests only need the per-tick input sequence; there
  is no clock to fake.
- [ENG-F6](../engine/ENG-F6-headless-deterministic-logic/_feature.md)'s "injected clock" criterion becomes "simulation time comes only
  from the tick". ENG-F6's survey note claiming variable frame time is
  corrected.
- If a future rule needs real time (for example, a real-time event in
  multiplayer), that is a new ADR. It would then introduce a time source
  that the tick feeds, not a second independent clock.
- Higher frame rates need camera interpolation, or movement will judder
  between ticks. That is presentation work in ENG-F6, not a reason for
  variable-step simulation.
