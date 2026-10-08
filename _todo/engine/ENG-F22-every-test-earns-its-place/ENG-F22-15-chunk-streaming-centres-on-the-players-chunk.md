# Chunk streaming centres on the player's chunk at negative coordinates

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#182](https://github.com/WrackedFella/moho/issues/182)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

Chunk streaming centres on the wrong chunk when the player stands at a negative x or z: between -15 and -1 it streams around chunk 0, and it disagrees with the LOD centre one chunk to the west or south. After this, streaming and LOD agree on the player's chunk everywhere.

## Deliverables

- `ChunkStreamer::update` computes the player's chunk with floor division, the same way the LOD code does.
- Streaming's per-frame budget and closest-first order are pinned.

## Acceptance criteria

```gherkin
Scenario Outline: Streaming centres on the chunk the player stands in
  Given a load radius of 0
  When the player is at x <x>, z <z>
  Then the only loaded column is <column>

  Examples:
    | x   | z   | column   |
    | -1  | -1  | (-1, -1) |
    | -17 | 5   | (-2, 0)  |
    | 15  | 16  | (0, 1)   |

Scenario: A frame loads only its budget, closest columns first
  Given a load radius of 1 and a budget of 1 column per frame
  When the player at the origin streams one frame
  Then exactly one column is loaded
  And it is the player's column
```

## Tech spec

**Design:** replace `floor() as i32 / 16` with `div_euclid` on the floored position, or reuse the LOD code's player-chunk helper so the two can't drift.

**Test map**
| Scenario | Test |
|---|---|
| Centres on the player's chunk | `app::chunk_streamer` `update_centers_on_euclidean_chunk_for_negative_positions` |
| Budget, closest first | `app::chunk_streamer` `update_loads_closest_columns_first_within_budget` |

**Out of scope:** eviction writing modified chunks and reloading them from disk (chunk files are cwd-relative; injecting a saves directory is a separate decision).

**Gate class:** glue (binary).

**Risks:** tests must use a world name with no chunk files under cwd `saves/` (`load_chunk` reads there first).

## Verification

In game: walk west across x = 0. Terrain ahead keeps loading with no gap one chunk wide near the origin.
