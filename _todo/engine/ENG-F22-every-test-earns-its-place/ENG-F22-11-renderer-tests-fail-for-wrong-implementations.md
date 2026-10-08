# Renderer tests fail for wrong implementations and every loaded shader is validated

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#178](https://github.com/WrackedFella/moho/issues/178)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

The renderer's instance preparation, material table, skybox and shader tests stop passing for wrong implementations; every shader production loads is validated; and transparent draw order, the opaque/transparent split and instance-buffer growth get tests. Each test is shown red against the wrong implementation it targets.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.
- A seam `pipeline::shaders::main_shader_source()` used by `load_shaders`, so tests validate the exact production string; `test_shader_concatenation` is deleted.
- A seam `MeshRenderer::next_capacity(current, required) -> usize` used by `ensure_capacity_and_upload`.
- `instance_collector` `test_instance_collector_new` and `resources` `test_initial_material` are deleted once their replacements land.

## Acceptance criteria

| Test | Pins | Red against |
|---|---|---|
| `mesh_renderer` `prepare_instances_copies_every_field_in_order` (merges `test_prepare_instances_single` and `_multiple`) | two instances with distinct model translation, material, object type and padding; output equals the expected bytes (`bytemuck` comparison; the types don't derive `PartialEq`) | `object_type` hard-coded 0 |
| `mesh_renderer` `test_flatten_instances_multiple_draws` | draws with materials 1, 2, 3: `instances[offsets[k]].material == k+1` and the last instance has material 3 | draws concatenated in reverse |
| `mesh_renderer` `next_capacity_grows_by_doubling` | (0,0) → 1; (1,3) → 4; (8,5) → 8; (4,5) → 8 | off-by-one in the doubling loop |
| `instance_collector` `test_collect_spheres` | three spheres with materials a, b, a → instance materials [0, 1, 0], table length 2 | material index ignored |
| `instance_collector` `test_clear` | a sphere, a cube and (until #138 lands) a chunk collected; after `clear` every list is empty | cube list not cleared |
| `materials` `dedup_materials_basic` | after `clear_dirty()`, looking up an existing material returns its index and leaves the table clean | dirty set on every lookup |
| `moho_render_api::material` `is_transparent_only_when_params2_positive` | `params[2]` 0.0 → false, 0.5 → true | constant false |
| `resources` `test_skybox_quad_generation` | `count == vertices.len() == 6`; every vertex is a corner (±1, ±1, 0); each triangle uses three distinct corners; the two omitted corners are diagonally opposite | a triangle covering the wrong half |
| `block_on` `pending_future_parks_instead_of_spinning` | polls within 2..10 (a spurious unpark may poll again) | spinning |
| `tests/shaders_validation.rs`, one test per shader | `main_shader_source()`, `skybox.wgsl`, `shadow.wgsl` (with `Capabilities::IMMEDIATES`), `gtao.wgsl`, `ssao_blur.wgsl` parse and validate; paths via `CARGO_MANIFEST_DIR`; the unused `instance.wgsl` is no longer the subject | a syntax error in `fragment.wgsl` |
| `tests/scene_draw_order.rs` `transparent_instances_draw_back_to_front_grouped_by_mesh` | eye at the origin; transparent sphere at 5, cube at 4, cube at 3, sphere at 1 → calls (sphere,[5]), (cube,[4,3]), (sphere,[1]); empty opaque calls ignored | front-to-back sort; no grouping |
| `tests/scene_draw_order.rs` `transparent_material_routes_to_transparent_pass`, `out_of_range_material_index_draws_opaque`, `materials_upload_only_when_table_changed` | routing by material; fallback to opaque; `set_materials` called once for two identical frames and again when a new material appears | always upload |

## Tech spec

**Design:** the draw-order tests drive `Scene::render` through a recording `RendererBackend` fake with local `Renderable` and `RenderMaterial` fixtures, in a new file, so they don't depend on `moho_game` and don't collide with #138's rewrite of `scene_render_mock.rs`.

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- `buffer_manager` tests, `test_collect_chunks`, `scene_render_mock.rs`: #138 (ENG-F10-01).
- `frame_ops` tests: #156 (ENG-F20-02).
- Deleting the unused shader files (`instance.wgsl`, `instance_old.wgsl`, `pcss.wgsl`): not decided here.
- cwd-relative shader loading at runtime: not touched.

**Test map:** the table above.

**Gate class:** glue (renderer).

**Risks:** #155 (ENG-F20-01) adds a UI shader to `shaders_validation.rs`; whichever lands second adds its row to the other's layout.

**Deviations (implementation):**
- `main_shader_source` stays in the private `pipeline::shaders` module and is re-exported as `pipeline::main_shader_source`, so the integration test reaches it without making `load_shaders` public.
- `Renderer::prepare_instance_buffer` repeated the doubling loop and upload; it now calls `MeshRenderer::ensure_capacity_and_upload`, so `next_capacity` covers both growth paths.
- `test_clear` collects a sphere and a cube only: #138 removed the chunk list.
