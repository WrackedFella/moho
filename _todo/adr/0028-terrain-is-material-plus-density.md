# 0028 — Terrain is one material and one density per block; water is a per-block fluid layer

**Status:** Accepted (2026-10-27)

## Context

[SG-F6](https://github.com/WrackedFella/moho/issues/707) needs slopes that are the
generator's surface, not a staircase, and the vision now promises gradual mining: a block
wears away instead of vanishing whole. Later features want still water to become flowing
water, and load, cave-ins and sapping to read the terrain. The
[world generation research](../../wiki/flagstone/worldgen-research.md) §2 compares the options.

Today a block in `flagstone_voxel` is air or one material id with an optional resource id.
Every consumer reads that binary fact: light, sky exposure, the coarse LOD, the cube
raycast, the start-area checks and `is_solid_at` (`material_at(pos).is_some()`). Three
consumers draw or hit the smooth surface: the hybrid mesher, which builds a 0/1 density
field and interpolates it at 0.5; `raycast_surface`, which mining aims with; and the
underground view's cut caps. The last two rebuild the surface from 0/1 corner patterns
with every vertex at an edge midpoint (`MarchingCubes::cell_triangles`), so they only
match the drawn mesh while the field is 0/1.

## Decision

1. **Every block stores a density** as a `u8` beside its material. The field value is
   `density / 255`, the surface is its 0.5 isosurface, and a block is solid when
   `density >= 128`, the mesher's existing "denser than 0.5" rule.
2. **Material and density agree.** A block has a solid material exactly when it is solid.
   An air block may carry a density below 128 (the part of a surface sample above the
   surface). An all-air chunk with no density is never stored, as today. So `is_solid_at`,
   `material_at` and every binary consumer keep their meaning and their callers.
3. **The stored density is the generator's field, scaled to be exact.** Near the surface the
   field changes by about one unit per block and is clamped no closer than one block either
   side of it, so two adjacent samples across the surface are never clamped and the
   interpolated crossing is the generator's crossing. A narrower band bends the drawn
   surface by up to about 0.09 block.
4. **One surface, three readers.** The mesher, the aiming raycast and the cut caps build
   cell triangles from the same stored corner densities with the same interpolation. No
   reader keeps a 0/1 shortcut once densities are fractional.
5. **Placed blocks are density 255** and keep the cube path (material ≥ 100). Mining's
   destruction step sets the hit block to air with density 0. `Pawn::mine` and
   `ResourceYield { resource_id, amount }` do not change; the destruction step is the only
   code that knows a block's granularity, and gradual wear later changes only it
   (subtract work, remove at the threshold).
6. **Nothing about a block lives only in the mesh.** Material, density, resource and fluid
   are grid data, saved in the chunk file. Meshes, colliders and caps are derived and never
   read back as truth, so a support or cave-in rule can run on the grid alone.
7. **Water is a per-block fluid layer in the same grid**, not a drawn plane and not a solid
   material: a fluid kind per block, absent from chunks that hold none, occupying only
   non-solid blocks. A water block keeps its terrain density, so the shore keeps its slope
   and draining water later reveals the real ground. Fluid blocks are never solid; whether a
   pawn can enter one is a movement and navigation rule over this layer. Still water needs
   only the kind; flow adds a level per fluid block when it is designed.
8. **Saves.** The chunk file version is bumped for density and the fluid layer; every
   persisted block record carries density. Under
   [ADR-0006](0006-save-format-contract.md) pre-v1 worlds are not migrated: the old chunk
   file is rejected and the streamer regenerates the chunk.

## Consequences

- Smooth slopes cost one byte per block (4 KiB per stored chunk) plus the fluid layer where
  water is. Chunks just above a surface that holds only sub-threshold air densities are now
  stored.
- `raycast_surface` and the cut caps change with the first density card, not after it; a
  card that stores fractional density without them would aim mining up to half a block off
  the drawn surface.
- Light, sky exposure, the coarse LOD and the cube raycast need no change: they ask "is
  this block solid", whose answer is unchanged.
- Gradual mining, cave-ins and sapping need no new per-block storage; flowing water adds one
  field to the fluid layer. Each is its own feature and decision.
- Density is an integer, so wear and generation write the same bytes on every platform,
  which [ADR-0011](0011-simulation-stays-network-ready.md) and co-op replay need.
- Sub-grid deformation (a block as a grid of sub-blocks) is not planned; reconsidering it
  needs a new ADR.
- Existing pre-v1 worlds lose their player edits to chunks when this lands; new worlds are
  the supported path, as for every save change before v1.
