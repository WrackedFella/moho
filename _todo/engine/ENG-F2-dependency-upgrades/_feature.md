# ENG-F2 — Dependency Upgrades

**Status:** in progress

## Summary

Keep workspace dependencies current before drift makes an upgrade a rewrite.

## Items

| Item | Status |
|---|---|
| [ENG-F2-01 bincode-migration](ENG-F2-01-bincode-migration.md) | not started (needs a decision) |
| [ENG-F2-02 egui-ui-architecture-migration](ENG-F2-02-egui-ui-architecture-migration.md) | not started |

## Watchlist (blocked, no action until unblocked)

- `glam` 0.30→0.32 — blocked on `rapier3d` (pins glam 0.30 via `glamx`).
- `wgpu`/`naga` 29→30 — blocked on `egui-wgpu` (no wgpu-30 release yet).
