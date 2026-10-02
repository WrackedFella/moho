# Lint ratchet group is empty

**Status:** not started
**Feature:** ENG-F1

## Summary

`[workspace.lints.clippy]` in the root `Cargo.toml` allows pre-existing
violations under a "Ratchet" group. Mechanical lints are cleared separately;
this card covers the ones that need judgment.

## Deliverables

- Every lint left in the Ratchet group is fixed and its `allow` line deleted,
  or moved to the Won't-fix group with a reason the user agrees to.
- `missing_errors_doc` / `missing_panics_doc` enabled for library crates
  (public error contracts are documented).
- `too_many_lines` is enabled once ENG-F1-03 lands.
