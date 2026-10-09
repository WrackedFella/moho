# ENG-F5 — Engine and games live in separate repos

**Note:** Phase 3; gate agreed 2026-08-21, amended 2026-10-05 and 2026-10-07
**Issue:** [#232](https://github.com/WrackedFella/moho/issues/232)
**Status:** Backlog
**Labels:** feature, line:engine

## Summary

Split the monorepo into an engine repo and one repo per game, joined by a
superproject (git submodules) so all three can still be developed together.
The split gives each line its own versions and release cadence. It doesn't
stop either game from diverging: each game already extends the engine in its
own crates, and the layering check ([ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md)) keeps the lines apart.
Splitting before a second consumer has used the engine would freeze an API
proven by one game, and every engine change would then cost two PRs and a
version bump. So the gate asks for evidence that the boundary has settled, not
a date.

## Exit criteria (the gate)

1. **Strategy v1 slice.** [SG-F1-04](../../strategy-game/SG-F1-core-interaction-loop/SG-F1-04-pickup-feedback.md) (pickup feedback) is done, and
   [SG-F2-03](../../strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md) (mining not persisted) and [SG-F2-02](../../strategy-game/SG-F2-known-bugs/SG-F2-02-mining-mesh-gaps.md) (mesh gaps after mining)
   are fixed.
2. **Save format decided.** Met: [ADR-0006](../../adr/0006-save-format-contract.md).
3. **Licences.** `just deny` passes, non-crate assets have been audited, and
   builds ship the third-party notices file ([ADR-0007](../../adr/0007-third-party-licence-policy.md)).
4. **Engine is agnostic about world geometry.** [ENG-F10](../ENG-F10-world-geometry-from-any-source/_feature.md) is done: no engine
   crate names a voxel type.
5. **A second consumer exists.** The FPS prototype loads a non-voxel map
   through [ENG-F10](../ENG-F10-world-geometry-from-any-source/_feature.md)'s contract, using only engine-line crates. The layering
   check enforces that the FPS line can't reach strategy crates, so anything
   generic it needs has moved to the engine by then.
6. **Boundary has settled.** The seam list named in [ADR-0012](../../adr/0012-engine-crate-map-for-m2.md) is unchanged by
   the last two FPS features.
7. `v1.0` is tagged on a green `just check`, with all-OS CI passing.

Not required: [SG-F2-01](../../strategy-game/SG-F2-known-bugs/SG-F2-01-spawn-inside-terrain.md) (spawn inside terrain), [ENG-F4-04](../ENG-F4-terrain-rendering-debt/ENG-F4-04-lod1-mesh-holes.md) (LOD1 mesh holes),
[ENG-F1](../ENG-F1-engine-hygiene/_feature.md) and [ENG-F3](../ENG-F3-renderer-pipeline-cleanup/_feature.md) cleanup, and the blocked `glam`/`wgpu` upgrades.

## Scope

- In: the three repos and the superproject, with history preserved; CI and
  shared config in each repo.
- Out: engine API changes. Any that are needed land before the split, inside
  the monorepo.

## Items

| Item |
|---|

## Notes

Target shape: the engine repo holds the engine line of `scripts/layering.txt`
at split time. The strategy and FPS repos hold their lines. The superproject
holds no source, only the submodules and a top-level workspace for aggregate
builds. The table is the `filter-repo` path list.

Mechanics, all well understood:

1. Use `git filter-repo` to move each line's crates into its repo, keeping
   history.
2. Use path dependencies while development spans repos. Switch to pinned git
   dependencies once the engine API is stable.
3. Copy shared infrastructure (`rustfmt.toml`, CI, workspace dependency pins,
   `deny.toml`, the layering check) into each repo and keep the copies in sync.
4. Add the submodules to the superproject.
