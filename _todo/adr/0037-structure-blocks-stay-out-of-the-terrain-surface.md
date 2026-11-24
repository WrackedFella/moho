# 0037 — Structure blocks stay out of the terrain surface

**Status:** Accepted (2026-11-24); amends [0028](0028-terrain-is-material-plus-density.md)

## Context

A block is terrain (material < 100, drawn as a smooth surface) or a structure (material ≥ 100,
drawn as a cube). The mesher chose, per chunk, whether structures counted as solid in the
terrain density field: an all-terrain chunk counted them, a chunk holding both left them out,
and a chunk with no terrain drew no terrain surface. A chunk's field reaches one block into its
neighbours and it owns the cells up to their first layer, so two chunks with different rules
drew different surfaces on their shared seam and left holes
([#850](https://github.com/WrackedFella/moho/issues/850)). 0028 makes the density field the
one surface that the mesher, the cut caps and the aim raycast read, and keeps placed blocks on
the cube path, but does not say whether a structure is part of that surface.

## Decision

1. **The terrain surface is built from terrain blocks only.** A structure block is air to the
   terrain density field wherever it sits, inside a chunk or in its padding. Under 0028 a
   structure's stored density (255) says it is solid; the terrain surface reads its sample as
   air.
2. **Every chunk meshes by the same rule:** the terrain surface from its padded terrain field,
   plus a closed cube for each structure block in the chunk. A chunk's content never selects
   the rule. The cut caps follow the same rule.
3. **Solidity is unchanged.** `is_solid_at`, light, sky exposure, contact shading, collision
   and the coarse LOD keep treating a structure as solid.
4. **Every reader of the surface follows rule 1.** The aim raycast adopts it when structures
   become aimable: a ray hits a structure's cube, not a terrain hull around it.

## Consequences

- Seams agree by construction; no pair of chunks can disagree about a block in their overlap.
- Terrain meets a structure at the structure's faces. Where they touch, a terrain face lies
  inside the structure's cube face (hidden), and at a cut the terrain cap and the structure's
  square overlap at the corners.
- A structure still shades the terrain beside it, and pawns still stand on it.
- `flagstone_game::raycast_surface` still treats structures as smooth surface until the first
  card that lets the player aim at a structure (SG-F3); that card owns rule 4.
- 0028's density card reads a structure's sample as air when it builds cell triangles, in all
  three readers.
