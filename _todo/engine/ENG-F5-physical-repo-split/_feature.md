# ENG-F5 — Physical Repo Split

**Status:** parked — not urgent, do when it makes logical sense, not on a
timeline

## Summary

Split the monorepo into an engine repo and per-game repos, joined via a
superproject (git submodules) so both can be developed together without
losing independent version control. Don't force it: split when the FPS
project reaches the point of needing the engine independently, or the engine
boundary needs to stop moving to unblock parallel work.

## Gate — agreed 2026-08-21, still applies

Split only after the Strategy line reaches a stable "v1", not on a timeline:

1. `SG-F1`'s items [SG-F1-01](../../strategy-game/SG-F1-core-interaction-loop/SG-F1-01-mine-voxel.md) through [SG-F1-04](../../strategy-game/SG-F1-core-interaction-loop/SG-F1-04-pickup-feedback.md) all done (mining, hotbar, tool gating,
   pickup feedback) — not just the first three. The later items exercise the
   `moho_ui` decoupling, `InputDispatcher` layering, and event-bus boundaries
   the first item alone doesn't.
2. `SG-F2` bugs `mining-mesh-gaps` ([SG-F2-02](../../strategy-game/SG-F2-known-bugs/SG-F2-02-mining-mesh-gaps.md)) and `mining-not-persisted` ([SG-F2-03](../../strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md))
   fixed — the latter is data loss, not polish. `spawn-inside-terrain`
   ([SG-F2-01](../../strategy-game/SG-F2-known-bugs/SG-F2-01-spawn-inside-terrain.md)) and `lod1-mesh-holes` ([ENG-F4-04](../ENG-F4-terrain-rendering-debt/ENG-F4-04-lod1-mesh-holes.md)) are explicitly NOT required for this
   gate.
3. `ENG-F2`'s bincode 2→3 strategy decided (not necessarily executed) — a
   save-format promise is implicit in tagging v1.
4. Dependency and asset licenses re-audited against the intended commercial
   distribution (`just deny` plus a manual pass over non-crate assets).
5. Tag `v1.0`; full test/clippy green.

Explicitly excluded from the gate: [SG-F2-01](../../strategy-game/SG-F2-known-bugs/SG-F2-01-spawn-inside-terrain.md)/[ENG-F4-04](../ENG-F4-terrain-rendering-debt/ENG-F4-04-lod1-mesh-holes.md), [ENG-F1](../ENG-F1-engine-hygiene/_feature.md)/[ENG-F3](../ENG-F3-renderer-pipeline-cleanup/_feature.md) cleanup items, blocked
`glam`/`wgpu`-30 upgrades — opportunistic backlog, no pre-split sweep.

**Why gate at all if not urgent:** splitting early freezes an API proven
against too little real use. [SG-F1](../../strategy-game/SG-F1-core-interaction-loop/_feature.md)'s first item alone already found two real
cross-boundary bugs by being used for real — the rest of [SG-F1](../../strategy-game/SG-F1-core-interaction-loop/_feature.md) is more of that
same signal before the boundary gets harder to change.

## Target shape

```
moho-engine/        (repo) — moho_core, moho_renderer, moho_audio,
                      moho_physics, moho_render_api, moho_input, moho_types,
                      moho_sim
moho-strategy/       (repo) — current moho_game/, moho_ui/, src/ binary
moho-fps/            (repo) — new, once FPS-F1 produces real reqs
moho/                (superproject repo) — no source, just git submodules
                      pointing at the three above + a top-level workspace
                      config for aggregate builds
```

## Known prep work (not yet started)

- `moho_game::controller::{PlayerController, controller_to_camera}` is
  generic FPS-style camera/movement math misplaced under "game" — it was
  classified there under a voxel-game-only reading of the engine boundary.
  Move to `moho_core` before or during the split; needed by both games.
- `moho_core::voxel` (grid, marching-cubes meshing, chunk streaming, LOD,
  light propagation) is NOT generic — it's this game's terrain engine, not
  reusable "engine." Don't let it drag into `moho-engine` by default; decide
  deliberately whether it moves there (reusable-if-FPS-ever-wants-voxels) or
  stays with `moho-strategy`.
- `moho_render_api::register_indexed_mesh` already accepts arbitrary static
  meshes (confirmed 2026-08) — an FPS level-geometry importer needs no
  renderer API change, just an importer that feeds it real level data instead
  of voxel-baked lighting channels.

## Mechanics (well-understood, do when triggered)

1. `git filter-repo` engine crates into `moho-engine`, preserving history.
2. Path-dep during active cross-repo development; switch to pinned git dep
   once the engine API stabilizes.
3. Duplicate shared infra (`rustfmt.toml`, CI, workspace dep pins) across
   repos, kept in sync.
4. Add submodules to the superproject once repos exist.
