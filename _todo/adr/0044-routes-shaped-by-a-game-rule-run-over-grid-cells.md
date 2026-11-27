# 0044 — Seam 11 answers routes shaped by a game's area rule, over a gridded navmesh

**Status:** Accepted on merge of #1091 (extends [0019](0019-navigation-is-an-engine-seam.md))

## Context

FPS-F11-06 [#1057](https://github.com/WrackedFella/moho/issues/1057) sends a flanker on a route no player can see (1 now, up to 4 in co-op).
ENG-F42 [#638](https://github.com/WrackedFella/moho/issues/638) settled that the game decides what "seen" means and the engine never sees
threats. Spike ENG-F42-01 [#1070](https://github.com/WrackedFella/moho/issues/1070) compared how the engine asks: per Recast polygon (Detour
area flags and costs), per A* step with samples (Detour `getCost`), on a navmesh cut into
grid cells, and with a precomputed visibility table. Recast polygons reach 158 m² in the arena
and 431 m² on the proving ground. Per-polygon answers either crossed seen area (up to 5.4 m)
or ruled out whole halls and found no route. Per-step sampling crossed it after the funnel
straightened the route. Cells never let a route enter area the rule ruled out.

## Decision

- Seam 11 gains a cost-shaped route query beside `NavMesh::path`. It runs over the navmesh cut
  by the world grid lines x = k·g and z = k·g, where the game chooses the cell size g.
- The game's rule judges one cell at a time (its corners and centre) and answers costly (a
  multiplier of at least 1) or not walkable. The engine asks at most once per cell per query,
  and only for cells the search reaches.
- The route stays inside the cells the search chose. It never enters a not-walkable cell. If
  either end is in one, or no walkable corridor joins them, the answer is no path, never a
  partial route.
- The rule never receives threats, eyes or rays from the engine. Precomputed visibility, if a
  game wants it, sits behind its rule.
- The gridded mesh is plain data built deterministically from a `NavMesh`: cut points come from
  each shared edge's endpoints in a fixed order, using arithmetic and `floor` only ([0043](0043-simulation-transcendentals-come-from-one-libm-source.md)).

## Consequences

- The cell type the rule reads and the query's signature are a public API across the engine
  boundary, part of ENG-F5's "no engine API change" check.
- Fidelity is bounded by g. A seen strip narrower than the gap between a cell's samples can
  pass; a coarser g finds fewer hidden routes. The FPS line starts at 1 m.
- The route can touch the edge of ruled-out area but never enters it. A game whose body must
  stay clear pads its rule.
- Cost grows with the cells the search reaches. The worst case on the proving ground (9,445
  cells at 1 m, four threats, goal unreachable) is about 25 ms on the hardware floor, most of it
  the game's rays. Arena-sized levels are about 6 ms. Large maps (FPS-M3) need a budget or a
  hierarchy, decided then.
- The route is a snapshot of the rule when asked. Threats moving during the route, time-in-view
  tolerances and doors that open later are the game's to handle by asking again.
- Strategy can reuse the query if it routes on a navmesh. Its voxel routing stays its own
  ([0019](0019-navigation-is-an-engine-seam.md)).
