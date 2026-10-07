# Lint ratchet group is empty

**Feature:** [ENG-F1](_feature.md)

## Summary

`[workspace.lints.clippy]` in the root `Cargo.toml` allows pre-existing
violations under a "Ratchet" group. Mechanical lints are cleared separately;
this card covers the ones that need judgment.

## Deliverables

- Every lint left in the Ratchet group is fixed and its `allow` line deleted,
  or moved to the Won't-fix group with a reason the user agrees to.
- `missing_errors_doc` / `missing_panics_doc` enabled for library crates
  (public error contracts are documented).
- `too_many_lines` is enabled once [ENG-F1-03](ENG-F1-03-god-module-splits.md) lands.
- Rust lint `missing_debug_implementations` enabled (30 sites at the
  2026-10 baseline; the project requires `Debug` on public types).
- Placeholder `#[ignore]` tests with no assertions
  (`moho_ui/tests/console_rendering.rs`, `tests/console_toggle.rs`) are
  replaced with real tests or deleted.
- `prefs` video round-trip test exercises `Prefs::save`/load and fails when
  parsing fails (it currently asserts inside `if let Ok(..)`).
