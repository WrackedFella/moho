# 0002 — `voxel/` and `MaterialType` stay in `moho_core`

**Status:** Superseded by [0010](0010-world-geometry-is-a-mesh-contract.md)

## Context

The engine/game split moved game-domain modules into `moho_game`. Two
candidates resisted: `MaterialType` is stored directly by `VoxelGrid`'s
`MaterialRegistry`, and `VoxelChunk` carries GPU-upload bookkeeping
(`is_uploaded`, mesh handle, double-buffering) that the renderer's
`BufferManager`/`InstanceCollector` mutate in place through ECS queries.

## Decision

- The `voxel/` subtree (grid, meshing, LOD, lighting, chunk jobs, streaming)
  and `MaterialType` stay in `moho_core`. World-generation *rules* (biomes,
  ore placement) live in `moho_game`.
- No mutable-state trait boundary is introduced for `VoxelChunk`; the
  renderer references it concretely.

## Consequences

- `voxel/` is this game's terrain engine, not generic engine code. At the
  repo split ([ENG-F5](https://github.com/WrackedFella/moho/issues/232)) decide deliberately whether it goes to the engine or
  the strategy repo; it must not move by default.
- `MaterialType`'s placement is forced by concrete ownership in
  `VoxelGrid`; [ENG-F1-05](../engine/ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md) removes that constraint.
