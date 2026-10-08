# Game, audio and renderer tests carry no duplicates or tests of uncalled code

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#170](https://github.com/WrackedFella/moho/issues/170)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

In `moho_game`, `moho_audio`, `moho_renderer` and `moho_render_api`, accessor-plumbing tests, strict subsets of a neighbour, `thiserror` Display tests, compile-only tests and tests of uncalled code are deleted, with the uncalled code. Same-scenario tests become tables. Removal and merge only.

## Deliverables

- The removals and merges below are done, with the listed dead code deleted.
- The PR shows, per touched source file, that the branch misses no mutant that `dev` catches.

## Acceptance criteria

`moho_game`:
- [ ] `simulation`: remove `controller_input_mut_when_written_updates_controller_input` (the movement test writes through the same accessor) and `set_camera_mode_when_isometric_changes_reported_mode` (the isometric look-at test fails if the setter is a no-op).
- [ ] Merged: `simulation` `position_when_constructed_returns_initial_position` → the forward-movement test, which starts at (1,2,3) and expects (1,2,5) after forward=1, dt=0.5 (both position and camera asserts updated).
- [ ] `actors`: remove `center_mut_moves_only_the_identified_actor`; the test at its side that also covers cubes becomes the "an actor moves by its handle" test (#65) by name or doc comment.
- [ ] `actors`: remove `handle_index_beyond_current_actors_resolves_to_none_after_clear` (passes with or without the generation check). Merged: `cube_handle_from_before_clear_resolves_to_none` + the sphere twin → `handle_from_before_clear_resolves_to_none`, rows {sphere, cube}, keeping the length and centre asserts for both.
- [ ] Merged: `scene_persistence` `save_to_file_then_load_from_file_round_trips` → `scene_file_round_trips` (keep that name; carry over `lights[0].intensity == 3.0`).

`moho_audio`:
- [ ] `audio_cache`: remove `test_cache_size_tracking`, `test_clear_audio_cache` and `test_clear_all_caches` with the uncalled `AudioCache::{audio_cache_size, ui_cache_size, total_cache_size, clear_audio_cache, clear}` and `AudioSystem::{cache_size, clear_cache}`.

`moho_renderer` / `moho_render_api`:
- [ ] `scene::preparation`: remove the four `test_prepared_scene_*` tests with `PreparedScene::empty` and `is_empty` (marked "used in tests") and the then-unused test helper.
- [ ] `mesh_renderer`: remove `test_prepare_instances_empty` and `test_flatten_instances_single_draw`.
- [ ] Remove `pipeline` `test_pipeline_init_error_display`, `device` `test_device_init_error_display` (log-only `thiserror` text) and `builder` `test_builder_compiles` (`Renderer::new` runs the same chain).
- [ ] Replace `render_ops::shadow_ops` `test_max_shadow_lights` with a relation in `gpu_types` `sizes_are_expected`: `size_of::<MultiLightShadowGpu>() == MAX_SHADOW_LIGHTS * size_of::<ShadowMatrixGpu>() + 32`, which ties the constant to the GPU layout.
- [ ] Remove `tests/compile_api.rs` with `create_renderer_from_arc` (no consumer).
- [ ] Delete `MaterialTable::reset_for_tests` (no caller at all).
- [ ] Merged: `instance_collector` `test_collect_cubes` → `collect_routes_spheres_cubes_and_chunks_to_their_lists` (from `test_collect_mixed_objects`), keeping its two-cubes-one-material case.

## Tech spec

**Design:** test-only edits plus deleting the named uncalled items.

**Proof of no loss:** `cargo mutants --file` on `dev` and the branch for `moho_game/src/{simulation,actors,scene_persistence}.rs` and `moho_audio/src/{audio_cache,lib}.rs` (renderer files are excluded in `.cargo/mutants.toml`; list them as skipped). The branch adds no missed mutant, or the PR names each new one and why it is acceptable.

**Out of scope**
- `simulation`'s clock tests (`game_clock_mut…`, `set_time_of_day…`, `celestial_directions…`): #145 (ENG-F11-03) deletes that API.
- `moho_game/tests/save_load.rs`: #141 (ENG-F10-04) names it as a test.
- `buffer_manager` tests, `test_collect_chunks`, `tests/scene_render_mock.rs`: #138 (ENG-F10-01) rewrites them.
- `render_ops::frame_ops` tests: #156 (ENG-F20-02) deletes them.
- Tests whose removal needs a strengthened replacement first (`test_instance_collector_new`, `test_initial_material`, the `prepare_instances` merge): in #178 (ENG-F22-11).

**Test map:** the checklist; each line names its test.

**Gate class:** glue (no rule changes; tests removed or folded).

**Risks:** low.

## Notes

- PR #68 kept its adversarial-challenge tests on purpose. The two `actors` items above revisit that: one cannot fail on its own, and the other differs from its twin only by actor kind. Approving this card approves both.
