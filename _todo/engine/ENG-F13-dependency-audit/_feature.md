# ENG-F13 — Every third-party dependency has a recorded, reasoned verdict

**Issue:** #83


## Summary

Library choices so far are unaudited, and the repo split would copy them into
three repos. Each direct dependency, and each planned addition, gets a verdict
before more are added. Analysis, not code: only non-keep verdicts become code
cards.

## Exit criteria

- A table under this feature lists every direct dependency with purpose and
  call sites, maintenance, licence against
  [ADR-0007](../../adr/0007-third-party-licence-policy.md), advisories,
  transitive weight and compile cost, alternatives, and a verdict with a
  one-line reason.
- Verdicts are keep / replace / fork / homebrew / drop. A homebrew verdict
  names the tests that prove it; a fork verdict records licence, inherited
  unsafe code and how much of the crate is used.
- Planned additions (gamepad, glTF, image, `tracing`, an audio alternative)
  have verdicts too. Scripting runtimes and netcode candidates are listed
  with notes only, no verdict.
- Each replace/fork/homebrew/drop verdict is a card under this feature.
- `just deny` passes.

## Scope

- In: all direct dependencies; planned additions.
- Out: performing the replacements (the cards do that).

## Notes

- Runs alongside ENG-F10; must finish before ENG-F12 or any asset-import
  feature adds a dependency.
- Known small candidates: `once_cell`, `crossbeam-channel`, `ini`/`phf`
  key tables, `noise`'s duplicate `rand`.

## Items

| Item |
|---|
