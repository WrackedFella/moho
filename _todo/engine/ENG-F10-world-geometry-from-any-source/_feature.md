# ENG-F10 — The engine renders and collides with world geometry from any source

**Status:** proposed (early tier; must land before the FPS prototype loads maps)

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
  table and [ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md)'s line table.
- Out:
  - Any FPS map format, loader or generator (FPS line).
  - Streaming for non-voxel maps.
  - The material-model rethink ([ENG-F1-05](../ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md)).
  - A shared app shell for the two binaries.
  - Moving the generic `PlayerController` math to the engine. That happens
    when an FPS crate needs it, and the layering check forces it.

## Items

| Item | Status |
|---|---|

## Notes

- Suggested cut: contract and renderer migration first (judgment). The crate
  extraction afterwards is mostly mechanical moves and import updates.
- The extraction moves many files. Use GitNexus `rename` and `impact` for
  symbol moves, not find-and-replace.
