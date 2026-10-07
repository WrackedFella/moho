# Moho — Roadmap

The order in which features are tackled, and why, as milestones with gates. [`README.md`](README.md)
indexes what exists; this file sequences it. Item status is not tracked here; it lives on the
[project board](https://github.com/users/WrackedFella/projects/1). Update this file when a milestone gate closes, an item moves between phases, or a decision is made.

**Now:** M0 is done. M1 is open: the only milestone planned to card level
(rolling wave, [`_STANDARDS.md`](_STANDARDS.md#milestones)).

## M0 — Foundation (done 2026-10-05)

**Intent:** Feature work can start safely.
**Gate:** [ENG-F8](engine/ENG-F8-foundation-gate/_feature.md) criteria (verified).

Exit: all four [ENG-F8](engine/ENG-F8-foundation-gate/_feature.md) criteria verified. Order matters: the crate table comes
first, so later moves land in the right crates. [ENG-F7-01](engine/ENG-F7-maintained-ecs/ENG-F7-01-typed-entity-stores.md) has no dependency
and can run alongside 0.2–0.3.

| # | Item | Criterion | Depends on |
|---|---|---|---|
| 0.1 | Accept [ADR-0005](adr/0005-crate-lines-and-dependency-direction.md) (crate lines) and [ENG-F8](engine/ENG-F8-foundation-gate/_feature.md) | G2 | — |
| 0.2 | [ENG-F8-01](engine/ENG-F8-foundation-gate/ENG-F8-01-moho-sim-merged-into-game.md): `moho_sim` merged into `moho_game` | G2 | 0.1 |
| 0.3 | [ENG-F8-02](engine/ENG-F8-foundation-gate/ENG-F8-02-layering-check.md): layering check in `just check`; `moho_core` drops `winit` | G2 | 0.2 |
| 0.4 | [ENG-F7-01](engine/ENG-F7-maintained-ecs/ENG-F7-01-typed-entity-stores.md): typed chunk and actor stores (domain: tests reviewed first) | G1 | 0.1 |
| 0.5 | [ENG-F7-02](engine/ENG-F7-maintained-ecs/ENG-F7-02-legion-removed.md): legion removed; deny ignores name their owning cards | G1, G3 | 0.4 |
| 0.6 | [ENG-F2-01](engine/ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md) rewritten to [ADR-0006](adr/0006-save-format-contract.md) (decision only) | G4 | — |


## M1 — Two consumers can start

**Intent:** Both games build on the engine without forking it.
**Gate (all must hold):**
- [ENG-F10](engine/ENG-F10-world-geometry-from-any-source/_feature.md), [ENG-F11](engine/ENG-F11-shared-app-loop/_feature.md) and [ENG-F12](engine/ENG-F12-input-actions/_feature.md) done.
- [ENG-F13](engine/ENG-F13-dependency-audit/_feature.md) verdicts recorded.
- [ADR-0011](adr/0011-simulation-stays-network-ready.md) (proposed, being written) accepted.
- 1.2 and 1.6 done.

Grouped by trigger: "now" items are cheap and independent; the rest land
just before the work that needs them, not sooner.

| # | Item | When |
|---|---|---|
| 1.6 | [ENG-F3-03](engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-03-misc-cleanups.md): delete the unsound `FrameCallback` path | now (soundness) |
| 1.2 | [ENG-F2-03](engine/ENG-F2-dependency-upgrades/ENG-F2-03-platform-default-features-and-unused-deps.md): winit/egui-winit default features off; unused deps removed ([ADR-0008](adr/0008-keep-winit-for-windowing-and-input.md)) | now |
| 1.7 | Amend [ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md)'s gate (boundary readiness, FPS as second consumer) | now |
| 1.11 | [ENG-F13](engine/ENG-F13-dependency-audit/_feature.md): dependency audit | now, before ENG-F12 or any new dependency |
| 1.8 | [ENG-F10](engine/ENG-F10-world-geometry-from-any-source/_feature.md): world geometry from any source; voxels move to the strategy line ([ADR-0010](adr/0010-world-geometry-is-a-mesh-contract.md)) | before P.2; contract and renderer first |
| 1.9 | [ENG-F11](engine/ENG-F11-shared-app-loop/_feature.md): shared app loop, headless tick, engine `GameClock` | before P.2 |
| 1.10 | [ENG-F12](engine/ENG-F12-input-actions/_feature.md): game-defined input actions; replaces the former 1.5 | before P.2; after 1.11 |
| 1.12 | [ADR-0011](adr/0011-simulation-stays-network-ready.md) (proposed, being written): simulation stays network-ready | now (session B) |
| 1.1 | Save-format migration to [ADR-0006](adr/0006-save-format-contract.md) ([ENG-F2-01](engine/ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md)), envelope in the engine | before [SG-F3](strategy-game/SG-F3-buildings-and-construction/_feature.md) (first new persisted type); an [SG-F2-03](strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md) fix only if it changes the save format |
| 1.13 | [ENG-F20](engine/ENG-F20-graphics-upgrade-touches-one-crate/_feature.md): a wgpu or egui upgrade touches one crate | after 1.9; before any M2 engine feature |
| 1.3 | [ENG-F9](engine/ENG-F9-tests-prove-behaviour/_feature.md): domain tests catch behaviour changes | before [SG-F4](strategy-game/SG-F4-worker-pawns/_feature.md) |

1.1 and 1.3 are triggered by later work, not gate items.

## M2 — Engine MVP

**Intent:** The engine carries both games' current scopes.
**Gate (all must hold):**
- [FPS GDD](../wiki/fps/game-design-document.md) §7 Phase 1–2 playable on engine-line crates:
  ENG-F14–F19 done.
- Strategy v1 slice done: SG-F1-04, SG-F2-03, SG-F2-02.
- Save envelope on [ADR-0006](adr/0006-save-format-contract.md) (1.1).

Features named, not carded until M1's gate closes. The strategy slice and
the FPS rules crate are greenlit now (they don't wait on M1).

### Engine (order: F14 → F15 → F16 → F18 → F17; F19 before FPS Phase 2; all after ENG-F13 and ENG-F20)

| # | Feature | Depends on |
|---|---|---|
| 2.5 | [ENG-F14](engine/ENG-F14-scene-import/_feature.md) scene import | 1.8, 1.11 |
| 2.6 | [ENG-F15](engine/ENG-F15-physics-queries-and-bodies/_feature.md) physics queries and bodies | 1.11 |
| 2.7 | [ENG-F16](engine/ENG-F16-positional-audio/_feature.md) positional audio | 1.11 |
| 2.8 | [ENG-F18](engine/ENG-F18-shared-ui-shell/_feature.md) shared UI shell | 1.11 |
| 2.9 | [ENG-F17](engine/ENG-F17-navigation/_feature.md) navigation (Phase 1 AI may use waypoints meanwhile) | 1.8, 1.11 |
| 2.10 | [ENG-F19](engine/ENG-F19-data-and-mod-content/_feature.md) data and mod content | 1.11; before FPS Phase 2 |

### Strategy

| # | Feature | Depends on |
|---|---|---|
| 2.1 | [SG-F1-04](strategy-game/SG-F1-core-interaction-loop/SG-F1-04-pickup-feedback.md) pickup feedback | M0; land before ENG-F10's voxel extraction |
| 2.2 | [SG-F2-03](strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md) mining not persisted (reproduce first), then [SG-F2-02](strategy-game/SG-F2-known-bugs/SG-F2-02-mining-mesh-gaps.md) mesh gaps | M0; land before ENG-F10's voxel extraction |
| 2.3 | [SG-F3](strategy-game/SG-F3-buildings-and-construction/_feature.md) buildings and construction (not in the v1 slice) | 1.1 |
| 2.4 | [SG-F4](strategy-game/SG-F4-worker-pawns/_feature.md) worker pawns (first possible ECS consumer, [ADR-0004](adr/0004-entity-storage-without-a-general-ecs.md)) | 1.3, 2.3 |

### FPS line

| # | Feature | Depends on |
|---|---|---|
| P.1 | [FPS-F1](fps-game/FPS-F1-game-design-document/_feature.md) [GDD](../wiki/fps/game-design-document.md) v0.2 | — |
| P.3 | FPS rules crate (magazine, body-part damage): platform-free, fps + domain rows in the layering check; needs its own feature first | P.1 |
| P.2 | FPS prototype crate in the monorepo, held apart by the layering check; loads a non-voxel map. Scope: GDD §7 Phase 1 (arena baseline) | P.1, 1.8, 1.9, 1.10; arena also 2.5–2.9 |

FPS phases after P.2 follow GDD §7 (items, weapon wear, wounds, survival, hub
and jobs, Director). Each becomes a feature only after the previous one is
approved.

## M3 — v1.0 and split

**Intent:** The engine boundary is settled and versioned.
**Gate:** the [ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md) gate: licences and the third-party
notices file; two FPS features that didn't change the engine's public API;
the `v1.0` tag.

## M4 — Production-ready

**Intent:** A build could ship to players.
**Gate:** defined when M3 closes. Likely: per-platform perf budgets, crash
reporting, packaging and installers, a save-compatibility promise, all-OS CI
on every PR.

## M5+ — Engine depth

**Intent:** Features found in mature engines, each pulled by a named consumer.
**Gate:** each is its own feature, in no set order: in-engine editor,
rendering options (forward+/deferred, quality tiers), skeletal animation
tooling, scripted-mods ADR, netcode ADR.

Guardrail until then: don't close the door. Scenes stay data (an editor
edits data); ADR-0011's rules cover netcode.

## Backlog (opportunistic, when touching the area)

[ENG-F1-01](engine/ENG-F1-engine-hygiene/ENG-F1-01-constructor-size-cleanup.md)/[02](engine/ENG-F1-engine-hygiene/ENG-F1-02-error-handling-backlog.md)/[03](engine/ENG-F1-engine-hygiene/ENG-F1-03-god-module-splits.md)/[05](engine/ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md)/[07](engine/ENG-F1-engine-hygiene/ENG-F1-07-lint-ratchet.md), [ENG-F2-02](engine/ENG-F2-dependency-upgrades/ENG-F2-02-egui-ui-architecture-migration.md), [ENG-F3-01](engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-01-bind-group-split.md)/[02](engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-02-frustum-culling-sorting.md), [ENG-F4](engine/ENG-F4-terrain-rendering-debt/_feature.md)-*, ENG-F2-05/06/07 (`tracing`, std channels, own `block_on`), [ENG-F2-04](engine/ENG-F2-dependency-upgrades/ENG-F2-04-paste-advisory-cleared.md) (`paste` advisory via `rapier3d`),
[ENG-F1-08](engine/ENG-F1-engine-hygiene/ENG-F1-08-headless-app-tests.md) (headless app wiring tests),
[ENG-F1-09](engine/ENG-F1-engine-hygiene/ENG-F1-09-malformed-prefs-are-reported.md) (malformed prefs reported; land before 1.10, ENG-F12, reworks bindings in prefs).

## Decisions

| Date | Decision | Record |
|---|---|---|
| 2026-10-02 | Foundation gate defines when feature work starts | [ENG-F8](engine/ENG-F8-foundation-gate/_feature.md) |
| 2026-10-02 | Crates belong to one line; dependencies point game → engine; checked in `just check` | [ADR-0005](adr/0005-crate-lines-and-dependency-direction.md) |
| 2026-10-02 | Typed entity stores now; `hecs` when a consumer needs an ECS | [ADR-0004](adr/0004-entity-storage-without-a-general-ecs.md) |
| 2026-10-02 | Save format: postcard envelope; old saves break, no promise before v1.0 | [ADR-0006](adr/0006-save-format-contract.md) |
| 2026-10-02 | Closed-source commercial sale; licence policy | [ADR-0007](adr/0007-third-party-licence-policy.md) |
| 2026-10-02 | Keep winit; drop Wayland title bar to clear `ttf-parser` | [ADR-0008](adr/0008-keep-winit-for-windowing-and-input.md) |
| 2026-10-04 | [ENG-F6](engine/ENG-F6-headless-deterministic-logic/_feature.md) parked: simulation already steps at a fixed 1/60 s; no determinism defect found | [ENG-F6](engine/ENG-F6-headless-deterministic-logic/_feature.md) Notes |
| 2026-10-04 | [SG-F2-03](strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md) is reproduce-first and no longer waits on the save-format migration | [SG-F2-03](strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md) |
| 2026-10-04 | Simulation time comes from one fixed tick; no separate clock | [ADR-0009](adr/0009-simulation-time-is-one-fixed-tick.md) |
| 2026-10-05 | G1 amended: `paste` also arrives via `rapier3d`, so its ignore moves to ENG-F2-04 (G3 covers it); gate verified and closed | [ENG-F8](engine/ENG-F8-foundation-gate/_feature.md) |
| 2026-10-05 | The engine sees world geometry as meshes; voxels move to a strategy-line crate | [ADR-0010](adr/0010-world-geometry-is-a-mesh-contract.md) |
| 2026-10-05 | Repo-split gate requires an agnostic engine and the FPS as a settled second consumer | [ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md) |
| 2026-10-06 | ADR-0010 accepted; ADR-0002 superseded, ADR-0003 narrowed to engine-wide events | [ADR-0010](adr/0010-world-geometry-is-a-mesh-contract.md) |
| 2026-10-05 | Multiplayer later or never; no design may close it | [ADR-0011](adr/0011-simulation-stays-network-ready.md) (proposed, being written) |
| 2026-10-05 | Shared app loop is an engine crate ([ENG-F11](engine/ENG-F11-shared-app-loop/_feature.md)); input is engine actions ([ENG-F12](engine/ENG-F12-input-actions/_feature.md)), replacing 1.5 | [ENG-F11](engine/ENG-F11-shared-app-loop/_feature.md), [ENG-F12](engine/ENG-F12-input-actions/_feature.md) |
| 2026-10-05 | Mods data-only now (layered roots, override by stable id); scripted mods later, own ADR | [ENG-F19](engine/ENG-F19-data-and-mod-content/_feature.md) |
| 2026-10-05 | No dependency assumed; audit verdicts keep / replace / fork / homebrew / drop | [ENG-F13](engine/ENG-F13-dependency-audit/_feature.md) |
| 2026-10-05 | Map format: glTF 2.0 recommended (audit confirms); importer engine, map loader FPS-line | [ENG-F14](engine/ENG-F14-scene-import/_feature.md) |
| 2026-10-06 | Milestones with gates; only the next milestone is carded | [`_STANDARDS.md`](_STANDARDS.md#milestones) |
| 2026-10-06 | ENG-F6's tick criterion moves to ENG-F11; ENG-F6 keeps seeded RNG | [ENG-F6](engine/ENG-F6-headless-deterministic-logic/_feature.md) |
| 2026-10-07 | Audit gives economical and lean verdicts per crate, user calls each; large well-maintained crates green-lit; follow-ups go under ENG-F2 or the feature reworking that code | [ENG-F13](engine/ENG-F13-dependency-audit/_feature.md) |
| 2026-10-07 | Dependency calls recorded: `noise`, `crossbeam-channel`, `pollster`, `phf`, `log`/`env_logger` leave; `ini` stays; `rodio` + own gain/pan for ENG-F16 | [audit](engine/ENG-F13-dependency-audit/audit.md) |
| 2026-10-07 | Fast-moving stacks (wgpu/egui, rapier/glam) upgrade once per milestone close; ENG-F20 (containment) approved and placed in M1, before engine features build on the renderer | [ENG-F2](engine/ENG-F2-dependency-upgrades/_feature.md) |
