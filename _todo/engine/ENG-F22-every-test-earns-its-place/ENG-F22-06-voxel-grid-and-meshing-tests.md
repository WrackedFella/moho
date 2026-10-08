# Voxel grid and meshing tests fail for wrong implementations

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#173](https://github.com/WrackedFella/moho/issues/173)
**Status:** Backlog
**Gate class:** domain
**Labels:** line:engine

## Summary

`moho_core`'s live voxel grid, chunk, blocky and coarse meshing tests stop passing for wrong implementations, and chunk persistence edges and negative coordinates get tests. Each test is shown red against the wrong implementation it targets.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.
- `chunk` `test_chunk_empty` and `mesh::marching_cubes` `test_corner_position` are deleted once their replacements here land.

## Acceptance criteria

| Test (`moho_core::voxel`) | Pins | Red against |
|---|---|---|
| `grid` `test_is_smooth_at` | material 99 → `Some(true)` (with 0 and 100) | `< 99` |
| `grid` `block_positions_reconstruct_world_coords` (from `test_block_positions_count`) | the set is exactly {(0,0,0), (1,0,0), (-1,17,3)} | x and z swapped in local reconstruction |
| `grid` `test_get_height` | blocks at y=17, y=20 and y=-3 in one column, plus one in another column's chunk → `Some(20)`, over 16 fresh grids (chunk storage is a randomly seeded `HashMap`) | first chunk's hit returned |
| `grid` `test_iter_block_data` | the exact set of (position, material, resource), one block with a resource | material and resource swapped |
| `grid` `test_chunk_block_data` | chunk (1,0,0) → [(20,0,0), material 2, no resource] | chunk base offset dropped |
| `grid` `negative_positions_round_trip` | place and read back (-1,-1,-1), (-16,0,0), (-17,5,31) | truncating division in chunk lookup |
| `grid` `overwrite_without_resource_clears_resource` | place with resource 7, re-place with none → no resource | the "none removes" arm deleted |
| `grid` `chunk_persists_only_when_modified` | `serialize_chunk` is `None` for an absent or unmodified chunk and after `clear_chunk_modified` | modified flag never cleared |
| `grid` `chunk_round_trips_into_other_chunk_pos` | a chunk at (2,0,-1) round-trips with world positions intact, and the loaded chunk's light is marked dirty | base offset dropped on load |
| `grid::paletted` `test_palette_deduplication` | re-setting an existing material leaves `to_bytes().len()` unchanged (no private `palette.len()`) | no dedup |
| `chunk` `test_chunk_with_geometry` | vertices with no indices → `!has_geometry()`; both → true | `&&` → `||` |
| `chunk` `test_from_grid_lod_stamps_lod_field` | a smooth block at (1,1,1): every LOD 1 coordinate is an even integer; some LOD 0 coordinate is not an integer | `lod` ignored |
| `mesh::blocky` `test_corner_block_has_occlusion` | neighbours at (1,1,0) and (0,1,1): the +Y vertex at (1,1,1) has AO ≈ 0.4, the one at (0,1,0) 1.0 (the block's local cube is [0,1]³ since #196); vertices found by position and normal | no "both sides → max occlusion" rule (gives 0.6) |
| `mesh::blocky` `all_indices_in_range` | referenced indices are exactly 0..24 | `base_index = 0` |
| `mesh::hybrid` `test_generate_coarse_mesh_solid_chunk_has_geometry` | full 16³ chunk: 2560 vertices, 3840 indices, 128 quads facing +X | culling disabled |
| `mesh::hybrid` `test_generate_coarse_mesh_uses_stride2` | a single block at (1,1,1) yields geometry | only the cell root sampled |
| `mesh::marching_cubes` `test_full_density_field` | a 2³ solid at samples 6..=7: vertices in [5.5,7.5]³, normals point away from (6.5,6.5,6.5), trilinear density at each vertex ≈ 0.5, all per-vertex arrays the same length; drop the "outer shell" comment | corners 1 and 3 swapped |

## Tech spec

**Design:** tests only, in the existing test modules; observe through public `VoxelGrid` and `VoxelChunk` API, except `paletted`, whose own type is the unit.

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- `BlockModifier`, `ChunkState`, `MeshJobQueue` and face-culling tests: #185 (ENG-F22-18) deletes them.
- Mixed smooth+blocky chunks and blocky alignment: #183 (ENG-F22-16).
- Single-voxel normals: #184 (ENG-F22-17) (that is why this card uses a 2³ solid).
- Whether adjacent blocky faces are culled: deferred to SG-F3.

**Test map:** the table above.

**Gate class:** domain (`moho_core` voxel rules).

**Risks:** #140 (ENG-F10-03) moves these files; land before its branch is cut or after it merges. ENG-F4-04 (±Y skirts) would change the coarse counts; update them there.
