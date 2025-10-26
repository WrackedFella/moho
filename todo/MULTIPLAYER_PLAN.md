## MULTIPLAYER_PLAN (moved to todo/)

This plan was moved from the repository root into `todo/` to keep active, high-value work visible.

Status summary (2025-10-26)
- Phase 1 — Extract Game Simulation: DONE
  - A headless simulation crate (`moho_sim`) exists with deterministic tests and a `SimulationController` wrapper.
  - The application was refactored to call into the simulation layer.
- Phase 2 — Input Abstraction: DONE
  - `PlayerInput` command types and input mapping are implemented in `moho_sim`.
  - Tests exist for input mapping and determinism.
- Phase 4 — State Serialization & Snapshotting: DONE
  - Snapshot/restore implemented using bincode (2.x) with CRC32 header.
  - Unit tests cover roundtrip and corruption detection.
- Phase 3 — Multiplayer Foundation: PARTIAL / PENDING
  - The codebase is now ready for multiple players conceptually, but we still need:
    - a `PlayerId` canonical type in core types, wired into ECS/player entities
    - persistent mapping of `PlayerController` → `PlayerId` (ECS component or map)
    - camera/system changes to follow a chosen local player (configurable)
  - Networking (host/client, input forwarding, state sync) has NOT been implemented.

Guiding decisions
- Keep the simulation pure and deterministic. Network code should only forward/receive `PlayerInput` commands and apply them in the simulation layer.
- Prefer the authoritative host model for initial implementation (simpler to reason about). Peer-to-peer can be considered later.

Immediate next steps (high-value, short scope)
1. Introduce a canonical `PlayerId` type in `moho_core` and export it for use in `moho_sim`. (small API change)
2. Add `PlayerId` support to player entity/component storage (replace single hardcoded controller with a map/component). (modest refactor)
3. Add a local test harness that spawns 2 simulated players feeding prerecorded `PlayerInput` sequences to `moho_sim` and asserts deterministic state convergence. (test-driven)

Medium-term next steps (networking)
1. Prototype a minimal input-forwarding protocol (UDP/TCP) that sends serialized `PlayerInput` from clients to host.
2. Host applies inputs in order, periodically snapshots state and broadcasts deltas.
3. Implement client prediction & interpolation only if necessary for perceived latency.

References
- See `moho_sim/` for current simulation code and `moho_sim/tests/` for snapshot and input tests.
- For serialization and snapshot format: see `moho_sim::snapshot` implementation (bincode 2.x + crc32fast 1.5).
- For CI and build info: see `docs/` and `.github/CICACHE.md`.

If there's no further interest in implementing multiplayer in the near term, this file may be archived in git history and removed from `todo/`. For now I've placed it under `todo/` so it's easy to pick up and iterate from the current workspace state.

---
Updated: 2025-10-26 — moved and reconciled status to current workspace.
