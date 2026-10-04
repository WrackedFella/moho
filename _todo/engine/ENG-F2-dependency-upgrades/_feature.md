# ENG-F2 — Dependency Upgrades

**Status:** in progress

## Summary

Keep workspace dependencies current before drift makes an upgrade a rewrite.

## Items

| Item | Status |
|---|---|
| [ENG-F2-01 bincode-migration](ENG-F2-01-bincode-migration.md) | not started (decided: [ADR-0006](../../adr/0006-save-format-contract.md)) |
| [ENG-F2-02 egui-ui-architecture-migration](ENG-F2-02-egui-ui-architecture-migration.md) | not started |
| [ENG-F2-03 platform-default-features-and-unused-deps](ENG-F2-03-platform-default-features-and-unused-deps.md) | not started |

## Watchlist (blocked, no action until unblocked)

- `glam` 0.30→0.32 — blocked on `rapier3d` (pins glam 0.30 via `glamx`).
- `wgpu`/`naga` 29→30 — blocked on `egui-wgpu` (no wgpu-30 release yet).
- `ttf-parser` unmaintained (RUSTSEC-2026-0192) — removed by [ENG-F2-03](ENG-F2-03-platform-default-features-and-unused-deps.md) (winit default features off).
- `bincode` unmaintained (RUSTSEC-2025-0141); 3.0.0 is a `compile_error!` tombstone — replaced by [ENG-F2-01](ENG-F2-01-bincode-migration.md) ([ADR-0006](../../adr/0006-save-format-contract.md)).
- `legion` and its unmaintained deps — see [ENG-F7](../ENG-F7-maintained-ecs/_feature.md).
