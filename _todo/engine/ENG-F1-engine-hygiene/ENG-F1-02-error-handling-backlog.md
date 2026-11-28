# Convert panic!/unwrap backlog to Result

**Feature:** [ENG-F1](_feature.md)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

Per project standard: no `panic!` in library crates, minimal `unwrap`.
Convert opportunistically when the surrounding function is already touched.

## Deliverables

- `moho_core/src/prefs/mod.rs` — 2 `panic!` (parse failures) → `Result`.
- `moho_renderer/src/render_ops/frame_ops.rs` — 1 `panic!`.
- `moho_core/src/voxel/modification.rs` — 9 `unwrap`.
- `moho_core/src/voxel/light_jobs.rs` — 7 `unwrap`.
- `moho_renderer/src/shadow.rs` — 4 `unwrap`.
- `clippy::unwrap_used` / `expect_used` enabled workspace-wide (145 sites at
  the 2026-10 baseline; tests may allow them).
