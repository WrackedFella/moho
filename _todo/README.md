# Moho — Planning Index

Three project lines sharing one repo until the split
([ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md)). Card format, IDs and
lifecycle: [`_STANDARDS.md`](_STANDARDS.md). Architecture decisions:
[`docs/adr/`](../docs/adr/README.md).

| Line | Folder | Focus |
|---|---|---|
| Engine | [`engine/`](engine/) | Shared crates; hygiene and debt land opportunistically |
| Strategy/base-builder | [`strategy-game/`](strategy-game/) | **Primary** |
| FPS | [`fps-game/`](fps-game/) | GDD only until it defines real scope |

## engine/

| Feature | Status |
|---|---|
| [ENG-F1 Engine hygiene](engine/ENG-F1-engine-hygiene/_feature.md) | in progress, opportunistic |
| [ENG-F2 Dependency upgrades](engine/ENG-F2-dependency-upgrades/_feature.md) | in progress |
| [ENG-F3 Renderer pipeline cleanup](engine/ENG-F3-renderer-pipeline-cleanup/_feature.md) | not started |
| [ENG-F4 Terrain rendering debt](engine/ENG-F4-terrain-rendering-debt/_feature.md) | in progress |
| [ENG-F5 Physical repo split](engine/ENG-F5-physical-repo-split/_feature.md) | parked (v1 gate) |
| [ENG-F6 Headless, deterministic logic](engine/ENG-F6-headless-deterministic-logic/_feature.md) | proposed |

## strategy-game/

| Feature | Status |
|---|---|
| [SG-F1 Core interaction & resource loop](strategy-game/SG-F1-core-interaction-loop/_feature.md) | in progress (3/4) |
| [SG-F2 Known bugs](strategy-game/SG-F2-known-bugs/_feature.md) | not started, low priority |
| [SG-F3 Buildings & construction](strategy-game/SG-F3-buildings-and-construction/_feature.md) | parked |
| [SG-F4 Worker pawns](strategy-game/SG-F4-worker-pawns/_feature.md) | parked |

## fps-game/

| Feature | Status |
|---|---|
| [FPS-F1 Game design document](fps-game/FPS-F1-game-design-document/_feature.md) | not started |

## Shipped before this structure

Engine/game crate split: `moho_game` created, renderer decoupled behind
`moho_render_api`, `AudioEvent` unified, wgpu 27→29.0.4. Lasting decisions are
in ADRs 0001–0003; the rest is in git history.
