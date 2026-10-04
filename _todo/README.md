# Moho — Planning Index

Three project lines sharing one repo until the split
([ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md)). Card format, IDs and
lifecycle: [`_STANDARDS.md`](_STANDARDS.md). Architecture decisions:
[`docs/adr/`](adr/README.md). Order of work: [`ROADMAP.md`](ROADMAP.md).

**Feature work is paused** until the agentic workflow's first draft lands; then
all features get a planning review before any agent picks up work.

**Planning review checklist item: commercial licensing.** Decided 2026-10-02 in
[ADR-0007](adr/0007-third-party-licence-policy.md); the text below is the original prompt. Before the review closes,
decide how the shipped product will be licensed and sold, then audit the dependency
tree (`cargo deny list`, plus non-crate assets: fonts, audio, textures, shaders) for
terms that conflict. `deny.toml` already rejects GPL/LGPL and allows MPL-2.0 (file-level
copyleft), but it checks only crate licenses, not attribution duties or assets. Repeat
the audit at the v1 gate ([ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md)).

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
| [ENG-F7 Entity storage is maintained and legion-free](engine/ENG-F7-maintained-ecs/_feature.md) | approved (gate) |
| [ENG-F8 Foundation gate](engine/ENG-F8-foundation-gate/_feature.md) | approved — **current focus** |
| [ENG-F9 Domain tests catch behaviour changes](engine/ENG-F9-tests-prove-behaviour/_feature.md) | proposed (early) |

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
