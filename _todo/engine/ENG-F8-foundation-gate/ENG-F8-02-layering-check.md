# The gate rejects dependencies that cross game lines or reach platform crates

**Feature:** [ENG-F8](_feature.md)
**Issue:** #64

## Summary

[ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md) assigns every crate to one line and fixes the direction dependencies
may point. Today nothing checks it. Make `just check` enforce it, and remove
the one violation that exists after [ENG-F8-01](ENG-F8-01-moho-sim-merged-into-game.md): `moho_core`'s unused `winit`.

## Deliverables

- `moho_core` no longer declares `winit`.
- A layering check runs inside `just check` (locally, in CI and for agents).
  It reads the crate-to-line table from one data file.
- The check fails, naming the offending crate and edge, when:
  - a workspace member is missing from the table, or appears twice;
  - an engine crate reaches a game-line crate;
  - a game line reaches another game line;
  - a domain crate (`moho_core`, `moho_game`) reaches `winit`, `wgpu` or
    `egui`.
- A self-test shows the check fails on a known-bad table.
- [ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md) points to the data file as the machine-readable copy of its table.

## Acceptance criteria

- [ ] `just check` passes on the branch and runs the layering check.
- [ ] `cargo tree -p moho_core -p moho_game -e normal,build --all-features`
      shows no `winit`, `wgpu` or `egui`.
- [ ] Classifying `moho_ui` as engine in a fixture table makes the check
      exit non-zero, and the output names `moho_ui → moho_game`.
- [ ] Adding a workspace member without a table row makes the check exit
      non-zero.
- [ ] Each rule produces a readable message naming the rule it broke.

## Tech spec

**Design.**
- Data: `scripts/layering.txt`, one line per line: `<line>: <crate> <crate> …`
  (`engine`, `strategy`, `fps`, plus `domain:` for the platform-free set).
  There is no TOML parsing in bash.
- `scripts/check-layering.sh [table]`, following `check-comment-refs.sh`'s
  style:
  - Members come from
    `cargo tree --workspace --depth 0 -e normal --prefix none --format '{p}'`
    (verified to list all members).
  - For each crate, collect its reachable workspace and platform crates with
    `cargo tree -p <crate> -e normal,build --all-features --prefix none --format '{p}'`.
    That covers normal and build edges, including optional deps. Dev edges
    stay allowed ([ADR-0001](../../adr/0001-render-api-boundary.md)).
  - Exit 1 with all violations listed, not just the first.
- `justfile`: add a `layering` recipe that runs the script on the real table,
  then asserts it fails on `scripts/fixtures/layering-violation.txt`.
  `check` depends on `layering`.
- Why `cargo tree` over cargo-deny `wrappers` or a Rust test: it needs no new
  tool or crate, works on all three CI OSes under the justfile's bash shell,
  and it is the same command a human runs to debug a failure.

**Out of scope.**
- Moving `voxel/` or game event types out of `moho_core` ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md) prep).
- Splitting `moho_ui`.
- Any other unused dependency (Phase 1.2).

**Test map.**
| Criterion | Test |
|---|---|
| Real graph passes | `just layering` (inside `just check`) |
| Cross-line edge fails | self-test against `scripts/fixtures/layering-violation.txt` (`moho_ui` as engine) |
| Unassigned member fails | second fixture omitting `moho_types` |
| Platform reach fails | covered by the real-graph run once `winit` is removed; a third fixture listing `moho_ui` under `domain:` proves the rule fires |

**Gate class:** glue.

**Risks.**
- Adds about 11 `cargo tree` calls to `just check`. They are fast after the
  first resolve, but measure in CI and batch them if the step exceeds ~10 s.
- Windows CI runs the script through Git Bash, as the comment-refs check
  already does.
- Depends on [ENG-F8-01](ENG-F8-01-moho-sim-merged-into-game.md). Until `moho_sim` is gone, the engine rule fails on
  `moho_sim → moho_game`.

## Verification

No runtime behavior changes. `just layering` (inside `just check`) is the
check; full-platform CI on the branch ran it on Linux, macOS and Windows.

## Notes

Beyond the spec, all within scope:
- Two more table rules: `unknown:` (a row names a non-member, e.g. a typo
  in `domain:` that would silently skip the platform rule) and
  `repeated row:`. Each has a fixture.
- `cargo tree` runs with `--target all`; the host-only default missed
  target-gated deps (a `cfg(windows)` `winit` in `moho_core` passed on Linux).
- Ten script runs per `just layering` took 13 s, over the ~10 s budget.
  `MOHO_LAYERING_GRAPH` lets the recipe resolve the graph once (~2 s).
- Fixtures can't fake the crate graph, so `--target all`, build edges and a
  failing `cargo tree` were verified by hand, not by a self-test.
