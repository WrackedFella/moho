# SG-F6 — I pick my colony's ground on a believable world map

**Status:** Draft
**Labels:** feature, line:strategy
**Note:** Scope proposed 2026-10-26 from Justin's direction (smooth believable terrain, caves, ore
seams, an overworld to pick a slice from). Research and the why: [worldgen research](../../../wiki/flagstone/worldgen-research.md).
Awaiting Justin's approval of scope and exit criteria before the issue is filed.

## End state

A new world is a large map with coasts, mountains, rivers, lakes and biomes. The player picks a slice
of it and plays on terrain that looks like the map promised: smooth slopes, caves under the surface,
ore in seams worth following. Later the same map carries the world's history and factions, the player
reads a slice's riches and dangers before choosing it, and the colony grows into neighbouring slices.
See the [vision](../../../wiki/flagstone/vision.md) and [GDD §6](../../../wiki/flagstone/gdd.md#6-world-and-persistence).

## Summary

The player starts a world, sees its map, picks where to settle, and gets a slice whose surface,
caves and ore are believable and consistent with the map. Mining works as it does today. This
replaces the current square of faceted noise with no water, no caves and scattered ore.
**Moves toward the end state by:** the world is a place the colony loop happens in, and the map is
the stage history, scouting and expansion will be built on.

## Exit criteria

Observable rules first; look and feel under Verification.

```gherkin
Scenario: The map has the shapes of a world
  Given a new world from a seed
  When its map is generated
  Then it contains sea, land, at least one mountain range, at least one river and at least one lake

Scenario: Every river reaches water
  Given a generated map
  When I follow any river downstream
  Then each step is level or downhill
  And the river ends in a lake or the sea

Scenario: The slice is what the map showed
  Given a generated map
  When I pick a tile region and start
  Then every tile of the slice has the biome, water and relief the map showed for it

Scenario: The same choice gives the same slice
  Given a seed and a chosen tile region
  When the world is generated twice
  Then the two slices are identical, block for block

Scenario: Neighbouring slices meet
  Given two slices that share an edge, generated separately
  When I compare the columns along that edge
  Then they are identical, so a later expansion needs no stitching

Scenario: Slopes are continuous
  Given a slice
  When I compare the drawn surface with the generator's surface at any column
  Then they differ by less than a tenth of a block

Scenario: Caves lie under the surface and open onto it
  Given a slice
  When I look below the surface
  Then there are caves, none within the top layers except where one opens to the surface
  And at least one cave has a surface entrance

Scenario: Ore lies in seams
  Given a slice
  When I look at where ore blocks are
  Then every ore block belongs to a connected body of at least ten ore blocks
  And no ore block stands alone

Scenario: Ore is findable from the start
  Given a new slice and its start area
  When I search outward from the start
  Then an ore seam is reachable within a distance the design sets

Scenario: Mining still yields
  Given a possessed pawn with a tool
  When it mines a surface block, a cave wall block and an ore block
  Then each block is gone
  And the ore block yields one unit of its resource, the others yield nothing

Scenario: Still water is impassable
  Given a slice with a lake or river
  When a pawn walks into the water
  Then it is stopped at the water's edge

Scenario: The start area is safe ground
  Given a new slice
  When the pawn spawns
  Then it stands on dry, nearly level ground with no cave beneath it
  And it is not inside terrain

Scenario: Trees grow where the biome says
  Given a slice with a forested biome and a bare one
  When both are generated
  Then the forested area holds trees and the bare one holds none
  And a tree block mines like any other block
```

Verification (cannot be asserted): gentle slopes show no terraces or facets; cave walls are smooth;
the map reads as a globe or projection at a glance; a river in the slice looks like a river, not a
corridor; ore shows on cave walls and cliff faces.

## Scope

- In: smooth terrain from continuous density; the overworld map (sea, land, mountains, rivers,
  lakes, biomes, mineral richness per tile); picking a tile region as the slice; deterministic
  slice generation from the map; caves of two kinds (caverns and tunnels) with surface entrances;
  ore in seams; at most one new ore beyond iron and crystal; still water; voxel trees; a found,
  unflattened start area; mining unchanged in contract.
- Out: history and factions (the map keeps room for them, nothing is written); scanning a slice
  before picking it; expanding into or buying neighbouring slices; flowing water, swimming, boats;
  gradual mining (density wear); structures, ruins, roads; weather and seasons; hydraulic erosion of
  the map; showing neighbouring terrain beyond the slice edge; new biomes beyond what the climate
  table yields.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Where does smoothness come from? | Each block stores a density (0..1) and a material; the surface is the 0.5 isosurface | Only representation where the surface is the generator's surface; one byte per block. Smoothing a binary grid in the mesher is cheaper but blurs a staircase and never gives gradual mining |
| What happens to "sub-grid deformation" in the vision? | Replaced by density wear: a block erodes by losing density | Same promise, no second grid. Needs Justin's OK as a vision edit |
| Where does large-scale shape come from? | A map generated once per world, saved with it, the only source of shape; the slice is a pure function of seed, map and absolute coordinates | Rivers that flow downhill need a global pass; stored data is what history and scouting read later. Stacked noise alone cannot make a river reach the sea |
| What is a slice? | A rectangle of map tiles; the tile is the unit of picking and of later expansion | Same mechanism as RimWorld's map-size choice; no stitching later. Fixed world squares would need a second generator for expansion |
| How are caves carved? | Noise terms in the terrain's density function, chunk-local | Agent or cellular carvers need a whole-slice pass that conflicts with streaming |
| Whole-block ore or seams? | Whole-block yield stays; placement becomes seams scaled by the tile's richness | The two were conflated; only placement was the problem. Spread seams of partial blocks would change the yield contract for no visible gain |
| Is water in scope? | Still and impassable | Flow and swimming are a system of their own; a dry river bed is not believable |
| Trees? | Voxel structures that mine like blocks | Models need a new entity kind, harvesting and collision |
| Is the start area flattened? | Found, not flattened; caves suppressed under it | A flattened plateau is the one thing that looks generated |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| History and factions written onto the map | Needs the map first; the tile record is the hook | After this feature and a setting decision |
| Scanning a slice before picking it (richness, hostility) | Reads tile data this feature produces; UI only | When the colony loop makes the choice matter |
| Expanding into or buying neighbouring slices | Guaranteed possible by the edge rule; needs ownership and streaming across slices | After SG-F3 and SG-F4 |
| Gradual mining by density wear | The destruction step is the only place it touches | With automation, when mining speed is a progression lever |
| Flowing water, swimming, boats | A system of its own | When water is a strategy roadmap item |
| Hydraulic erosion on the map; plate tectonics | Prettier mountains for seconds of generation time | If maps read as bland |
| Worm-carved cave systems | Noise caves first; worms if they read as random | After playtest |
| Neighbouring terrain shown past the slice edge | Cosmetic | With expansion |
| Structures, ruins, roads | Setting and history | With history |

## Items

| Item |
|---|

Candidate increments (not cards; STRAT-M4 is not the open milestone): density blocks; caves and ore
seams; the map and slice picking; still water and trees. Each is observable alone and none depends
on a later one.

## Notes

- Mining contract: `Pawn::mine` and `ResourceYield { resource_id, amount }` keep their shape; only
  the destruction step learns about density.
- No slice-local randomness and no slice-centred coordinates anywhere in the slice generator; this
  is what makes neighbouring slices meet.
- Ore sits under the top layers on open ground (finding from the former SG-F1-02); seams reaching
  cave walls and cliffs are how it becomes findable.
- Open questions for the Tech Lead:
  - TL: cost of per-block density: chunk storage, chunk file version (migrate or regenerate pre-v1
    saves under the save contract), the solid threshold for light, navigation, raycast and the
    coarse LOD; whether the cut caps for the underground view need a change.
  - TL: a 3D noise source: extend the in-house 2D noise or adopt a crate under the dependency policy.
  - TL: map generation budget at the default size and where it runs (the generation job exists).
  - TL: still water: solid for physics and blocked for navigation, or a surface collider; whether an
    opaque surface does or translucency needs an engine request.
  - TL: the slice edge: keep today's hard boundary, and how the slice origin maps to absolute map
    coordinates so the edge rule holds.
  - TL: showing the map in the new-world screen: can the UI shell draw a generated image, or is
    that an engine request.
  - TL: ADRs: terrain is density plus material; the map is stored data and the slice a pure
    function of it.
