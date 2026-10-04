# Chunks and actors have typed stores with guaranteed lookup rules

**Status:** ready
**Feature:** [ENG-F7](_feature.md)
**Issue:** #65

## Summary

[ADR-0004](../../adr/0004-entity-storage-without-a-general-ecs.md) replaces `legion` with typed stores. This card adds the two stores
and their invariants, test-first, without wiring them in. [ENG-F7-02](ENG-F7-02-legion-removed.md) swaps
them in.

## Deliverables

- `moho_core::voxel::ChunkStore`: chunk meshes keyed by chunk position.
- `moho_game::actors::ActorStore`: spheres and cubes, addressed by `ActorId`
  handles that never resolve to the wrong actor.

## Acceptance criteria

```gherkin
Scenario: A remeshed chunk replaces the previous one
  Given the store holds chunk A at (1, 0, 2)
  When chunk B for (1, 0, 2) is inserted
  Then the store holds exactly one chunk at (1, 0, 2), which is B
  And A is handed back to the caller

Scenario: An evicted chunk leaves the store
  Given the store holds a chunk at (0, 0, 0)
  When the chunk at (0, 0, 0) is removed
  Then the removed chunk is returned
  And no chunk is found at (0, 0, 0)

Scenario: Chunk iteration order doesn't depend on insertion order
  Given chunks inserted at any positions, in any order
  When the store is iterated
  Then positions come out unique, in ascending (x, y, z) order

Scenario: An actor moves by its handle
  Given a sphere and a cube are spawned
  When the sphere's centre is set through its handle
  Then only the sphere's centre changes

Scenario: A handle from before a clear resolves to nothing
  Given a sphere was spawned and its handle kept
  When the store is cleared and another sphere is spawned
  Then the old handle resolves to no actor
```

## Tech spec

**Design.**
- `ChunkStore` (moho_core, beside `VoxelChunk`):
  - `insert(VoxelChunk) -> Option<VoxelChunk>` keys on the chunk's own
    position.
  - `remove(IVec3)`, `get`/`get_mut(IVec3)`, `iter`, `iter_mut`, `len`,
    `is_empty`, `clear`.
  - Backed by an ordered map keyed by `(x, y, z)`. glam's `IVec3` has no
    `Ord` (verified), so the key is a tuple or `[i32; 3]`, and the ordered
    iteration is a contract (deterministic saves and draw order).
- `ActorStore` (moho_game, beside `Sphere`/`Cube`):
  - `spawn_sphere`/`spawn_cube -> ActorId`, `spheres() -> &[Sphere]`,
    `cubes() -> &[Cube]`, `center_mut(ActorId) -> Option<&mut Vec3>`,
    `clear`, `len`.
  - `ActorId` carries kind, index and the store's generation, which `clear`
    bumps. Individual actors are never despawned today, so per-slot
    generations aren't needed. A despawn feature brings them in.
- Why not `slotmap` or `hecs`: one handle-holding caller (physics test
  bodies) doesn't justify a dependency ([ADR-0004](../../adr/0004-entity-storage-without-a-general-ecs.md)).

**Out of scope.**
- Wiring the stores into the binary, renderer or persistence ([ENG-F7-02](ENG-F7-02-legion-removed.md)).
- Despawning a single actor.
- Light gizmos.

**Test map.**
| Scenario | Test |
|---|---|
| Remeshed chunk replaces | `moho_core::voxel::chunk_store::tests::insert_at_occupied_position_replaces_and_returns_previous` |
| — | `…::insert_at_new_position_returns_none` |
| Evicted chunk leaves | `…::remove_returns_chunk_and_position_becomes_empty`, `…::remove_missing_position_returns_none` |
| Iteration order | `…::iteration_is_unique_and_ascending_for_any_insert_order` (`proptest`, first in the workspace) |
| — | `…::iter_mut_changes_are_visible_through_get` |
| Actor moves by handle | `moho_game::actors::tests::center_mut_moves_only_the_identified_actor` |
| — | `…::sphere_and_cube_handles_with_equal_index_do_not_alias` |
| — | `…::spawned_actors_iterate_in_spawn_order` |
| Handle after clear | `…::handle_from_before_clear_resolves_to_none` |

`proptest` is added as a workspace dev-dependency (licence: MIT OR
Apache-2.0).

**Gate class:** domain. The failing tests pause for review before
implementation.

**Risks.** None at runtime: nothing uses the stores until [ENG-F7-02](ENG-F7-02-legion-removed.md).
