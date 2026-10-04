# 0005 — Every crate belongs to one line; dependencies point from game to engine

**Status:** Accepted

## Context

The end state is three repos: engine, strategy and FPS ([ENG-F5](../engine/ENG-F5-physical-repo-split/_feature.md)). Until the
split, only convention keeps the lines apart. [ADR-0001](0001-render-api-boundary.md) forbids
renderer/audio/core → `moho_game`, but nothing checks it. The 2026-10 graph
already has two drifts. `moho_sim`, listed as engine in [ENG-F5](../engine/ENG-F5-physical-repo-split/_feature.md), depends on
`moho_game`. And `moho_core` declares an unused `winit`, which drags a
platform crate into every domain crate (`moho_game`, `moho_sim`,
`moho_physics`, `moho_audio`).

`moho_sim` is not the deterministic integer simulation its description claims.
Its `Simulation`/`PlayerInput`/`input_map` API has no production caller. The
part in use, `SimulationController`, is a wrapper over `moho_game`'s
`PlayerController` and `GameClock`, using `f32` math.

## Decision

Each crate belongs to exactly one line:

| Line | Crates |
|---|---|
| engine | `moho_core`, `moho_render_api`, `moho_renderer`, `moho_audio`, `moho_physics`, `moho_input`, `moho_types` |
| strategy | `moho_game` (absorbs `moho_sim`), `moho_ui`, the `moho` binary |
| fps | none yet; an FPS crate joins this line when [FPS-F1](../fps-game/FPS-F1-game-design-document/_feature.md) produces scope |

Rules, checked by `just check` over normal and build edges:

1. Engine crates never depend on a game-line crate. Dev-dependencies are
   allowed for integration tests, as in [ADR-0001](0001-render-api-boundary.md).
2. A game line never depends on another game line.
3. Domain crates (`moho_core`, `moho_game`) never reach `winit`, `wgpu` or
   `egui`.

A new crate needs a deployable, reuse or compile-time boundary. Its pull
request adds a row to this table.

## Consequences

- `moho_sim` is merged into `moho_game`. Its unused toy API and the tests that
  only exercise it are deleted. `CLAUDE.md`'s architecture table and
  domain-logic paths drop `moho_sim`.
- `moho_core` drops its unused `winit` dependency. Domain crates then build
  without a windowing stack.
- `moho_ui` stays in the strategy line because it depends on `moho_game`
  (HUD). When the FPS needs menus, settings or the console, the generic part
  moves to an engine crate. It is not split before that consumer exists.
- Engine-line crates still carry strategy vocabulary: `voxel/` and
  `MaterialType` ([ADR-0002](0002-voxel-and-materials-stay-in-core.md)), and voxel `WorldEvent`s ([ADR-0003](0003-core-owns-event-types.md)). This table
  doesn't move them. Their placement is decided as [ENG-F5](../engine/ENG-F5-physical-repo-split/_feature.md) preparation, and the
  check keeps the lines from coupling further in the meantime.
- At the physical split, the table is the `filter-repo` path list.
