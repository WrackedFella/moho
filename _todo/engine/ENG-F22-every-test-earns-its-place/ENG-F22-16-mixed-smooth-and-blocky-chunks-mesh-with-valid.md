# Mixed smooth and blocky chunks mesh with valid indices and aligned geometry

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#183](https://github.com/WrackedFella/moho/issues/183)
**Status:** Backlog
**Gate class:** domain
**Labels:** line:engine

## Summary

Chunks that mix smooth terrain (materials below 100) with blocky blocks (100 and up) mesh wrongly: blocky indices point past the vertex array, and the smooth part sits half a block off. Blocky cubes are also centred on the block's corner instead of filling it. Nothing places material 100 or higher yet, so players can't see it today; SG-F3 buildings will. After this, every mesh path puts block V in [V, V+1] and every index is valid.

## Deliverables

- Mixed chunks produce indices that are all below the vertex count, with the smooth part aligned like an all-smooth chunk.
- Blocky meshes fill [V, V+1] like the smooth and coarse paths.

## Acceptance criteria

```gherkin
Scenario: A mixed chunk's indices are valid
  Given a chunk with a smooth block and a blocky block
  When the chunk is meshed at full detail
  Then every index is less than the vertex count

Scenario: Smooth terrain in a mixed chunk is aligned
  Given a smooth block at (5,5,5) and a blocky block elsewhere in the chunk
  When the chunk is meshed
  Then the smooth vertices are centred on (5.5, 5.5, 5.5)

Scenario Outline: Every mesh path fills the voxel it represents
  Given a <kind> block at <position>
  When its chunk is meshed at LOD <lod>
  Then the block's vertices span <span> on every axis

  Examples:
    | kind   | position | lod | span        |
    | smooth | (5,5,5)  | 0   | centre 5.5  |
    | blocky | (5,5,5)  | 0   | [5, 6]      |
    | smooth | 2³ cell at (4,4,4) | 1 | [4, 6] |
```

## Tech spec

**Design**
- `HybridMeshGenerator::append_mesh_with_offset` adds the target's current vertex count once (the separate smooth offset double-counts it); the mixed path applies the same density-to-world shift as the smooth path.
- The blocky path offsets its cube by +0.5 so block V spans [V, V+1].

**Test map**
| Scenario | Test |
|---|---|
| Mixed indices valid | `tests/mesh_alignment.rs` `mixed_chunk_indices_are_in_range` |
| Mixed smooth aligned | `tests/mesh_alignment.rs` `mixed_chunk_smooth_part_is_aligned` |
| Every path fills its voxel | `tests/mesh_alignment.rs` `mesh_is_centered_on_the_voxel_it_represents` (parameterizes the existing smooth test) |

**Out of scope:** culling faces between adjacent blocky blocks (deferred to SG-F3); greedy meshing.

**Gate class:** domain (`moho_core` meshing).

**Risks:** #140 (ENG-F10-03) moves these files; land before its branch is cut or after it merges.

## Notes

Reproduced on `dev` with a throwaway test: a chunk with one smooth and one blocky block had max index 71 for 48 vertices, and its smooth part was centred at 6.0 instead of 5.5.
