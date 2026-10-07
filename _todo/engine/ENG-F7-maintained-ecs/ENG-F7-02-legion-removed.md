# The game runs without legion, and the renderer takes plain data

**Feature:** [ENG-F7](_feature.md)
**Issue:** #66

## Summary

Swap `legion::World` for [ENG-F7-01](ENG-F7-01-typed-entity-stores.md)'s stores everywhere and remove `legion`
from the workspace. The renderer's collection API takes slices and an
iterator of chunks instead of a world, which closes [ENG-F7](_feature.md) and [ENG-F8](../ENG-F8-foundation-gate/_feature.md)'s G1.

## Deliverables

- `moho_game::scene::SceneEntities { actors: ActorStore, chunks: ChunkStore }`
  replaces `World` in the binary, `scene_builders` and `scene_persistence`.
- `moho_renderer::{Scene::render, ScenePreparation::prepare,
  InstanceCollector::collect}` take `&[S]`, `&[C]` and an iterator of
  `&mut VoxelChunk`. `S` and `C` are bound by `Renderable` only.
- The physics controller tracks `ActorId`s and syncs colliders from the
  `ChunkStore`.
- `legion` is removed from every manifest and from `[workspace.dependencies]`.
- `CLAUDE.md`'s architecture notes say entities live in typed stores
  ([ADR-0004](../../adr/0004-entity-storage-without-a-general-ecs.md)), not `legion`.
- `deny.toml` loses the `instant` ignore. Each remaining ignore's
  reason names its owning card (bincode → [ENG-F2-01](../ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md), ttf-parser → [ENG-F2-03](../ENG-F2-dependency-upgrades/ENG-F2-03-platform-default-features-and-unused-deps.md), paste → [ENG-F2-04](../ENG-F2-dependency-upgrades/ENG-F2-04-paste-advisory-cleared.md)).

## Acceptance criteria

- [ ] `cargo tree --workspace -i legion` reports no match.
- [ ] `just check` and `just deny` pass.
- [ ] No public item in an engine crate names a world or ECS type.
- [ ] `cargo bench --bench event_bus_bench` shows no regression against
      `dev`.
- [ ] In-game, behaviour is unchanged: terrain streams in and out, mining
      remeshes the right chunk, the `spawn sphere`/`spawn cube` console
      commands drop physics bodies that fall, light gizmos appear, and
      save → quit → load restores actors and terrain.

## Tech spec

**Design.**
- Binary: `App.world` → `App.entities: SceneEntities`.
  - The chunk-remesh path becomes `entities.chunks.insert(chunk)`.
  - `remove_chunk_entity` becomes `chunks.remove(pos)`.
  - `debug_assert_chunk_entity_unique` is deleted, because the store
    guarantees uniqueness.
  - `light_gizmos` is deleted: it is written but never read. Gizmos stay
    plain spheres.
- `PhysicsController`:
  - `test_bodies: Vec<(RigidBodyHandle, ActorId)>`;
    `step -> Vec<(ActorId, Vec3)>`.
  - The caller applies each update via `center_mut`. A `None` there is
    logged at `warn` with the handle, never ignored silently.
  - `sync_colliders_from_ecs(&World)` → `sync_colliders(&ChunkStore)`.
- `scene_persistence` iterates the stores. The file layout is unchanged;
  only the chunk order becomes deterministic.
- Renderer: drop the `legion::storage::Component` bounds and the legion
  imports. Draw output is identical: chunks were previously drawn in
  arbitrary order, now in ascending position order.

**Out of scope.**
- Adopting an ECS.
- Fixing chunk GPU-buffer lifetime on replace.
- Save-format changes ([ENG-F2-01](../ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md)).
- Renaming `Sphere`/`Cube`.

**Test map.**
| Criterion | Test |
|---|---|
| Renderer collects from plain data | port `moho_renderer::instance_collector::tests::*` and `tests/scene_render_mock.rs` to slices and stores |
| Persistence round trip | port `moho_game/tests/save_load.rs`; add `moho_game::scene_persistence::tests::round_trip_preserves_spheres_cubes_and_chunks` |
| Stale body handle is surfaced | `moho::app::physics_controller::tests::step_reports_bodies_by_actor_id` |
| No legion | `cargo tree` criterion above, run in the PR's verification |
| Behaviour unchanged | Verification steps (manual) |

**Gate class:** glue (adapters and wiring; the store rules were reviewed in
[ENG-F7-01](ENG-F7-01-typed-entity-stores.md)).

**Risks.**
- Widest change in the gate: about 11 source files across the binary,
  `moho_game` and `moho_renderer`. Run GitNexus `impact` on
  `InstanceCollector::collect_from_world`, `Scene::render`,
  `ScenePreparation::prepare`, `scene_persistence::{encode_to_bytes,
  load_from_bytes}` and `scene_builders::voxel_terrain_scene*` before
  editing.
- Borrowing: actors (shared) and chunks (mutable) come from disjoint fields
  of `SceneEntities`, so `render` can take both at once.
- Old saves still load: the scene layout is unchanged.

**Deviations.**
- `InstanceCollector::collect` is split into `collect_actors` and
  `collect_chunks`, so no `too_many_arguments` allow is added; `prepare`
  clears the collector each frame.
- `scene_builders::voxel_terrain_scene*` take `&mut ChunkStore`: they only
  insert chunks.
- The `paste` ignore stays: `rapier3d` → `simba` also pulls it in. It is owned
  by [ENG-F2-04](../ENG-F2-dependency-upgrades/ENG-F2-04-paste-advisory-cleared.md).
- `SceneEntities::clear` replaces paired store clears, and `load_scene` resets
  physics with it, so a failed load can't leave bodies tracking dead actors.
  `PhysicsController::forget_body` drops a body whose actor is gone after one
  warn.
- Loading a save with two chunks at one position keeps the last and logs a
  warn.

## Verification

1. `cargo run`, create a world, and walk until chunks stream in and out.
2. Mine near a chunk boundary: the mined chunk remeshes.
3. Console: `spawn sphere` and `spawn cube`. Both fall and settle.
4. Save, quit, relaunch and load. Actors and terrain are restored, and the
   log shows no stale-actor-handle warning.
