# 0022 — Character changes are tick input

**Status:** Accepted (on merge); supersedes [0014](0014-characters-move-by-velocity.md) (accepted 2026-10-12)

## Context

[ADR-0014](0014-characters-move-by-velocity.md) split character movement into a velocity in state, rules in one pure step, and impacts reported. Its file still read Proposed although it was accepted on 2026-10-12. Three `PhysicsWorld` setters (position, added velocity, look) wrote character state outside the step. A replay or a predicting client of the command stream had no record of those writes, and when two systems wrote in one tick, call order decided the result. [ADR-0011](0011-simulation-stays-network-ready.md) promises that replays need only the initial state and the command sequence.

## Decision

1. **A character carries a velocity.** Velocity is part of the character's simulation state and persists between ticks.
2. **The movement step owns the rules.** Gravity, jumping, ground friction, and ground and air acceleration live in one movement step, driven by the per-tick command ([ADR-0011](0011-simulation-stays-network-ready.md)) and by movement settings the game supplies as data. `PhysicsWorld` provides collide-and-slide and queries; it holds no movement rules.
3. **Velocity is clipped against what the slide hits.** Speed into a surface is removed; speed along it is kept.
4. **The step reports impacts; the game decides what they mean.** Each impact names the surface, its normal and the speed lost into it. Damage, fall damage and sounds are game rules built on that report.
5. **The step is a function of (character state, command, queued effects, settings, collision queries).** It reads nothing else and writes only the character's state.
6. **Every change to a character from outside the step is a queued effect.** Effects are plain, serialisable data, queued per character and applied in queue order at the start of the character's next step. `PhysicsWorld` exposes no setter for character state; a character's starting state is given when it is added.

## Consequences

- A replay or a predicting client records (initial state, and per tick the command and the effects). Nothing else changes a character.
- Two systems acting on one character in a tick compose in queue order, not call order.
- External changes take effect on the next tick. Presentation that must not blend across a discontinuity (a teleport) is told so by the code that queued it.
- An effect that comes from server authority (another player's hit) is a correction, not a prediction; a netcode ADR decides how corrections enter. Nothing here chooses netcode.
- Later movement rules (modes, assists, moving ground, pushing) state where in the step they run; the step's order is recorded in its documentation.
