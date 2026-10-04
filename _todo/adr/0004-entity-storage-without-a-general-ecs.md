# 0004 — Entities live in typed, domain-owned stores until a consumer needs an ECS

**Status:** Accepted

## Context

`legion` 0.4.0 (last release February 2021) is used only as a typed entity
store. Every entity has exactly one component (`VoxelChunk`, `Sphere`, `Cube`,
light gizmos). There are no systems, schedules or `Resources`, and every query
reads a single component type. Chunk lookup by position scans every chunk.
The binary, `moho_game` (scene build and persistence) and `moho_renderer`
(`InstanceCollector`, `Scene`) pass `legion::World` around, so an engine crate's
public API names the ECS type.

legion is the source of two unmaintained-crate advisories (`instant`, `paste`)
and about eight duplicate crate versions (`parking_lot` 0.11, `uuid` 0.8,
`syn` 1, `bit-set` 0.5, `getrandom` 0.2, and others).

No current consumer needs an ECS. The likely future consumers are worker
pawns ([SG-F4](../strategy-game/SG-F4-worker-pawns/_feature.md): many entities sharing position, stats and jobs) and FPS
enemies and projectiles ([FPS-F1](../fps-game/FPS-F1-game-design-document/_feature.md) has no GDD yet).

Candidates, as of 2026-10:

| Option | Maintenance | Weight | Determinism | Migration |
|---|---|---|---|---|
| Fork legion | Us: 2021 archetype storage built on `unsafe` | Keeps the duplicates | Unchanged | None |
| `bevy_ecs` 0.19 | Active, a breaking release roughly every 3–4 months | 64 crates | Deterministic storage; system order must be pinned explicitly | Mechanical, plus scheduler adoption |
| `hecs` 0.11 | Active, small API, stable for years | 5 crates | Deterministic for the same sequence of operations | Mechanical, one-to-one |
| Typed stores | No dependency | None | Fully ours | Small; API improves |

## Decision

- Remove `legion`. Entities live in domain-owned typed stores. Chunks go in a
  map keyed by chunk position, owned with the voxel grid. Actors and gizmos go
  in collections owned by `moho_game` or the binary. Generational handles are
  added only where something holds a handle (light gizmos).
- Engine crates take domain-shaped data (iterators or slices of `Renderable`,
  `&mut VoxelChunk`), not a world type.
- No general-purpose ECS until a named feature needs one: many entity kinds
  that share components, or queries across them. When that happens, `hecs` is
  the default candidate. Choosing anything else needs a new ADR.
- Forking legion is rejected: we would own unsafe archetype code to keep a
  feature set we don't use. `bevy_ecs` is rejected for now because of its
  release churn and weight, which buy a scheduler we don't need.

## Consequences

- Clears the `instant` and `paste` advisory ignores and the duplicates legion
  brings in.
- `InstanceCollector::collect_from_world` and `Scene` lose their legion
  parameters. [ADR-0001](0001-render-api-boundary.md)'s renderer boundary gets tighter, not looser.
- Chunk lookup by position becomes a map lookup.
- Adopting `hecs` later is a mechanical change from typed stores, so this
  decision is reversible.
- The [ENG-F1](../engine/ENG-F1-engine-hygiene/_feature.md) note about `EguiAdapter` not fitting a `Sync` ECS resource slot
  no longer constrains anything.
