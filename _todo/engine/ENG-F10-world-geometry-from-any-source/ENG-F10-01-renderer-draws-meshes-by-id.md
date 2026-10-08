# The renderer draws, replaces and removes a game's meshes by id

**Feature:** [ENG-F10](_feature.md)

## Summary

The renderer takes world geometry through the
[ADR-0010](../../adr/0010-world-geometry-is-a-mesh-contract.md) contract:
indexed meshes keyed by a game-chosen id, with add, replace and remove. The
strategy binary feeds its chunk meshes through it, and the renderer stops
reading `VoxelChunk` or writing a GPU handle into it. Replaced and evicted
chunks now free their GPU mesh; today they leak.

## Deliverables

- The world-geometry contract (mesh id, mesh, validation error) in `moho_render_api`.
- The renderer draws contract meshes. No `moho_renderer` source or test names
  `VoxelChunk`, `ChunkStore` or `MaterialType`, and `moho_renderer` has no
  `moho_game` dev-dependency.
- The strategy binary hands chunk meshes to the renderer on generation, remesh
  and load, and removes them on eviction.
- `VoxelChunk` holds no GPU handle.

## Acceptance criteria

```gherkin
Scenario: A non-voxel mesh is drawn
  Given a static triangle mesh registered through the world-geometry contract
  When a frame is prepared
  Then the mesh is in the draw list

Scenario: Replacing a mesh draws only the new one
  Given a mesh registered under an id
  When a different mesh is registered under the same id
  And a frame is prepared
  Then the draw list holds the new mesh once
  And the old mesh's GPU resources are freed

Scenario: Removing a mesh stops drawing it and frees it
  Given a mesh registered under an id
  When the id is removed
  And a frame is prepared
  Then the mesh is not in the draw list
  And its GPU resources are freed

Scenario: A mesh registered before the renderer exists is drawn on the first frame
  Given a mesh registered while no renderer backend exists
  When the first frame is prepared
  Then the mesh is in the draw list

Scenario: An empty mesh draws nothing
  Given a mesh registered under an id
  When a mesh with no triangles is registered under the same id
  And a frame is prepared
  Then nothing is drawn for that id
  And the previous mesh's GPU resources are freed

Scenario Outline: A malformed mesh is refused
  Given a mesh whose <fault>
  When it is built
  Then it is refused with an error naming the fault

  Examples:
    | fault                                    |
    | channels differ in length                |
    | indices point past the last vertex       |
    | index count is not a multiple of three   |
```

## Tech spec

**Design.**
- `moho_render_api::world_geometry` (new module):
  - `WorldMeshId(pub u64)`: `Copy`, `Eq`, `Hash`, `Ord`, `Debug`. The game picks
    it and keeps it stable (ADR-0011's stable ids). Renderer and physics key on the
    same value, so the game keeps no handle map.
  - `WorldMesh`: owned, one `Vec` per channel, matching the GPU vertex:
    `positions [[f32;3]]`, `normals [[f32;3]]`, `ao f32`, `light_rgb [[f32;3]]`,
    `sky_exposure f32`, `surface u32`, `indices u32`. Fields private;
    `WorldMesh::new(..) -> Result<Self, WorldMeshError>` checks equal channel
    lengths, `indices.len() % 3 == 0` and every index in range. Slice accessors and
    `is_empty()` (no triangles). Seven arguments is within clippy's default limit.
  - `WorldMeshError` enum, one variant per fault, `Display` + `std::error::Error`.
  - Owned rather than borrowed because the renderer queues meshes until a backend
    exists; physics copies the data today anyway.
- `moho_renderer::world_meshes::WorldMeshes` (new), owned by `Scene`:
  `upsert(id, WorldMesh, material_idx)` and `remove(id)` queue changes (last change
  per id wins); `flush(&mut dyn RendererBackend)` registers new meshes and
  unregisters replaced or removed handles; draws are one identity-transform
  instance per live mesh. Upserting an empty mesh is a remove.
- Upload mapping onto the unchanged GPU vertex: `surface` → `geometry_type`,
  `light_rgb` → `block_light_rgb`, `sky_exposure` → `sky_exposed`. The
  `light_level` slot gets the largest `light_rgb` component; only the debug light
  view (mode 4) reads it.
- `Scene::render` drops its `chunks` and `terrain_material_idx` parameters, gains
  `world_meshes_mut()`, and flushes before collecting draws. `BufferManager`'s
  chunk-keyed API (register, pending/swap, pool, unregister) and
  `InstanceCollector::collect_chunks` are deleted; delete `BufferManager` if
  nothing else is left in it.
- Strategy binary, new `src/app/world_geometry.rs`: `chunk_mesh_id(IVec3) ->
  WorldMeshId` (21 bits per signed axis, `debug_assert!` on range) and
  `chunk_world_mesh(&VoxelChunk) -> Result<WorldMesh, WorldMeshError>`. Call
  `upsert` wherever a chunk enters `ChunkStore` (generation, remesh, scene load)
  and `remove` on eviction. Direct calls, not bus events: same system.
- `moho_core`: `VoxelChunk` loses `set_mesh_handle`, `get_mesh_handle`,
  `is_uploaded` and the field behind them.
- Renderer tests use local `Renderable`/`RenderMaterial` stand-ins; drop the
  `moho_game` dev-dependency.

**Out of scope.**
- Physics colliders and the binary's chunk collider map ([ENG-F10-02](ENG-F10-02-physics-collides-with-meshes-by-id.md)).
- Moving voxel code, `MaterialType` or `WorldEvent` (ENG-F10-03, ENG-F10-04).
- Any shader or GPU vertex layout change, including renaming WGSL fields.
- Per-vertex material selection by `surface`; ENG-F1-05's material rethink.
- Streaming for non-voxel maps; LOD changes; ENG-F11's loop extraction.

**Test map.**
| Scenario | Test | Gate class |
|---|---|---|
| Non-voxel mesh drawn | `moho_renderer::world_meshes::tests::registered_mesh_is_drawn_after_flush`; scene level: `tests/scene_render_mock.rs::non_voxel_mesh_is_in_draw_list` | glue |
| Replace | `world_meshes::tests::replacing_mesh_draws_new_and_frees_old` | glue |
| Remove | `world_meshes::tests::removed_mesh_is_not_drawn_and_is_freed` | glue |
| Before renderer | `world_meshes::tests::mesh_queued_before_first_flush_is_drawn` | glue |
| Empty mesh | `world_meshes::tests::empty_mesh_draws_nothing_and_frees_previous` | glue |
| Malformed (3 rows) | `moho_render_api::world_geometry::tests::{mismatched_channel_is_refused, index_out_of_range_is_refused, partial_triangle_is_refused}` | glue |
| Edge: last change wins | `world_meshes::tests::last_queued_change_per_id_wins` | glue |
| Edge: no leak (proptest over upsert/remove sequences) | `world_meshes::tests::live_handles_match_live_ids` | glue |
| Edge: id packing | `src/app/world_geometry::tests::chunk_mesh_id_is_unique_within_range` (proptest) | glue |
| Wiring: eviction | `src/app/world_geometry::tests::evicted_chunk_is_not_drawn` (headless, ENG-F1-08 style) | glue |

**Gate class:** glue.

**Risks.**
- Each chunk mesh is copied once more into the queue; freed after upload.
- VRAM use changes (drops) because replaced meshes are now freed.
- `Scene::render`'s signature change touches the binary's window handlers.
- Collides with ENG-F11 (moves the binary's renderer setup) and
  [#101 ENG-F2-08, graphics stack](https://github.com/WrackedFella/moho/issues/101).
  Land this first; they rebase.

## Verification

- Terrain, AO and lighting look as they do on `dev`; mining updates the terrain;
  chunks stream in and out while walking.
- The renderer's active mesh count stays flat while walking back and forth over
  the same ground.
- Debug view 4 (raw light level) now shows the brightest baked-light channel
  (accepted change).
