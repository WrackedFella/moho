# 0019 — Navigation over static levels is engine seam 11

**Status:** Accepted (2026-10-21; extends [0012](0012-engine-crate-map-for-m2.md))

## Context

[ADR-0012](0012-engine-crate-map-for-m2.md) lists the engine's public seams and makes adding
one an ADR-level change; [ADR-0017](0017-ui-shell-and-debug-values-are-engine-seams.md) added
seams 9 and 10. ENG-F17 ([#229](https://github.com/WrackedFella/moho/issues/229)) gives the FPS
line's enemies routes around walls (#454, #455). Any game with static levels needs the same
query, so it is engine work. The dependency audit on #229 found no maintained Rust path query
and adopted `rerecast` (a Recast port) for generation only. World geometry reaches the engine
as meshes ([ADR-0010](0010-world-geometry-is-a-mesh-contract.md)), and the simulation must
stay deterministic over plain data ([ADR-0011](0011-simulation-stays-network-ready.md)).

## Decision

- Seam 11, **navigation** (`moho_level::nav`): builds a navmesh from world meshes for one
  agent profile (radius, height, climb, slope), and answers a path query between two points
  with the route's corners, or none when either end is off the mesh or no route joins them.
- The navmesh is plain data, and building and querying are deterministic: the same meshes,
  profile and points give the same corners.
- The query (point snapping, A* over polygons, funnel) is the engine's own code. No
  third-party navmesh type crosses the seam, so `rerecast` can be forked or replaced without
  an API change.
- It is a module of `moho_level`, not a crate (ADR-0012, modules first): it takes
  `WorldMesh`es, not a `Level`, so any mesh source can feed it.

## Consequences

- ENG-F5's "no engine API change" check includes seam 11.
- Another agent size is another `NavMesh`; one navmesh never serves several profiles.
- Voxel routing stays on the strategy line (#437, #438). A shared A* waits on the roadmap's
  Out-of-M2 row.
- Float results can differ across platforms until `rerecast`'s `libm` feature is turned on;
  netcode work turns it on.
- `rerecast` is built at `opt-level = 3` in debug profiles, so arena-backed tests stay fast.
