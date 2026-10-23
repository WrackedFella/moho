# A game places a registered mesh at transforms it sets each frame

**Feature:** ENG-F38
**Status:** Draft
**Gate class:** glue
**Labels:** line:engine

## Summary
Today a world mesh draws only at the identity transform, so a moving thing is rebuilt and
re-uploaded every frame (searchlight: seven boxes per enemy per frame). After this card a
game uploads a mesh once and places it, by id, at any number of transforms and materials
each frame. Placed copies go through the same draw list as static world meshes, so they
cast and receive shadows and take part in SSAO. Carries ADR-0023, amending seam 2.

## Deliverables
- `WorldMeshes::upsert_movable(id, mesh)`: upload a mesh that draws only where it is placed.
- `WorldMeshes::place(id, transform, material)`: draw one copy this frame; many per id.
- Placements last one frame: `Scene::render` consumes them whether or not the frame draws.
- All placements of one mesh go to the backend as one instanced draw.
- ADR-0023 "World meshes are placed per frame" (amends ADR-0010's contract).
- Wiki: rendering page, world-geometry section.

## Acceptance criteria
```gherkin
Scenario: Moving a placed mesh uploads no vertices
  Given a movable mesh was upserted and flushed
  When the game places it at a new transform on each of three frames
  Then the backend registers the mesh once and never again
  And each frame draws one instance at that frame's transform

Scenario: One mesh draws at several transforms in one frame
  Given a movable mesh was upserted
  When the game places it at three transforms in one frame
  Then the backend receives one draw for that mesh with three instances, in placement order

Scenario: Each placement carries its own material
  Given a movable mesh without a materials channel
  When the game places it twice with materials 3 and 5
  Then the two instances carry materials 3 and 5
  And the uploaded vertices defer to the instance material

Scenario: A mesh not placed this frame is not drawn
  Given a movable mesh was placed and drawn on one frame
  When the next frame renders without placing it
  Then nothing is drawn for it and it stays uploaded

Scenario: Placements last one frame even when the frame is skipped
  Given a movable mesh is placed
  When the frame's surface cannot be acquired
  Then the next frame draws nothing for it unless it is placed again

Scenario: A movable mesh never draws at the identity on its own
  Given a movable mesh was upserted and flushed
  When a frame renders with no placements
  Then no draw is issued for it

Scenario: Static meshes are unchanged
  Given a mesh upserted with upsert(id, mesh, material)
  When a frame renders
  Then it draws once at the identity in its upsert material, as before

Scenario: Placing a static mesh adds a copy
  Given a static mesh is live
  When the game places it at a transform
  Then it draws at the identity and at that transform

Scenario: A placement on an id with no live mesh draws nothing
  Given no mesh is live under an id, or it was removed this frame
  When the game places that id
  Then no draw is issued for it and nothing panics

Scenario: Replacing or removing a movable mesh follows the static rules
  Given a movable mesh is live
  When it is upserted again, upserted empty, or removed
  Then the old upload is freed exactly once and the new one, if any, is drawn where placed
```

## Tech spec

**Design.** Immediate-mode placements on the existing retained mesh store; nothing in
`moho_render_api` changes.

- `moho_renderer::world_meshes`
  - The pending and live entries gain a kind: `Static { material_idx }` (today's
    behaviour) or `Movable`. `upsert_movable(id, mesh)` queues a `Movable` upload; an
    empty mesh is a remove, as for `upsert`. Upserting an id with the other method
    replaces it and switches its kind.
  - `flush` uploads a movable mesh's `materials` channel when it has one and an empty
    slice otherwise; `interleave_vertices` fills a missing channel with
    `NO_VERTEX_MATERIAL`, so `vertex.wgsl` uses the placement's material.
  - `place(id, transform: glam::Affine3A, material: u32)` appends to a
    `BTreeMap<WorldMeshId, Vec<(Affine3A, u32)>>`. It is not checked against live ids at
    call time: an upsert queued the same frame is flushed before draws.
  - `draws()` keeps its item type `(u32, InstanceGpu)`, one per instance: each static
    mesh's identity instance, then every placement of a live id in id order and, within
    an id, call order. A placement on an id that is not live after `flush` is skipped.
    Instances use `object_type: 0`, as world meshes do today.
  - `clear_placements()` drops every placement.
- `moho_renderer::scene::Scene::render`: after `flush`, collect `world_meshes.draws()`
  grouped by consecutive backend handle into `(handle, Vec<InstanceGpu>)`, call
  `clear_placements()`, and only then `begin_frame`, so a skipped frame also consumes
  them. Each group is one `enqueue_draw`, in the opaque section where world meshes
  draw today.

**Why.** Immediate placements over retained per-instance handles: the game already reads
sim state every frame to draw (ADR-0011) and passes the interpolated transform, and the
backend re-uploads the whole instance buffer every frame anyway, so retained instances
would add ids, hide flags and stale-state bugs for no saved work. "Hide" is not placing it.
`Affine3A` over `Mat4` keeps projective matrices out of the model slot.

**ADR-0023, World meshes are placed per frame** (file `_todo/adr/0023-world-meshes-are-placed-per-frame.md`,
README index row; Status Accepted on merge, amends ADR-0010):
- Context: ADR-0010's contract draws each mesh once at the identity, so moving geometry
  is re-uploaded per frame. Characters, loose objects (#600), models (ENG-F39) and
  effects (ENG-F40) all need to move.
- Decision: a world mesh is uploaded once, as static (drawn at the identity every frame
  until replaced or removed) or movable (drawn only where placed). Each frame the game
  places any live mesh at an affine transform with a material; placements last one
  frame. The game composes hierarchies (a model's nodes, a socket) and interpolates
  between ticks; the engine receives final world transforms. Placements are
  presentation only and never create or move physics colliders.
- Consequences: one mesh path for shadows, SSAO and materials; one instanced draw per
  mesh; baked-lighting channels on a movable mesh are not moved lighting, so games fill
  them with neutral values (searchlight already does); normals transform by the model
  matrix, which is exact for rotation, uniform scale and axis-aligned faces under
  non-uniform scale, and a general mesh under non-uniform scale needs the cofactor
  normal transform, which ENG-F39 adds with its first such model; skinning, per-camera
  visibility and per-placement fields are added by the feature that needs them.

**Out of scope.**
- Searchlight adoption (enemies through `place`): FPS card, after this merges.
- Flagstone actors: they keep the `Renderable` sphere and cube path.
- Shader changes, including the normal (cofactor) transform: ENG-F39.
- Transparent placements: placed meshes draw opaque, like static world meshes.
- Culling, GPU-driven instancing, the per-call instance upload inside `Renderer::enqueue_draw`.
- Physics colliders for moving meshes (characters and #600 bodies own theirs).
- Skin palettes, per-camera hiding, own field of view (ENG-F39), emissive or unlit effects (ENG-F40).
- Any change to `WorldMesh`, `WorldMeshId` or `InstanceGpu` in `moho_render_api`.

**Footprint** (one part):
- `moho_renderer/src/world_meshes.rs`: kinds, `upsert_movable`, `place`, `clear_placements`, `draws`; unit tests.
- `moho_renderer/src/scene.rs`: group world draws, clear placements before `begin_frame`.
- `moho_renderer/tests/scene_draw_order.rs`: scene-level tests.
- `_todo/adr/0023-world-meshes-are-placed-per-frame.md`, `_todo/adr/README.md`: the ADR.
- `wiki/architecture/rendering.md`: section 2 and the frame sequence diagram.

**Test map** (all part A):

| Scenario | Test |
|---|---|
| Moving uploads nothing | `world_meshes::tests::moving_a_placed_mesh_registers_it_once` |
| Several transforms | `scene_draw_order::placements_of_one_mesh_are_one_draw_in_placement_order` |
| Own material | `world_meshes::tests::placement_material_reaches_the_instance_and_vertices_defer` |
| Not placed, not drawn | `world_meshes::tests::unplaced_movable_mesh_draws_nothing_and_stays_registered` |
| Skipped frame | `scene_draw_order::failed_begin_frame_still_consumes_placements` |
| No identity draw | `world_meshes::tests::movable_mesh_has_no_identity_draw` |
| Static unchanged | existing `registered_mesh_is_drawn_after_flush` and siblings, unchanged |
| Placing a static mesh | `world_meshes::tests::placing_a_static_mesh_adds_a_copy` |
| No live mesh | `world_meshes::tests::placement_on_a_missing_or_removed_id_is_skipped` |
| Replace and remove | `world_meshes::tests::live_handles_match_live_ids` extended with `UpsertMovable` and `Place` ops: drawn instances equal the placements of live ids plus one per live static id, no leaked or double-freed handle |
| Edge: upsert and place in one frame | `world_meshes::tests::mesh_upserted_and_placed_in_one_frame_draws` |
| Edge: kind switch | `world_meshes::tests::upserting_with_the_other_kind_switches_it` |

The fixture's `RecordingBackend` in `scene_draw_order.rs` gets a `begin_frame` that can
fail, for the skipped-frame test.

**Gate class:** glue (`moho_renderer` is not a domain path).

**Risks.**
- Draw order: placed meshes join the opaque world draws; order is deterministic (id, then call).
- Performance: fifty placed meshes add fifty instances (80 B each) and at most one draw per
  distinct mesh, fewer than today's one draw per uploaded box. The GTX 1080 frame-budget
  check runs with the FPS adoption card, where fifty moving things exist.
- Blast radius: `WorldMeshes` callers in flagstone and searchlight keep compiling;
  `draws()` yields the same items for static meshes.

## Verification
None in-game for this card; the FPS adoption card is where moved meshes are seen.

## Notes
- Depends on nothing; first card of ENG-F38 and the base for #600, ENG-F39 and ENG-F40.
