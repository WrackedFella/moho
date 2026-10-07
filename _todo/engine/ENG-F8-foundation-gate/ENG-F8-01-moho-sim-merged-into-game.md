# The simulation controller lives in moho_game, and moho_sim is gone

**Feature:** [ENG-F8](_feature.md)
**Issue:** #63

## Summary

`moho_sim` is listed as engine but depends on `moho_game`. Most of its API has
no production caller. Fold what is used into `moho_game` and delete the rest,
so [ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md)'s crate table holds and agents stop treating `moho_sim` as "the
deterministic simulation".

## Deliverables

- `SimulationController` lives in `moho_game::simulation`, and the binary
  uses it from there.
- The `moho_sim` crate, its README and its workspace entry are deleted.
- Code and tests with no production caller are deleted: the toy
  `Simulation`, `PlayerInput`, `input_map` (`ContinuousState`, `TimedInput`,
  `map_to_player_inputs`, `stamp_inputs`), `apply_controller_tick`, and
  `SimulationController::snapshot_bytes`/`restore_from_bytes` with their
  snapshot struct.
- `CLAUDE.md` (architecture table, domain-logic paths), `_todo/_STANDARDS.md`'s
  gate-class domain list, the root README and `wiki/README.md` no longer
  mention `moho_sim`.

## Acceptance criteria

- [x] `cargo metadata --no-deps` lists no `moho_sim` package.
- [x] The game builds and runs. Movement, mouselook, camera-mode toggle and
      the day/night clock behave as before.
- [x] `just check` passes. `just mutants` reports no surviving mutant in the
      diff.
- [x] Nothing outside `_todo/` mentions `moho_sim`
      (`git grep -n moho_sim -- ':(exclude)_todo'` is empty).

## Tech spec

**Design.**
- Move `moho_sim/src/simulation.rs` to `moho_game/src/simulation.rs` with
  `git mv`. Change only the imports (`moho_game::` → `crate::`) and drop the
  snapshot code. Keeping the edit minimal lets `git diff` see a rename, so
  `just mutants --in-diff` only mutates the lines that actually changed.
- `moho_game` gains no dependency. `crc32fast` leaves the workspace if
  nothing else uses it.
- Binary: `moho_sim::SimulationController` →
  `moho_game::simulation::SimulationController` in `main.rs` and
  `app/initializer.rs`; remove the `moho_sim` dependency.
- Why delete and not keep the snapshot code: nothing in production
  snapshots the controller. [ENG-F2-01](../ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md) builds [ADR-0006](../../adr/0006-save-format-contract.md)'s shared envelope from
  scratch, and the old envelope stays in git history as a reference.

**Out of scope.**
- Renaming `SimulationController`.
- Fixed timestep or clock injection ([ENG-F6](../ENG-F6-headless-deterministic-logic/_feature.md)).
- Moving `PlayerController` to the engine ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md) prep).
- Touching `GameClock`.

**Test map.**
| Criterion | Test |
|---|---|
| Behaviour preserved | Existing `moho_game` controller and clock unit tests, unchanged |
| Toy API removed | `moho_sim/tests/{determinism,input_mapping,player_input_roundtrip}.rs` deleted with the code they test |
| Snapshot removed | `moho_sim/tests/snapshot_roundtrip.rs` deleted with the snapshot code |

No new tests: this moves code and preserves behaviour. If `just mutants`
flags a moved line, add a `moho_game::simulation::tests` case for it. Don't
exclude the mutant.

**Gate class:** glue.

**Risks.**
- Low blast radius: two binary files plus manifests.
- If git doesn't detect the rename (too many edited lines), every moved
  line is mutated. The 2026-10 baseline had 25 survivors in this file, so
  keep the edit small, or be ready to add tests.
- No save format is involved: the snapshot was never written to disk.

## Verification

`cargo run`, then check that movement, mouselook, the camera-mode toggle and
the day/night clock behave as before. Confirmed by the user on 2026-10-04.

## Notes

- Dropping the snapshot code leaves `simulation.rs` 41% similar to the
  original, below git's 50% rename threshold, so `just mutants --in-diff`
  mutates the whole file. `moho_game::simulation::tests` covers it (no
  survivors); nothing was excluded.
- The `':!_todo'` pathspec form fails on some git versions ("Unimplemented
  pathspec magic"); `':(exclude)_todo'` is equivalent.
