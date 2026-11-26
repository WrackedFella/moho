# Determinism

**Source:** `moho_core/src/sim_math.rs`, `moho_core/src/rng.rs`, `scripts/check-sim-math.sh`, `scripts/sim-math.txt`,
`scripts/check-physics-features.sh`, `deny.toml`, workspace `Cargo.toml` (`glam`, `rapier3d`), `moho_physics/tests/replay.rs`.
**Decision:** [ADR-0043](../../_todo/adr/0043-simulation-transcendentals-come-from-one-libm-source.md). **Rationale:** feature [#714](https://github.com/WrackedFella/moho/issues/714).

Simulation state must replay from its inputs. How far a replay holds depends on what
may differ between two runs; the levels name that.

## 1 Levels

| Level | Same result when | Needs |
|---|---|---|
| L0 | Same binary, same machine | Fixed order of float work, seeded randomness ([`SimRng`](overview.md)), no wall-clock or OS input in the sim |
| L1 | Same binary, same instruction set | L0, plus no dependence on the OS or its math library |
| L2 | Same source, any build | L1, plus no compiler-chosen float fusion or vector width: no `target-cpu` or `+fma` flags, no SIMD-only paths |
| L3 | Any instruction set and OS | L2, plus every float operation the sim uses is correctly specified on every target |

## 2 What the engine guarantees today

L0, proven for the physics step and character movement by `moho_physics/tests/replay.rs`
(every tick matches bit for bit in a fresh world). The OS-dependent inputs to the sim
are removed:

- glam's `libm` feature (workspace `Cargo.toml`) puts glam's own sine and cosine, as
  in `Quat::from_rotation_y`, on `libm`.
- rapier3d is built with `enhanced-determinism`, which keeps parry and the step off the
  platform math library; `scripts/check-physics-features.sh` (run by `just deny`) fails
  unless rapier3d and parry3d resolve it.
- Character movement and the searchlight lean offset take sine and cosine from
  `moho_core::sim_math`.

This is the groundwork for L1. Cross-OS agreement is reported per CI run, not gated
(section 6).

## 3 Rules

- Simulation code takes transcendentals from `moho_core::sim_math` (`sin_cos`, `atan2`),
  thin wrappers over `libm`. A function the sim needs and `sim_math` lacks is added there.
- No `target-cpu` or `+fma` build flags.
- rapier's and parry's `parallel` feature stays banned (`deny.toml`); it reorders float
  work.
- `sqrt`, `abs`, `to_radians`, `powi` and glam vector arithmetic are correctly
  specified or exact and are not restricted.

## 4 The sim-math gate

`just check` runs `scripts/check-sim-math.sh` over the files in `scripts/sim-math.txt`
(the `moho_physics` movement and world files, `searchlight/src/brain.rs` and `searchlight/src/lean.rs`). It prints the file and line of each
method call or `f32::`/`f64::` path of a platform transcendental and fails.

- The list is the boundary. Files outside it (camera, rendering, UI) are never scanned.
  A new sim file is added to the list when it is written.
- Test code is exempt: a `#[cfg(test)]` followed by an inline `mod name {` ends the scan
  of the file; any other `#[cfg(test)]` exempts only the next line.
- The check is textual; calls through a trait or an alias go unseen.
- Its self-tests (`just sim-math`) run fixtures under `scripts/fixtures/sim-math-*`
  and a per-package test that glam rotations use `libm`: a workspace-wide build lets
  rapier's `glamx` enable that feature and hide its absence.

## 5 Open gaps

- glam's SIMD paths: `Quat` products and `Vec4`/`Vec3A` dot and normalize can fuse or
  reorder float work per target, which blocks L2.
- `PhysicsWorld::brake_rolling` (`moho_physics/src/world/loose.rs`) calls `tan` and
  `cos`, and `loose.rs` is not on the list; it needs a `tan` wrapper in `sim_math` first.
- Game call sites in the flagstone and searchlight lines still use platform
  transcendentals; each line lists them under its README's "Determinism call sites"
  ([flagstone](../flagstone/README.md#determinism-call-sites),
  [searchlight](../searchlight/README.md#determinism-call-sites)).

## 6 Cross-OS report

`moho_physics/tests/replay.rs` hashes each tick's compared state (FNV-1a 64) and, when
`MOHO_REPLAY_HASH_OUT` is set, writes one `<tick> <hash>` line per tick. On the three-OS CI
plan the `Replay hashes` jobs upload those files and `Replay hash comparison`
(`scripts/compare-replay-hashes.sh`) reports, in the run summary, agreement, the first
disagreeing tick or a missing platform. Equal hashes on every OS mean L1 held for that run.
The report is measured, not gated: it never fails the run.
