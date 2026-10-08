# Lighting tests fail for wrong implementations

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#174](https://github.com/WrackedFella/moho/issues/174)
**Status:** Backlog
**Gate class:** domain
**Labels:** line:engine

## Summary

`moho_core`'s sky exposure, block-light propagation and light-job tests stop passing for wrong implementations, and two unpinned rules (sky occlusion stays in its column; equal-level ties in removal) get tests. Each test is shown red against the wrong implementation it targets.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.

## Acceptance criteria

| Test | Pins | Red against |
|---|---|---|
| `tests/lighting.rs` `sky_recompute_reflects_removed_block` (replaces `sky_recompute_is_idempotent`) | place opaque, recompute, remove it, recompute → the voxel below is exposed again | column max rebuilt monotonically |
| `tests/lighting.rs` `sky_dirty_chunks_below_propagates` | place first, clear chunk (0,0,0)'s flag, call `dirty_chunks_below` → it is dirty (`expect`, no `if let`); a chunk in another column and the chunk at `chunk_pos` stay clean | x/z column filter dropped |
| `tests/lighting.rs` `sky_ensure_ready_noop_when_clean` | recompute, place opaque, clear the flag, `ensure_chunk_sky_ready` → the voxel below still reads exposed (stale proves no recompute) | `sky_dirty` guard ignored |
| `tests/lighting.rs` `sky_block_in_other_chunk_column_does_not_occlude` | opaque at (21,20,5), recompute (1,1,0) then (0,0,0) → (5,15,5) exposed | x/z filter dropped |
| `tests/lighting.rs` `block_light_colored_propagation` | emission [15,8,2]: [14,7,1] at x=1, [13,6,0] at x=2; unused `make_torch` removed | every channel uses R's level |
| `tests/lighting.rs` `removal_of_absent_light_is_noop` | source 15 at x=0; `remove_light` at (20,0,0) → affected set empty, x=0..2 unchanged | removal clears light elsewhere |
| `tests/lighting.rs` `removal_with_adjacent_equal_sources_keeps_survivor` | [15,0,0] at x=0 and x=1; remove x=0 → x=0,1,2 read 14, 15, 14 | `<` → `<=` in the surviving-source check |
| `tests/lighting.rs` `flood_fill_propagates_all_emitters` | [15,8,2] at both torches, exactly [13,6,0] at x=2 | only R seeded |
| `light_jobs` `test_job_priority_ordering` | through `LightJobQueue` with an unlimited budget, results come back as priorities [25, 50, 100] | FIFO queue |
| `light_jobs` `test_job_cancellation` | cancel, `process_frame` → the source reads [0,0,0] and the result is cancelled; cancelling again → `false`; double-cancel assert dropped | token ignored |
| `light_jobs` `test_frame_budget` | budget of 1 block, 3 jobs → 1 per frame, 2 pending after the first | budget ignored |
| `light_jobs` `test_stats_tracking` | unlimited budget → `total_jobs_processed == 3`; time assert dropped | counted per frame |
| `light_system` `emit_dirty_events_publishes_one_event_per_affected_chunk` (replaces `test_process_frame_with_no_jobs`) | arrange with `on_block_placed` of an emissive block, `process_frame`; `emit_dirty_events()` returns the affected chunk count and the bus receives that many `ChunkMeshDirty`; a second call returns 0 | always returns 0 |

## Tech spec

**Design:** tests only. Lighting fixtures register emissive materials through `MaterialRegistry::set_lighting`.

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- `LightSystem::on_block_removed` not relighting, opaque placement shadowing, `handle_event` dispatch, batch removal: deferred (`LightSystem` receives no world events in production).
- Translucent `opacity_cost` 1..=14 and non-16 grids: open decisions.

**Test map:** the table above.

**Gate class:** domain (`moho_core` lighting rules).

**Risks:** #140 (ENG-F10-03) moves these files; land before its branch is cut or after it merges.
