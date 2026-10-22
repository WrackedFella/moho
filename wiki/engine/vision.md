# Engine — vision and scope

**Source:** [ADR-0005](../../_todo/adr/0005-crate-lines-and-dependency-direction.md), [ADR-0007](../../_todo/adr/0007-third-party-licence-policy.md), [ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md), [ADR-0011](../../_todo/adr/0011-simulation-stays-network-ready.md), [ADR-0012](../../_todo/adr/0012-engine-crate-map-for-m2.md), [roadmap](roadmap.md).

What the shared engine is for and what it refuses to be. The engine has no game design document;
its consumers are the two game lines. Order of work is the [roadmap](roadmap.md); how the crates fit is
[architecture/overview](../architecture/overview.md); the reasons for each boundary are the ADRs.
This page holds what stays true across features.

## 1 Purpose and consumers

One engine, two games: the strategy base-builder ([flagstone](../flagstone/vision.md)) and the FPS
([searchlight](../searchlight/game-design-document.md)). The engine supplies windowing and the fixed-tick loop,
rendering, physics, audio, input, persistence and UI hosting. The games supply rules, content and the
meaning of a frame.

## 2 Placement test

- "Would a different game reuse this as-is?" Yes: engine. Replaced per game: that game's line.
- A game extends the engine in its own crates and never forks engine code. If the engine must behave
  differently per game, add a seam (data, registration or a callback), not a copy.
- Dependencies point game to engine, never the reverse, and a line never depends on the other line.
  `just check` enforces both ([ADR-0005](../../_todo/adr/0005-crate-lines-and-dependency-direction.md)).
- Build nothing without a named consumer, except graphics and physics, which are engine goals in their own right.

## 3 Non-goals

- A general-purpose engine or editor for third parties. Editor tooling is M5+ and pulled by a consumer.
- A general ECS ([ADR-0004](../../_todo/adr/0004-entity-storage-without-a-general-ecs.md)).
- Game rules, content or terrain: voxels belong to the strategy line ([ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md)).
- Competitive multiplayer. Multiplayer means co-op with a trusted host, and may never ship.

## 4 Constraints

| Constraint | Source |
|---|---|
| Third-party licences allow closed-source commercial sale; no GPL, LGPL or AGPL | [ADR-0007](../../_todo/adr/0007-third-party-licence-policy.md) |
| Simulation is a fixed 60 Hz tick driven by commands; state is plain data with stable ids; presentation only reads | [ADR-0009](../../_todo/adr/0009-simulation-time-is-one-fixed-tick.md), [ADR-0011](../../_todo/adr/0011-simulation-stays-network-ready.md) |
| Lowest target GPU is a GTX 1080: a floor, never a filter. Higher tiers are optional, not dropped | [roadmap](roadmap.md) |
| wgpu/egui and rapier/glam upgrade once per milestone close | [roadmap](roadmap.md) |
| End state is three repos (engine, strategy, FPS) joined by a superproject; they split when the ENG-F5 gate holds, not on a date | [roadmap](roadmap.md) |

## 5 Quality targets

Perf criteria are measured on the lowest target GPU at 1080p. Each renderer feature states its budget in its
own card (for example, global illumination at 2 ms). This page records the rule, not the numbers.
