# ENG-F9 — Domain tests catch behaviour changes

**Note:** Early tier, post-gate; see [ENG-F8](https://github.com/WrackedFella/moho/issues/61)
**Issue:** [#234](https://github.com/WrackedFella/moho/issues/234)
**Status:** Backlog
**Labels:** feature, line:engine

## Summary

The diff-scoped `just mutants` gate only protects code that a change touches.
Existing domain code was written before that gate, and much of it is
unprotected. A 2026-10 sample over `moho_game`, `moho_sim` and `moho_types`
caught 223 of 739 viable mutants (30%; 15 more timed out). The
[SG-F1](../../strategy-game/SG-F1-core-interaction-loop/_feature.md)-era rules (`pawn`, `tools`, `inventory`) hold up, `raycast` is mixed
(65 caught, 94 survived), and older code doesn't hold up: `actors` (158 survivors, 0 caught), `controller` (82/0),
`scene_builders` (76/32), `biome` (22/5). This feature brings the
domain crates up to the standard before feature work leans on them, and
deletes tests that can't fail.

## Exit criteria

- A full `just mutants-full` baseline is recorded (the weekly workflow has
  run, and its summary is linked from this feature).
- In `moho_game` and `moho_core`'s rule modules, every surviving mutant is
  either killed by a test or excluded in the cargo-mutants config with a
  stated reason. Rendering-only or debug code may be excluded.
- No test is tautological or vacuous: none that runs the same pure function
  twice to "prove" determinism, none that asserts inside `if let`/`let else`
  and passes when the value is absent, and no `#[ignore]` placeholders without
  assertions.
- Tests for code with no production caller are deleted with that code.
- At least one `proptest` guards a core invariant (raycast/surface agreement,
  or inventory conservation). At least one `insta` snapshot guards the save
  envelope ([ADR-0006](../../adr/0006-save-format-contract.md)).
- New and touched tests use `scenario_expected_result` names. Existing
  `test_*` names are renamed when touched, not in a sweep.

## Scope

- In: domain crates' unit and integration tests; cargo-mutants exclusions;
  absorbing [ENG-F1-04](../ENG-F1-engine-hygiene/ENG-F1-04-keybind-test-layering.md) (keybind test layers) and [ENG-F1-07](../ENG-F1-engine-hygiene/ENG-F1-07-lint-ratchet.md)'s placeholder and
  vacuous-test deliverables.
- Out: renderer and UI coverage beyond what [ENG-F1-04](../ENG-F1-engine-hygiene/ENG-F1-04-keybind-test-layering.md) already covers;
  coverage percentage targets.

## Items

| Item |
|---|

## Notes

2026-10 findings:

- `moho_sim`'s determinism test runs the same pure code twice in one process,
  and the API it tests has no production caller (deleted with [ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md)'s
  merge).
- `moho_core/tests/lighting.rs` asserts inside `if let Some(..)`, so it passes
  vacuously when the chunk has no light data.
- `moho_renderer/tests/compile_api.rs` is a type-check guard, not a
  behaviour test. Keep it only if the signature is a deliberate contract.
- 279 tests use `test_`/non-scenario names, 65 assertions only check
  `is_ok`/`is_some`/non-empty, and there is no `proptest` or `insta` yet.
