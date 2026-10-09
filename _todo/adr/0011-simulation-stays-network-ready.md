# 0011 — The simulation stays network-ready

**Status:** Accepted (2026-10-08)

## Context

Multiplayer is not planned and may never ship. What makes it expensive is
retrofitting: a simulation built without a few rules can't be replayed,
synchronised or rolled back without a rewrite. The same rules give replays,
headless determinism tests and (later) scripted mods for free, so keeping them
costs little now.

[ADR-0009](0009-simulation-time-is-one-fixed-tick.md) already makes the fixed tick the only source of simulation time and
[ADR-0004](0004-entity-storage-without-a-general-ecs.md) keeps entities in plain typed stores. Four gaps remain:

- Input reaches the simulation as live device state, not as a per-tick value.
- Nothing says simulation state must be free of renderer, audio and UI handles.
- Some ids in the simulation depend on process history (process-wide counters
  for voxel jobs and state, noted in [ENG-F6](https://github.com/WrackedFella/moho/issues/233)). Renderer light ids are outside
  the simulation and unaffected.
- The event bus carries both notifications and requests, and nothing says
  whether a bus event may change simulation state.

## Decision

1. **The tick is driven by commands.** Simulation state advances only through
   the fixed tick ([ADR-0009](0009-simulation-time-is-one-fixed-tick.md)). Its input is a per-tick command set (game
   actions plus the look delta accumulated since the previous tick), never raw
   device events or live device state.
2. **Simulation state is plain data.** It lives in typed stores
   ([ADR-0004](0004-entity-storage-without-a-general-ecs.md)), is serialisable, and holds no handle into the renderer, audio
   or UI.
3. **Ids are stable values.** Entity and job ids in simulation state come from
   the simulation (a per-world sequence or position), never from a pointer or a
   process-wide counter.
4. **Presentation reads, never writes.** Rendering, audio, UI and
   interpolation read simulation state and never modify it. Per-frame mouselook
   and camera smoothing stay presentation ([ADR-0009](0009-simulation-time-is-one-fixed-tick.md)).
5. **The event bus carries notifications, not changes.** A bus event may tell
   presentation that something happened. It never mutates simulation state
   directly. A request that must change state (a UI "load world", a console
   command) is translated into a command applied at the next tick.

No netcode, transport, prediction or rollback is built or chosen here.

## Consequences

- Replays and determinism tests need only the initial state and the command
  sequence.
- A later netcode ADR starts from a compliant simulation and picks the model
  (lockstep, snapshots or rollback) on a named consumer.
- Scripted mods ([ENG-F19](https://github.com/WrackedFella/moho/issues/231)'s deferred ADR) can change state only by emitting
  commands, so a mod can't break determinism by reaching around the tick.
- The shared loop ([ENG-F11](https://github.com/WrackedFella/moho/issues/81)) must expose commands as the tick's input, and the
  input feature ([ENG-F12](https://github.com/WrackedFella/moho/issues/82)) must be able to produce them.
- Existing code that breaks rule 3 or 5 (process-wide id counters in voxel
  state; bus events that load worlds or change simulation state directly) is
  fixed when its owning feature next reworks it, not in a sweep.
- Cost: UI and console actions take effect on the next tick (at most one tick
  of latency), and per-tick commands are a type each game defines.
