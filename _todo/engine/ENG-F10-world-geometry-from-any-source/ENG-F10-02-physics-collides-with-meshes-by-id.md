# Physics collides with a game's meshes by id, and one change updates drawing and collision

**Feature:** [ENG-F10](_feature.md)

## Summary

Physics builds static colliders from the same contract meshes and ids as the
renderer ([ENG-F10-01](ENG-F10-01-renderer-draws-meshes-by-id.md)), so the
engine, not each game binary, keeps the id-to-collider map. The strategy binary
sends each mesh change to both through one call, and its chunk-keyed collider
code goes.

## Deliverables

- `moho_physics` adds, replaces and removes static colliders by `WorldMeshId`
  from a `WorldMesh`.
- The strategy binary updates renderer and physics for a mesh change through
  one call; `PhysicsController` has no chunk-keyed collider code.
- An empty mesh holds no collider (today a dummy ball collider is inserted).

## Acceptance criteria

```gherkin
Scenario: A body rests on a non-voxel mesh
  Given a static floor mesh registered through the world-geometry contract
  When a body falls onto it for one second of ticks
  Then the body stops on the floor

Scenario: Replacing a mesh replaces its collider
  Given a floor mesh registered under an id
  When a floor two units higher is registered under the same id
  And a body falls onto it for one second of ticks
  Then the body stops on the higher floor
  And the world holds one collider for that id

Scenario: Removing a mesh frees its collider
  Given a floor mesh registered under an id
  When the id is removed
  And a body falls for one second of ticks
  Then the body falls past where the floor was
  And the world holds no collider for that id

Scenario: An empty mesh holds no collider
  Given a floor mesh registered under an id
  When a mesh with no triangles is registered under the same id
  Then the world holds no collider for that id

Scenario: One change updates drawing and collision together
  Given the strategy game running headless with a recording renderer
  When a chunk is remeshed
  Then the renderer draws the new mesh and physics holds the new collider for it

Scenario: Evicting a chunk frees both its mesh and its collider
  Given the strategy game running headless with a recording renderer
  When a chunk is evicted by streaming
  Then the renderer no longer draws it and physics holds no collider for it
```

## Tech spec

**Design.**
- `moho_physics` depends on `moho_render_api` for the contract types (engine to
  engine).
- `PhysicsWorld` gains `set_world_mesh(WorldMeshId, &WorldMesh)`,
  `remove_world_mesh(WorldMeshId)` and `world_mesh_collider(WorldMeshId) ->
  Option<ColliderHandle>`, backed by a private `HashMap<WorldMeshId,
  ColliderHandle>`. Setting an id removes its old collider first. An empty mesh,
  or a trimesh rapier rejects, leaves no collider for the id (warn on rejection).
  `add_terrain_trimesh` and its dummy-ball fallback go. Why here and not in the
  binary: the FPS gets id-keyed colliders without copying the strategy binary's
  map (the placement test).
- Strategy binary, `src/app/world_geometry.rs` (from ENG-F10-01): `set_mesh(app,
  id, WorldMesh, material_idx)` and `remove_mesh(app, id)` call the renderer queue
  and then physics. Every chunk call site from ENG-F10-01 goes through them.
- `PhysicsController` loses `chunk_colliders`, `update_chunk_collider`,
  `remove_chunk_collider` and `sync_colliders`. After `reset()` (scene load), the
  load path re-sends every stored chunk through `set_mesh`.

**Out of scope.**
- Dropping `moho_physics`'s unused `moho_core` dependency
  ([#96 ENG-F2-03](https://github.com/WrackedFella/moho/issues/96) owns it).
- Physics queries and dynamic bodies (ENG-F15); the character controller (ENG-F21).
- Collider friction or material per surface; it stays 0.6 for every world mesh.
- Moving voxel code (ENG-F10-03, ENG-F10-04).

**Test map.**
| Scenario | Test | Gate class |
|---|---|---|
| Body rests | `moho_physics::world::tests::body_rests_on_registered_world_mesh` | glue |
| Replace | `world::tests::replacing_world_mesh_moves_the_floor` | glue |
| Remove | `world::tests::removed_world_mesh_lets_body_fall` | glue |
| Empty mesh | `world::tests::empty_world_mesh_holds_no_collider` | glue |
| Remesh updates both | `src/app/world_geometry::tests::remesh_updates_draw_and_collider_together` | glue |
| Eviction frees both | `src/app/world_geometry::tests::eviction_frees_draw_and_collider` | glue |
| Edge: collider count matches live ids (proptest over set/remove sequences) | `world::tests::colliders_match_live_world_mesh_ids` | glue |
| Edge: load resyncs colliders | `src/app/physics_controller::tests::reset_then_load_holds_one_collider_per_chunk` | glue |

**Gate class:** glue.

**Risks.**
- Depends on ENG-F10-01 for the contract types.
- [#102 ENG-F2-09, rapier upgrade](https://github.com/WrackedFella/moho/issues/102)
  touches `PhysicsWorld`'s trimesh build; whichever lands second rebases.

## Verification

- Walk, mine a block under the player, and fall into the hole.
- Walk to a streaming edge and back; the ground holds wherever it is drawn.
- Load a save; the player stands on the terrain.
