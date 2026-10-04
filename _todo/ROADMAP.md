# Moho — Roadmap

The order in which features are tackled, and why. [`README.md`](README.md)
indexes what exists; this file sequences it. Update it when a phase's
status changes, an item moves between phases, or a decision is made.

**Now:** Phase 0, the Foundation gate ([ENG-F8](engine/ENG-F8-foundation-gate/_feature.md), issue #61), on integration
branch `feature/ENG-F8-foundation-gate`.
No feature work on either game line until its four criteria hold.

## Phase 0 — Foundation gate

Exit: all four [ENG-F8](engine/ENG-F8-foundation-gate/_feature.md) criteria verified. Order matters: the crate table comes
first, so later moves land in the right crates. [ENG-F7-01](engine/ENG-F7-maintained-ecs/ENG-F7-01-typed-entity-stores.md) has no dependency
and can run alongside 0.2–0.3.

| # | Item | Criterion | Depends on | Status |
|---|---|---|---|---|
| 0.1 | Accept [ADR-0005](adr/0005-crate-lines-and-dependency-direction.md) (crate lines) and [ENG-F8](engine/ENG-F8-foundation-gate/_feature.md) | G2 | — | done |
| 0.2 | [ENG-F8-01](engine/ENG-F8-foundation-gate/ENG-F8-01-moho-sim-merged-into-game.md): `moho_sim` merged into `moho_game` | G2 | 0.1 | in progress (#63) |
| 0.3 | [ENG-F8-02](engine/ENG-F8-foundation-gate/ENG-F8-02-layering-check.md): layering check in `just check`; `moho_core` drops `winit` | G2 | 0.2 | ready (#64) |
| 0.4 | [ENG-F7-01](engine/ENG-F7-maintained-ecs/ENG-F7-01-typed-entity-stores.md): typed chunk and actor stores (domain: tests reviewed first) | G1 | 0.1 | ready (#65) |
| 0.5 | [ENG-F7-02](engine/ENG-F7-maintained-ecs/ENG-F7-02-legion-removed.md): legion removed; deny ignores name their owning cards | G1, G3 | 0.4 | ready (#66) |
| 0.6 | [ENG-F2-01](engine/ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md) rewritten to [ADR-0006](adr/0006-save-format-contract.md) (decision only) | G4 | — | done |

## Phase 1 — Early (post-gate, ahead of the feature that needs it)

| # | Item | Must land before | Status |
|---|---|---|---|
| 1.1 | Save-format migration to [ADR-0006](adr/0006-save-format-contract.md) ([ENG-F2-01](engine/ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md)) | [SG-F3](strategy-game/SG-F3-buildings-and-construction/_feature.md) (first new persisted type); an [SG-F2-03](strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md) fix only if it changes the save format | not started |
| 1.2 | [ENG-F2-03](engine/ENG-F2-dependency-upgrades/ENG-F2-03-platform-default-features-and-unused-deps.md): winit/egui-winit default features off; unused deps removed ([ADR-0008](adr/0008-keep-winit-for-windowing-and-input.md)) | — | not started |
| 1.3 | [ENG-F9](engine/ENG-F9-tests-prove-behaviour/_feature.md): domain tests catch behaviour changes | [SG-F4](strategy-game/SG-F4-worker-pawns/_feature.md) | proposed |
| 1.5 | `moho_types`/`moho_input` → `moho_ui`; one key model; cursor grab order ([ADR-0008](adr/0008-keep-winit-for-windowing-and-input.md)) | FPS input work | not carded |
| 1.6 | [ENG-F3-03](engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-03-misc-cleanups.md): delete the unsound `FrameCallback` path | — | not started |
| 1.7 | Amend [ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md)'s gate (boundary-readiness criteria, FPS consumer trigger) | Phase 3 | not started |

## Phase 2 — Strategy feature work

| # | Feature | Depends on | Status |
|---|---|---|---|
| 2.1 | [SG-F1-04](strategy-game/SG-F1-core-interaction-loop/SG-F1-04-pickup-feedback.md) pickup feedback | Phase 0 | not started |
| 2.2 | [SG-F2-03](strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md) mining not persisted (reproduce first), then [SG-F2-02](strategy-game/SG-F2-known-bugs/SG-F2-02-mining-mesh-gaps.md) mesh gaps | Phase 0 | not started |
| 2.3 | [SG-F3](strategy-game/SG-F3-buildings-and-construction/_feature.md) buildings and construction | 1.1 | parked |
| 2.4 | [SG-F4](strategy-game/SG-F4-worker-pawns/_feature.md) worker pawns (first possible ECS consumer, [ADR-0004](adr/0004-entity-storage-without-a-general-ecs.md)) | 1.3, 2.3 | parked |

## Parallel — FPS line

| # | Feature | Depends on | Status |
|---|---|---|---|
| P.1 | [FPS-F1](fps-game/FPS-F1-game-design-document/_feature.md) GDD (writing only; can run during Phase 0) | — | not started |
| P.2 | FPS prototype crate in the monorepo, held apart by the layering check | Phase 0, P.1 | — |

## Phase 3 — v1 and repo split

[ENG-F5](engine/ENG-F5-physical-repo-split/_feature.md): tag `v1.0` and split
once its amended gate holds (2.1, 2.2, the third-party notices file,
boundary readiness, and P.2 as the second consumer).

## Backlog (opportunistic, when touching the area)

[ENG-F1-01](engine/ENG-F1-engine-hygiene/ENG-F1-01-constructor-size-cleanup.md)/[02](engine/ENG-F1-engine-hygiene/ENG-F1-02-error-handling-backlog.md)/[03](engine/ENG-F1-engine-hygiene/ENG-F1-03-god-module-splits.md)/[05](engine/ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md)/[07](engine/ENG-F1-engine-hygiene/ENG-F1-07-lint-ratchet.md), [ENG-F2-02](engine/ENG-F2-dependency-upgrades/ENG-F2-02-egui-ui-architecture-migration.md), [ENG-F3-01](engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-01-bind-group-split.md)/[02](engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-02-frustum-culling-sorting.md), [ENG-F4](engine/ENG-F4-terrain-rendering-debt/_feature.md)-*, `ini`/`phf` key
tables, `noise`'s duplicate `rand`, `crossbeam-channel` → `std::sync::mpsc`,
`log` → `tracing`.

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
