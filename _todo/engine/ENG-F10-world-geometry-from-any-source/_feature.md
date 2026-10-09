# ENG-F10 — The engine renders and collides with world geometry from any source

**Issue:** [#80](https://github.com/WrackedFella/moho/issues/80)
**Status:** Backlog
**Labels:** feature, line:engine


## Summary

A game hands the engine meshes, whatever their source: editable voxel
terrain, a baked map file, or a generated non-voxel map. The engine draws them
and builds colliders for them, and it knows nothing about voxels. The voxel
terrain engine moves into the strategy line. This is the boundary the FPS
prototype needs, and the clearest evidence for when the engine is ready
for the repo split ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md)). Decision: [ADR-0010](../../adr/0010-world-geometry-is-a-mesh-contract.md).

## Exit criteria

- [ADR-0010](../../adr/0010-world-geometry-is-a-mesh-contract.md) is accepted, and [ADR-0002](../../adr/0002-voxel-and-materials-stay-in-core.md) is marked superseded.
- `moho_voxel` is listed in the strategy and domain rows of
  `scripts/layering.txt`. It holds the voxel grid, meshing, LOD, lighting,
  chunk store and streaming, `MaterialType`, and the block-level world events.
  `just check` passes.
- No engine-line crate depends on `moho_voxel`, including as a
  dev-dependency, and no engine crate's source names a voxel type.
- The renderer and physics accept a non-voxel mesh through the contract. Two
  tests prove it:
  ```gherkin
  Scenario: A non-voxel mesh is drawn
    Given a static triangle mesh registered through the world-geometry contract
    When a frame is prepared
    Then the mesh is in the draw list

  Scenario: A body rests on a non-voxel mesh
    Given a static floor mesh registered through the world-geometry contract
    When a body falls onto it for one second of ticks
    Then the body stops on the floor
  ```
- Replacing a mesh by handle swaps both the drawn mesh and its collider, and
  removing it frees both (tests).
- Strategy behaviour is unchanged. Existing tests pass, and a manual check
  shows the same results: mining updates the terrain and collision, chunks
  stream in, lighting looks the same, and a save loads.

## Scope

- In: the world-geometry contract in `moho_render_api`; the renderer and the
  binary's physics wiring moved onto it; extracting `moho_voxel`; moving
  `MaterialType` and the voxel events; updating `CLAUDE.md`'s architecture
  table.
- Out:
  - Any FPS map format, loader or generator (FPS line).
  - Streaming for non-voxel maps.
  - The material-model rethink ([ENG-F1-05](../ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md)).
  - A shared app shell for the two binaries.
  - Editing [ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md)'s line table.
    ADR-0010 records the move; ADRs are superseded, not edited.
  - Moving the generic `PlayerController` math to the engine. That happens
    when an FPS crate needs it, and the layering check forces it.

## Items

| Item |
|---|
| #138 [ENG-F10-01](https://github.com/WrackedFella/moho/issues/138) The renderer draws, replaces and removes a game's meshes by id |
| #139 [ENG-F10-02](https://github.com/WrackedFella/moho/issues/139) Physics collides with a game's meshes by id, and one change updates drawing and collision |
| #140 [ENG-F10-03](ENG-F10-03-voxel-terrain-is-a-strategy-crate.md) Voxel terrain builds as a strategy-line crate |
| #141 [ENG-F10-04](ENG-F10-04-no-engine-crate-names-a-voxel-type.md) No engine crate names a voxel type, and the layering check covers tests |

## Notes

- Suggested cut: contract and renderer migration first (judgment). The crate
  extraction afterwards is mostly mechanical moves and import updates.
- ENG-F10-01 and ENG-F10-02 PR into `dev`. ENG-F10-03 and ENG-F10-04 are the
  cross-line move: integration branch `feature/ENG-F10-voxel-extraction`, started
  after ENG-F10-01 merges and after the ENG-F22 cards that edit the voxel code land
  ([ENG-F22-02](https://github.com/WrackedFella/moho/issues/169), then [ENG-F22-06](https://github.com/WrackedFella/moho/issues/173) and
  [ENG-F22-07](https://github.com/WrackedFella/moho/issues/174)), and announced to the Strategy lane first.
- The move does not wait on Strategy cards (2026-10-08). SG-F1-04 and SG-F2-03/02
  are unfiled; whichever starts after the move targets `moho_voxel`. The conflict
  risk is open `voxel/` branches, so the start check is: no open PR edits
  `moho_core/src/voxel/`.
- The extraction moves many files. Use GitNexus `rename` and `impact` for
  symbol moves, not find-and-replace.
