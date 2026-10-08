# Lone smooth voxels have outward normals

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#184](https://github.com/WrackedFella/moho/issues/184)
**Status:** Backlog
**Gate class:** domain
**Labels:** line:engine

## Summary

A one-voxel smooth feature (a lone block left after mining, a thin spire) gets some normals pointing straight up instead of outward, so it shades wrongly. After this, every vertex normal on a smooth surface points away from the solid.

## Deliverables

- Marching-cubes normals point outward for single-voxel features.

## Acceptance criteria

```gherkin
Scenario: A lone solid voxel has outward normals
  Given a density field with one solid sample
  When it is meshed
  Then every vertex normal points away from the sample's centre

Scenario: A 2x2x2 solid keeps outward normals
  Given a density field with a 2x2x2 solid
  When it is meshed
  Then every vertex normal points away from the solid's centre
```

## Tech spec

**Design:** the normal comes from a central difference of the density at the vertex's floored sample, which is zero for vertices whose floor is the solid sample itself, so the code falls back to up. Use the gradient at the vertex position (interpolated, or from the edge's two endpoint gradients) so the difference is never sampled on a flat neighbourhood.

**Test map**
| Scenario | Test |
|---|---|
| Lone voxel | `mesh::marching_cubes` `single_voxel_normals_point_outward` |
| 2x2x2 solid | `mesh::marching_cubes` `test_full_density_field` (strengthened in #173 (ENG-F22-06); keep it green) |

**Out of scope:** lighting values at vertices; smoothing across chunk borders.

**Gate class:** domain (`moho_core` meshing).

**Risks:** normals change on all smooth terrain; check the look in game. #140 (ENG-F10-03) moves this file.

## Verification

In game: mine around a block until one smooth voxel stands alone; it shades like a rounded lump, with no flat-lit top-facing patches on its sides.

## Notes

Reproduced on `dev`: 8 of 24 vertices of a lone voxel had normal (0, 1, 0).
