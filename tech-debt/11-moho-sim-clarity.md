# TD-11: Clarify moho_sim — Two Simulations in One Crate

**Priority:** Low-Medium — the crate's stated purpose doesn't match what it contains.

## Problem

`moho_sim` exports two completely unrelated things under one crate name:

1. **`Simulation` + `PlayerInput`** — a tiny integer-grid headless simulation for
   determinism testing. This is what the crate doc says it is: "headless target for
   PlayerInput and determinism tests."

2. **`SimulationController`** — a full 3D float-based controller wrapping
   `PlayerController`, `ControllerInput`, `GameClock`, and snapshot/restore with a
   versioned binary format. This is the real runtime simulation used by the main
   binary every frame.

These have no shared logic. `SimulationController` is not "headless" (it depends on
`moho_core::controller`, `moho_core::game_clock`, and `glam::Mat4`). It is also
not "integer-only" — it uses `f32` floats throughout, including for position, yaw,
pitch, movement, and the game clock. The determinism claim in CLAUDE.md's crate
description applies to `Simulation` (toy), not to `SimulationController` (runtime).

Additionally, the crate exposes `PlayerInput as PlayerInputType` — a type alias for
a re-export that aliases to itself: `pub use PlayerInput as PlayerInputType`.

## Acceptance Criteria

Option A — Extract and rename:
- Move `SimulationController`, `controller_adapter`, and related snapshot logic into
  the main binary or into a new `moho_controller` crate (coordinate with TD-05).
- Keep `moho_sim` as the small headless harness it claims to be.
- `SimulationController` is documented accurately (float-based, depends on
  moho_core types, not integer-deterministic).

Option B — Rename and document:
- Rename the module structure to make the distinction clear:
  `moho_sim::headless::Simulation` vs `moho_sim::controller::SimulationController`.
- Add a crate-level doc that honestly describes both components.
- Fix the CLAUDE.md description to match.

Either option: remove `pub use PlayerInput as PlayerInputType`.

## Files

- `moho_sim/src/lib.rs` — crate structure and exports
- `moho_sim/src/simulation.rs` — `SimulationController`
- `CLAUDE.md` — description of moho_sim
