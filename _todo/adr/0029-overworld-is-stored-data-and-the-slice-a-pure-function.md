# 0029 — The overworld is stored data; a slice is a pure function of it

**Status:** Proposed (2026-10-27)

## Context

[SG-F6](https://github.com/WrackedFella/moho/issues/707) gives each world a large map with
seas, rivers that reach them, mountains and biomes; the player picks a rectangle of map tiles
and plays there. Later features read and write the same map (history and factions, scanning a
slice before picking it), grow the colony into neighbouring slices, and expose the generation
parameters as sliders on the new-world screen. The
[world generation research](../../wiki/flagstone/worldgen-research.md) §3 explains why
believable rivers need a global pass and cannot come from per-column noise.

Today a world is a square of 64 to 256 blocks centred on the origin. Chunks are generated on
demand by a pure function of a `TerrainConfig` and the chunk position, and only player-edited
chunks are saved. The world save keeps the `WorldSpec` (name, `u64` seed, size, day length);
`TerrainConfig` is rebuilt from it at load, with its biomes and ore layout taken from code
defaults and the seed cut to `u32`. Changing a default or the generator therefore changes the
unedited terrain of every existing world, next to its saved edited chunks.

## Decision

1. **Three levels: tile, slice, chunk.** The overworld is a grid of tiles, each a fixed
   square of blocks. A slice is a rectangle of tiles and is the unit of picking and of later
   expansion. Tile size, map size and slice size are parameters, not part of this decision.
2. **One world-level parameter set.** Every value the overworld and slice generators read
   that a player could reasonably tune is a field of one plain-data parameter set: the full
   `u64` seed, sizes, sea level, climate, cave, ore and tree settings. It is created at
   new-world time from defaults (later from the new-world screen) and saved in the world
   file. Generators read no other tunable, so a world keeps its terrain when a default
   changes.
3. **The overworld is generated once and stored.** At new-world time a pass over the whole
   tile grid produces one record per tile: height, water (sea, lake, river discharge and
   downstream direction), climate, biome and mineral richness keyed by resource id. The
   records are saved in their own strategy file kind beside the world file, written when the
   overworld changes rather than on every autosave. The tile record is the one place later
   map data (owner, site, ruin) is added, by a version bump.
4. **A slice is a pure function of (parameters, overworld, absolute coordinates).** Every
   block's material, density, resource and fluid ([ADR-0028](0028-terrain-is-material-plus-density.md))
   comes from that function, chunk by chunk, as streaming does today. Large-scale shape comes
   only from tile records. Randomness in slice generation is hashed from absolute position and
   the seed; no stateful generator is carried between chunks, and nothing depends on where
   the slice starts or on the order chunks are generated.
5. **Absolute coordinates at the generator, a local frame in the simulation.** The
   generators take absolute map block coordinates. The simulation keeps a slice-local frame,
   offset from absolute by a whole number of tiles that is saved with the world, and converts
   only at the generator call. Expansion keeps the offset; new tiles get local coordinates
   outside the first slice.
6. **The slice edge is hard,** as today: no blocks are generated outside the picked tiles.
   Tiles beyond the map's edge read as deep sea, so sub-tile detail near the edge is defined.
7. **Generation is bit-identical on every platform.** Generators use integer arithmetic,
   the correctly rounded float operations (`+ − × ÷ sqrt`) and in-house noise; no `std`
   transcendental functions (`sin`, `cos`, `exp`, `powf`, `ln`), whose results vary by
   platform. A dependency may replace in-house noise only if it meets the same rule and
   [ADR-0007](0007-third-party-licence-policy.md). Sequential overworld passes that need a
   random stream use `moho_core::rng::SimRng`; queue-ordered passes break ties by tile index.
8. **The world records its generator version.** A change that alters generated output for
   the same inputs bumps it; pinned known-output tests (a chunk hash and a small overworld
   hash, run on all three OSes) make such a change fail loudly. Before v1 a build rejects a
   world whose generator version it does not have, as [ADR-0006](0006-save-format-contract.md)
   does for save versions.

## Consequences

- What the map shows is what the slice gets, and two slices generated separately agree
  block for block along a shared edge, so expansion needs no stitching.
- History, scouting and expansion read or extend tile records; nothing in this feature has
  to anticipate their fields beyond the record being extensible.
- The new-world screen's sliders are a UI over the parameter set; adding a tunable is a
  parameter field plus a world-file version bump.
- The world file version is bumped for the parameter set, slice and offset, and the new
  overworld file kind gets its own pinned-bytes test under
  [ADR-0016](0016-game-lines-own-their-save-kinds.md). Pre-v1 worlds are not migrated.
- Generator work before v1 invalidates existing worlds whenever it changes output. From v1
  a changed generator either keeps the old version runnable or bakes a world's generated
  chunks to disk before upgrading; that choice is made at v1, not now.
- Overworld generation runs on the existing world-generation thread at new-world time. Its
  budget at the default map size is a card concern; hydraulic erosion and plate tectonics
  are excluded by that budget, not by this decision.
- The overworld map reaches the screen as a game-made image through the UI shell's paint
  data, which already carries egui textures, so it needs no engine request.
- Simulation positions stay small, so f32 precision in rendering and physics is unchanged
  however large the map is.
