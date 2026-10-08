# Game rule tests fail for wrong implementations

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#176](https://github.com/WrackedFella/moho/issues/176)
**Status:** Backlog
**Gate class:** domain
**Labels:** line:engine

## Summary

`moho_game`'s clock, pawn, raycast, terrain generation, persistence and controller tests stop passing for wrong implementations, and the controller's movement rules, terrain layering, biome shapes and surface-raycast edges get tests. Each test is shown red against the wrong implementation it targets.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.

## Acceptance criteria

| Test (`moho_game`) | Pins | Red against |
|---|---|---|
| `game_clock` `test_time_wrapping` | `set_time(-1.0)` → 23.0; `GameClock::new(30.0, ..)` → 6.0 | `%` instead of `rem_euclid`; `new` not wrapping |
| `game_clock` `test_daytime_nighttime` | 6.0 is day, 18.0 is night | `> 6.0` |
| `game_clock` `test_clock_tick` | day 120 s, night 60 s: 12.0 + tick(1) → 12.1; 0.0 + tick(1) → 0.2; 23.95 + tick(1) → 0.15 (within 1e-4) | day length used at night |
| `pawn` `deposit_updates_matching_inventory_counter` | deposit amount 5 → count 5 | `amount` ignored |
| `pawn` `mine_with_nothing_in_range_returns_none` | origin (0.5,0.5,0), block at (0,0,12), max 10 → `None`, block present, inventory empty | max distance doubled |
| `raycast` `raycast_returns_none_when_nothing_in_range` | origin 0, +Z, block (0,0,11), max 10 → `None`; block (0,0,10) → `Some` | `<=` → `<` or doubled max |
| `raycast` `raycast_surface_edge_cases` | zero direction → `None`; origin inside a lone block → `None`; block (0,0,12) from (0.5,0.5,0): max 10 → `None`, max 13 → distance 12.0 ± 0.01 | cutoff ignored |
| `scene_builders` `generate_chunk_outside_world_returns_empty` | world size 32: chunks (1,0,0), (-2,0,0), (0,0,1), (0,0,-2) empty; (-1,0,0) not | half-size check without `/2` |
| `scene_builders` `generate_chunk_matches_full_terrain` | full (position, material, resource) tuples equal the whole-terrain generator's for chunks (0,4,0) and (-1,4,-1) | wrong ore seed in `generate_chunk` |
| `scene_builders` `terrain_column_layers_and_ore_band` | column height 70: y 70, 69 surface; 68, 67 subsurface; 66 base; no ore at y ≥ 67 or ≤ 5 across an x/z/seed sweep, some ore in 6..=66 | surface and base swapped; ore band dropped |
| `biome` `biome_shapes` | `Cliffs.shape(0.3) == 0.25`, `Cliffs.shape(-0.1) == -0.25`, `Canyon.shape(-1.0) == -2.0`, `Canyon.shape(1.0) == 0.5` | shaping removed |
| `biome` `different_seeds_pick_different_biomes` | `BiomeMap::new(7)` vs `new(8)` with [Plains, Mountains] differ somewhere on a sweep | seed ignored |
| `scene_persistence` `round_trip_preserves_spheres_cubes_and_chunks` | all five `MaterialType` variants with distinct field values; chunks compared per position | `VoxelTerrain` top/side swapped; Emissive intensity dropped |
| `scene_persistence` `load_into_populated_entities_appends_actors_and_replaces_chunk_at_same_position` | target holds sphere A and chunk P (material 11); loading sphere B, chunk P (22), chunk Q → spheres [A, B], two chunks, P is 22 | clear before load |
| `controller` `movement_rules` (table) | sprint: forward 1, dt 0.5 → +3.0 z; pitch ±10 → ±1.54; isometric forward 1, dt 1 → look target += (-2.828, 0, -2.828), position unchanged; isometric up → no change; zoom +100 → height 5, -100 → 50 | sprint ignored; pitch unclamped; isometric moves position |
| `tests/surface_raycast.rs` `surface_raycast_reports_hits_on_the_visible_surface` | for each hit, removing `hit.block_pos` and re-casting gives `None` or a hit farther than `hit.distance + 0.01`; the dead DDA bookkeeping goes; the measured baseline is recorded next to the 95 % threshold | hit block chosen behind the surface |

## Tech spec

**Design:** tests only. Terrain layering tests call the crate-private `determine_material_id` and `determine_resource_id` directly.

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- `generate_chunk`'s fast-reject is an equivalent mutant (the ranges already clamp); accept the survivor.
- `Pawn::select_slot` out of range: #187 (ENG-F22-20). `MineOutcome.distance` doc vs `None` from inside: open decision.
- `simulation`'s clock API: #145 (ENG-F11-03) deletes it. If that card merges first, the `game_clock` rows land in the clock's new home.

**Test map:** the table above.

**Gate class:** domain (`moho_game` rules).

**Risks:** ENG-F21 moves the controller; land the controller table first so the move is proven by unchanged tests.
