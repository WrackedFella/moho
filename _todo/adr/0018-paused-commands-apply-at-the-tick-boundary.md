# 0018 — Paused commands apply at the tick boundary

**Status:** Proposed (narrows [0011](0011-simulation-stays-network-ready.md) rule 1)

## Context

[ADR-0011](0011-simulation-stays-network-ready.md) lets simulation state change only inside a tick. A strategy game must
let the player give orders while paused and see them take effect
([ENG-F11-04](https://github.com/WrackedFella/moho/issues/334)), and pausing means no tick runs.

## Decision

1. Pause and speed are loop state, not simulation state. Speed changes how many
   ticks run per second of real time, never the tick length ([ADR-0009](0009-simulation-time-is-one-fixed-tick.md)).
2. While paused, the loop may hand a command to the game without running a tick.
   The game applies it at the boundary before the next tick; the tick index and
   simulation time do not move.
3. A paused command uses no elapsed time (no `dt`): its effect depends only on the
   state at that boundary and the command.
4. A replay or network log records paused commands against the next tick's index,
   in order, and applies them before that tick's own command.

## Consequences

- Replays stay complete with the initial state plus commands; a paused order is a
  command stamped with a boundary.
- A later lockstep model treats pause and speed as session control, outside the
  simulation.
- Games opt in per command field; anything time-based waits for the next tick.
