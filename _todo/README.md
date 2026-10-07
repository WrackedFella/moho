# Moho — Planning Index

Three project lines sharing one repo until the split
([ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md)). Card format, IDs and
lifecycle: [`_STANDARDS.md`](_STANDARDS.md). Item state (Status, Priority, Agent-eligible)
is not tracked in this repo; it lives on the
[project board](https://github.com/users/WrackedFella/projects/1). Architecture decisions:
[`adr/`](adr/README.md). Order of work: [`ROADMAP.md`](ROADMAP.md).

The Foundation gate ([ENG-F8](engine/ENG-F8-foundation-gate/_feature.md)) holds; feature work is open. Milestones
and their gates are in [`ROADMAP.md`](ROADMAP.md).

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

| Feature |
|---|
| [ENG-F1 Engine hygiene](engine/ENG-F1-engine-hygiene/_feature.md) |
| [ENG-F2 The dependency tree is current, clean and carries only chosen crates](engine/ENG-F2-dependency-upgrades/_feature.md) |
| [ENG-F3 Renderer pipeline cleanup](engine/ENG-F3-renderer-pipeline-cleanup/_feature.md) |
| [ENG-F4 Terrain rendering debt](engine/ENG-F4-terrain-rendering-debt/_feature.md) |
| [ENG-F5 Engine and games live in separate repos](engine/ENG-F5-physical-repo-split/_feature.md) |
| [ENG-F6 Headless, deterministic logic](engine/ENG-F6-headless-deterministic-logic/_feature.md) |
| [ENG-F7 Entity storage is maintained and legion-free](engine/ENG-F7-maintained-ecs/_feature.md) |
| [ENG-F8 Foundation gate](engine/ENG-F8-foundation-gate/_feature.md) |
| [ENG-F9 Domain tests catch behaviour changes](engine/ENG-F9-tests-prove-behaviour/_feature.md) |
| [ENG-F10 World geometry from any source](engine/ENG-F10-world-geometry-from-any-source/_feature.md) |
| [ENG-F11 A game runs on the engine without copying the app loop](engine/ENG-F11-shared-app-loop/_feature.md) |
| [ENG-F12 Input is game-defined actions bound as data](engine/ENG-F12-input-actions/_feature.md) |
| [ENG-F13 Every dependency has a reasoned verdict](engine/ENG-F13-dependency-audit/_feature.md) |
| [ENG-F14 A game loads a static scene from a file](engine/ENG-F14-scene-import/_feature.md) |
| [ENG-F15 Games query physics and run many bodies](engine/ENG-F15-physics-queries-and-bodies/_feature.md) |
| [ENG-F16 Sounds play from positions](engine/ENG-F16-positional-audio/_feature.md) |
| [ENG-F17 Agents path over any static level](engine/ENG-F17-navigation/_feature.md) |
| [ENG-F18 Both games share the UI shell](engine/ENG-F18-shared-ui-shell/_feature.md) |
| [ENG-F19 Games load definitions and content from layered roots](engine/ENG-F19-data-and-mod-content/_feature.md) |
| [ENG-F20 A wgpu or egui upgrade touches one crate](engine/ENG-F20-graphics-upgrade-touches-one-crate/_feature.md) (proposed) |

## strategy-game/

| Feature |
|---|
| [SG-F1 Core interaction & resource loop](strategy-game/SG-F1-core-interaction-loop/_feature.md) |
| [SG-F2 Known bugs](strategy-game/SG-F2-known-bugs/_feature.md) |
| [SG-F3 Buildings & construction](strategy-game/SG-F3-buildings-and-construction/_feature.md) |
| [SG-F4 Worker pawns](strategy-game/SG-F4-worker-pawns/_feature.md) |

## fps-game/

| Feature |
|---|
| [FPS-F1 Game design document](fps-game/FPS-F1-game-design-document/_feature.md) |

## Shipped before this structure

Engine/game crate split: `moho_game` created, renderer decoupled behind
`moho_render_api`, `AudioEvent` unified, wgpu 27→29.0.4. Lasting decisions are
in ADRs 0001–0003; the rest is in git history.
