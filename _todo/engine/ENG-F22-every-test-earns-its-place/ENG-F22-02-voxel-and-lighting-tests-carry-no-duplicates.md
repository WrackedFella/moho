# Voxel and lighting tests carry no duplicates or tests of uncalled code

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#169](https://github.com/WrackedFella/moho/issues/169)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

`moho_core`'s voxel and lighting tests lose their duplicates, constructor-only tests and tests of code nothing calls; the uncalled code goes with its test. Same-scenario tests become tables. Removal and merge only.

## Deliverables

- The removals and merges below are done, with the listed dead code deleted.
- The PR shows, per touched source file, that the branch misses no mutant that `dev` catches.

## Acceptance criteria

Voxel (`moho_core::voxel`):
- [ ] `grid`: remove `test_new_grid`; `test_has_block_at` with `VoxelGrid::has_block_at` and `is_block_occupied`; `test_material_registry` with `MaterialRegistry::get`; `test_resource_registry` with `ResourceRegistry`; `test_block_category_smooth` and `test_block_category_blocky` with `VoxelBlock` and `BlockCategory`. None has a caller outside `grid` and its tests.
- [ ] `grid::paletted`: remove `test_empty_chunk_is_all_air` with `PalettedChunk::is_all_air`; remove `test_set_and_clear_block` (`grid`'s `test_place_and_query` and `test_clear_block` pin it).
- [ ] `chunk`: remove `test_chunk_mesh_handle`; `test_chunk_memory_size` with `VoxelChunk::memory_size`.
- [ ] `face`: remove `test_all_faces`, `test_get_visible_faces_isolated_block`, `test_vertex_ranges` with `FaceDirection::vertex_range` and its table, `test_index_ranges` with `index_range` and its table.
- [ ] `jobs`: remove `test_cancellation_token` (tests std) and `test_job_id_uniqueness` (`state`'s test pins it).
- [ ] `tests/voxel_system.rs`: remove `test_voxel_block_basics`; remove `test_voxel_grid_heights_and_neighbors` with `VoxelGrid::get_neighbor_heights` (no caller; `test_get_height` pins the rest).
- [ ] Merged: `mesh::hybrid` `test_analyze_all_smooth_chunk` + `test_analyze_all_blocky_chunk` → `analyze_chunk_classifies_uniform_chunks`, rows (material, content, smooth, blocky): (0, AllSmooth, 64, 0), (99, AllSmooth, 64, 0), (100, AllBlocky, 0, 64).
- [ ] Merged: `tests/chunk_remesh.rs` `removing_surface_voxel_changes_chunk_mesh` → `mesh_change_is_local_to_the_removed_voxel`, which gains `assert!(grid.mutator().remove(target))`; the `eprintln!`s go.

Lighting:
- [ ] `light_sky`: remove `open_air_chunk_is_fully_sky_exposed`, `voxel_below_opaque_block_is_not_exposed`, `block_in_chunk_above_occludes_chunks_below` (identical twins in `tests/lighting.rs`).
- [ ] `light_propagation`: remove `test_incremental_light_addition`, `test_light_removal_basic`, `test_light_removal_with_multiple_sources`, `test_light_removal_corner_case`, `test_cross_chunk_propagation`, `test_cross_chunk_removal` (twins in `tests/lighting.rs`).
- [ ] `light_propagation`: remove `test_chunk_boundary_detection` with `LightPropagator::get_chunk_coord`, the then-unread `chunk_size` field and `LightPropagator::new`'s argument (callers: `LightJobQueue::new`, `moho_game`'s `scene_builders`).
- [ ] `light_jobs`: remove `test_budget_presets`. `light_system`: remove `test_light_system_creation`.
- [ ] Merged: `light_propagation` `test_light_propagation_basic` → `tests/lighting.rs` `block_light_decay_one_per_step`, which asserts all six face neighbours of the source read 14 (today only +x, -x and -y are pinned).
- [ ] Merged: `light_propagation` `test_incremental_light_stops_at_brighter` + `tests/lighting.rs` `block_light_does_not_dim_brighter` → `weaker_source_does_not_dim_brighter`: pre-light a row at 10, add a source of 8, assert the source and x=4 still read 10.
- [ ] Merged: `tests/lighting.rs` `block_light_cave_is_pure_black` → `unlit_position_reads_zero`, rows "no `ChunkLight` allocated" and "allocated but out of range" (e.g. (7,7,7) from a level-10 source at the origin). `accessor_round_trip` keeps its RGB round trip; its sky assertion moves to its own test `transparent_block_does_not_occlude_sky` (the only pin of the transparent filter in sky occlusion).

## Tech spec

**Design:** test-only edits plus deleting the named uncalled items and the `voxel` re-exports that name them.

**Proof of no loss:** `cargo mutants --file` on `dev` and the branch for `grid.rs`, `grid/paletted.rs`, `chunk.rs`, `face.rs`, `mesh/hybrid.rs`, `light_sky.rs`, `light_propagation.rs`, `light_jobs.rs`, `light_system.rs`; the branch adds no missed mutant, or the PR names each new one and why it is acceptable.

**Out of scope**
- `BlockModifier`, `ChunkState`, `MeshJobQueue`/`jobs`, the face-culling functions and their remaining tests: #185 (ENG-F22-18) deletes them. Don't merge `modification` or `face` tests.
- `LightSystem`'s event handlers: deferred.
- Any test strengthening: #173 (ENG-F22-06) and #174 (ENG-F22-07).

**Test map:** the checklist; each line names its test.

**Gate class:** glue (no rule changes; tests removed or folded).

**Risks:** #140 (ENG-F10-03) moves these files to `moho_voxel` and checks the test count against `dev`. Land this before that branch is cut or rebase onto it after it merges.
