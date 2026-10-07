# ENG-F2 — Dependency Upgrades


## Summary

Keep workspace dependencies current before drift makes an upgrade a rewrite.

## Items

| Item |
|---|
| [ENG-F2-01 bincode-migration](ENG-F2-01-bincode-migration.md) |
| [ENG-F2-02 egui-ui-architecture-migration](ENG-F2-02-egui-ui-architecture-migration.md) |
| [ENG-F2-03 platform-default-features-and-unused-deps](ENG-F2-03-platform-default-features-and-unused-deps.md) |
| [ENG-F2-04 paste-advisory-cleared](ENG-F2-04-paste-advisory-cleared.md) |

## Watchlist (blocked, no action until unblocked)

- `glam` 0.30→0.32 — blocked on `rapier3d` (pins glam 0.30 via `glamx`).
- `wgpu`/`naga` 29→30 — blocked on `egui-wgpu` (no wgpu-30 release yet).
- `ttf-parser` unmaintained (RUSTSEC-2026-0192) — removed by [ENG-F2-03](ENG-F2-03-platform-default-features-and-unused-deps.md) (winit default features off).
- `bincode` unmaintained (RUSTSEC-2025-0141); 3.0.0 is a `compile_error!` tombstone — replaced by [ENG-F2-01](ENG-F2-01-bincode-migration.md) ([ADR-0006](../../adr/0006-save-format-contract.md)).
- `paste` unmaintained (RUSTSEC-2024-0436) — via `rapier3d` → `simba`; tracked by [ENG-F2-04](ENG-F2-04-paste-advisory-cleared.md).
