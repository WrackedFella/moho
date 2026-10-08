# ENG-F2 — The dependency tree is current, clean and carries only chosen crates

**Issue:** [#93](https://github.com/WrackedFella/moho/issues/93)
**Status:** unknown
**Labels:** feature, line:engine

## End state

The engine ships as a separate repo ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md))
whose dependencies are each chosen on purpose ([ENG-F13](https://github.com/WrackedFella/moho/issues/83)),
free of unowned advisories, and close enough to upstream that an upgrade is
routine work, not a rewrite.

## Summary

Carries out the [ENG-F13 audit](../ENG-F13-dependency-audit/audit.md)'s calls
and the upgrades it found overdue: unmaintained and unused crates leave,
small utilities become std or in-house code, logging becomes structured, and
the graphics and physics stacks reach current releases.
**Moves toward the end state by:** leaving the repo split nothing unexamined
to copy, and setting the upgrade rhythm later milestones follow.

## Exit criteria

- `deny.toml` has no advisory ignore, except one blocked upstream that is on
  the watchlist with the release that would clear it.
- `just deny` reports no yanked crates.
- No workspace manifest declares `bincode`, `noise`, `crossbeam-channel`,
  `pollster`, `once_cell`, `log`, `env_logger`, or any dependency its crate
  doesn't use.
- `wgpu`/`naga`/egui and `rapier3d`/`glam` are on their latest compatible
  releases as of the M1 close.
- `moho_ui` builds without `#![allow(deprecated)]`.

## Scope

- In: the ENG-F13 calls listed in the audit as owned here; the M1-close
  stack upgrades; the egui deprecation migration.
- Out: `phf` (ENG-F12 reworks key naming); planned additions (adopted by the
  features that need them); upgrade containment ([ENG-F20](../ENG-F20-graphics-upgrade-touches-one-crate/_feature.md)).

## Upgrade policy

- Fast-moving stacks (`wgpu`/`naga`/egui, `rapier3d`/`glam`) are brought to
  their latest compatible release once per milestone close, as one card per
  stack. Between closes, only security fixes and yanked-crate updates.
- An upgrade blocked by a dependency's pin goes on the watchlist with the
  release that would unblock it.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| When are fast-moving stacks upgraded? | Once per milestone close | Upgrading on demand let drift build into rewrites; every release would cost churn without a consumer asking |
| Do old saves survive the save-format change? | No, before v1.0 ([ADR-0006](../../adr/0006-save-format-contract.md)) | A migration shim is code to own for saves nobody ships |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Making the upgrade policy a standing milestone-gate line once this feature closes | The policy needs one milestone of use first | M1 close |
| Profiler bridge for `tracing` (`tracing-tracy`, `puffin`) | No consumer yet | A frame-time investigation needs it |

## Items

| Item |
|---|
| [ENG-F2-01 Saves use the ADR-0006 envelope and encoding](ENG-F2-01-bincode-migration.md) |
| [ENG-F2-02 UI screens use egui's current entry points](ENG-F2-02-egui-ui-architecture-migration.md) |
| [ENG-F2-03 Unused dependencies and default features are trimmed](https://github.com/WrackedFella/moho/issues/96) |
| [ENG-F2-04 The paste advisory ignore is removed](ENG-F2-04-paste-advisory-cleared.md) |
| [ENG-F2-05 Engine and game logs are structured tracing events](ENG-F2-05-structured-logging.md) |
| [ENG-F2-06 Event and job channels use the standard library](ENG-F2-06-std-channels.md) |
| [ENG-F2-07 Renderer device setup blocks on async without pollster](ENG-F2-07-renderer-blocks-without-pollster.md) |
| [ENG-F2-08 The graphics stack is on current wgpu and egui](ENG-F2-08-graphics-stack-current.md) |
| [ENG-F2-09 The physics stack is on current rapier3d](ENG-F2-09-physics-stack-current.md) |
| [ENG-F2-10 Terrain generation uses in-house Perlin noise](ENG-F2-10-terrain-noise-in-house.md) |

## Watchlist (blocked, no action until unblocked)

- `glam` 0.30→0.34 — blocked on `rapier3d` (pins glam 0.30 via `glamx`); re-check in [ENG-F2-09](ENG-F2-09-physics-stack-current.md).
- `wgpu`/`naga` 29→30 — recorded as blocked on `egui-wgpu`; `egui-wgpu` 0.36.2 has since shipped, so re-check in [ENG-F2-08](ENG-F2-08-graphics-stack-current.md).
- `ttf-parser` unmaintained (RUSTSEC-2026-0192) — removed by [ENG-F2-03](https://github.com/WrackedFella/moho/issues/96) (winit default features off).
- `bincode` unmaintained (RUSTSEC-2025-0141); 3.0.0 is a `compile_error!` tombstone — replaced by [ENG-F2-01](ENG-F2-01-bincode-migration.md) ([ADR-0006](../../adr/0006-save-format-contract.md)).
- `paste` unmaintained (RUSTSEC-2024-0436) — via `rapier3d` → `simba`; tracked by [ENG-F2-04](ENG-F2-04-paste-advisory-cleared.md).

## Notes

- [ENG-F2-02](ENG-F2-02-egui-ui-architecture-migration.md) lands before
  [ENG-F2-08](ENG-F2-08-graphics-stack-current.md): egui 0.36 removes the
  deprecated entry points, and 0.34.3 already has their replacements.
- [ENG-F2-09](ENG-F2-09-physics-stack-current.md) delivers
  [ENG-F2-04](ENG-F2-04-paste-advisory-cleared.md): `rapier3d` 0.36 drops `paste`.
- [ENG-F2-10](ENG-F2-10-terrain-noise-in-house.md) changes strategy-line code;
  it lands after [ENG-F10](../ENG-F10-world-geometry-from-any-source/_feature.md).
