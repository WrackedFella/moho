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
- `ttf-parser` unmaintained (RUSTSEC-2026-0192, ignored in `deny.toml`) —
  arrives via winit → ab_glyph; clears when upstream replaces it.
- `bincode` unmaintained (RUSTSEC-2025-0141) — resolved by ENG-F2-01.
- `legion` and its unmaintained deps — see ENG-F7.
