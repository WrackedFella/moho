# 0010 — The engine sees world geometry as meshes; voxels belong to the strategy line

**Status:** Proposed (supersedes [0002](0002-voxel-and-materials-stay-in-core.md) and narrows [0003](0003-core-owns-event-types.md) once accepted)

## Context

The engine should not care where a game's world comes from. The strategy game
uses editable voxel terrain. The FPS will load non-voxel maps, either baked
files or generated at load time, and won't edit them at the voxel level. Each game is
expected to extend the shared framework in its own direction.

In 2026-10, voxels leak into the engine line in three places:

- `moho_core` holds the whole voxel terrain engine (`voxel/`: grid, meshing,
  LOD, light propagation, chunk store and streaming), `MaterialType`, and the
  block-level `WorldEvent`s. Nothing outside the strategy line uses any of it.
- `moho_renderer` uploads terrain by taking `&mut VoxelChunk` and writing the
  GPU mesh handle back into it ([ADR-0002](0002-voxel-and-materials-stay-in-core.md) chose this to avoid a trait).
- The terrain vertex carries voxel-baked channels (ambient occlusion, block
  light, sky exposure, geometry type) that the renderer reads by name from
  the chunk.

Physics already takes plain triangle meshes; the binary builds them from
chunks.

## Decision

- **World geometry is a mesh contract.** `moho_render_api` defines world
  geometry as a set of indexed meshes keyed by a handle, with add, replace
  and remove. The renderer and the physics colliders consume only this
  contract. The renderer keeps its own handle-to-GPU map and never writes into
  game data.
- **The world vertex keeps baked lighting, under generic names.** It carries
  position, normal, ambient occlusion, baked light colour, sky exposure and a
  surface id that the game's materials interpret. A voxel source computes
  these from light propagation. A baked map bakes them offline or fills in
  neutral values.
- **Editable terrain is "replace mesh N".** The compromise for voxels is that
  the contract supports replacing a mesh and its collider by handle. A static
  map never calls it. Nothing else voxel-specific enters the engine.
- **No world-source trait.** Each game's binary produces meshes from its own
  source and feeds the contract. The difference between games lives in that
  wiring and in data, not in an engine abstraction.
- **Voxels move to a strategy-line crate.** `voxel/`, `MaterialType`,
  `WorldEvent` and `BlockChangeReason` move from `moho_core` to a new
  `moho_voxel` crate in the strategy line, which is also in the domain
  (platform-free) set. [ADR-0005](0005-crate-lines-and-dependency-direction.md)'s
  layering check then keeps every engine crate from depending on it.
- **Event types are still defined once,** but in the crate that owns their
  line. `moho_core` keeps the bus and engine-wide events (audio, input,
  lifecycle). Line-specific events live with their line's crate. This narrows
  [ADR-0003](0003-core-owns-event-types.md)'s "`moho_core` owns all event types".

## Consequences

- [ADR-0002](0002-voxel-and-materials-stay-in-core.md) is superseded, and the placement question it left for [ENG-F5](../engine/ENG-F5-physical-repo-split/_feature.md)
  is settled: voxels go to the strategy repo at the split. If another game
  wants voxels, the crate is promoted to the engine line by a new ADR.
- [ENG-F1-05](../engine/ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md) (rethink the material model) no longer decides any crate's
  placement. It stays as hygiene inside the strategy line.
- The FPS can render and collide with any map that it can turn into meshes,
  with no engine change. Map formats, map streaming and map generation are
  FPS-line work.
- Renderer tests that build `VoxelChunk`s switch to contract fixtures.
- The engine's world vertex is a contract: changing its channels changes
  both games. A game that needs a different terrain shader adds a pipeline in
  its own line, or proposes a contract change.
- Costs: one more crate, and baked-lighting channels that a game may fill with
  neutral values.
- Delivered by [ENG-F10](../engine/ENG-F10-world-geometry-from-any-source/_feature.md).
