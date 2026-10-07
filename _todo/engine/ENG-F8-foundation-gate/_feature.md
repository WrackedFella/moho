# ENG-F8 — Feature work starts on a verified foundation

**Issue:** #61
**Integration branch:** `feature/ENG-F8-foundation-gate`

## Summary

The Foundation gate: the exit criteria that must hold before feature work starts
on either game line. It covers only decisions that get more expensive with every
feature built on top of them: entity storage, dependency direction between the
lines, the supply chain and licence policy, and the save format. Everything else
from the 2026-10 foundation review is post-gate backlog (see Notes).

## Exit criteria

1. **Entity storage is maintained** ([ADR-0004](../../adr/0004-entity-storage-without-a-general-ecs.md) accepted).
   `cargo tree --workspace -i legion` reports no match, and `deny.toml` has no
   `instant` advisory ignore. (`paste` also arrives via `rapier3d`; its ignore
   is owned by [ENG-F2-04](../ENG-F2-dependency-upgrades/ENG-F2-04-paste-advisory-cleared.md).)
2. **Game lines are independent by construction** ([ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md) accepted).
   `just check` runs a layering check that fails when an engine crate depends
   on a game-line crate, a game line depends on another game line, or a domain
   crate (`moho_core`, `moho_game`) reaches `winit`, `wgpu` or `egui` through
   normal or build edges. The check has a test that injects a forbidden edge
   and sees it fail. `CLAUDE.md`'s architecture table and domain-logic paths
   match [ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md)'s crate table.
3. **The supply chain is accounted for** ([ADR-0007](../../adr/0007-third-party-licence-policy.md) accepted).
   `just deny` passes in CI, and every `[advisories] ignore` entry names the
   open card that removes it.
4. **The save format has a decided contract** ([ADR-0006](../../adr/0006-save-format-contract.md) accepted).
   [ENG-F2-01](../ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md) is rewritten to match the ADR. The decision is in the gate; the
   migration is not.

## Scope

- In: [ENG-F7](../ENG-F7-maintained-ecs/_feature.md) (rescoped to remove `legion`); the layering check; merging
  `moho_sim` into `moho_game`; dropping `moho_core`'s unused `winit`
  dependency; [ADR-0004](../../adr/0004-entity-storage-without-a-general-ecs.md), [ADR-0005](../../adr/0005-crate-lines-and-dependency-direction.md), [ADR-0006](../../adr/0006-save-format-contract.md) and [ADR-0007](../../adr/0007-third-party-licence-policy.md); the deny-ignore owner pass.
- Out: decoupling the fixed tick from rendering ([ENG-F6](../ENG-F6-headless-deterministic-logic/_feature.md), parked; [ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md)), test cull/rework, crate merges other
  than `moho_sim`, dependency upgrades, the physical repo split ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md)).
  Those are post-gate (Notes). Writing the FPS GDD ([FPS-F1](../../fps-game/FPS-F1-game-design-document/_feature.md)) is planning and
  changes no code, so it can run in parallel with the gate.

## Items

| Item | Criterion | Status |
|---|---|---|
| [ENG-F8-01 moho-sim-merged-into-game](ENG-F8-01-moho-sim-merged-into-game.md) | G2 | done |
| [ENG-F8-02 layering-check](ENG-F8-02-layering-check.md) | G2 | done |
| [ENG-F7-01](../ENG-F7-maintained-ecs/ENG-F7-01-typed-entity-stores.md) typed-entity-stores | G1 | done |
| [ENG-F7-02](../ENG-F7-maintained-ecs/ENG-F7-02-legion-removed.md) legion-removed (also clears G3's ignore owners) | G1, G3 | done |
| [ENG-F2-01](../ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md) rewritten to [ADR-0006](../../adr/0006-save-format-contract.md) | G4 | done |

## Notes

Post-gate tiers, from the 2026-10 foundation review:

- **Early** (soon after the gate, before the named feature that needs it):
  [ENG-F9](../ENG-F9-tests-prove-behaviour/_feature.md) test cull/rework; save-format migration (before the first feature
  adds a persisted type); `winit`/`egui-winit` default features off (clears
  `ttf-parser`); unused-dependency removal; `moho_types` and `moho_input`
  merged into `moho_ui` with one key-code model; [ADR-0008](../../adr/0008-keep-winit-for-windowing-and-input.md) accepted; [ENG-F5](../ENG-F5-physical-repo-split/_feature.md)
  gate amended; [ENG-F3-03](../ENG-F3-renderer-pipeline-cleanup/ENG-F3-03-misc-cleanups.md)'s unsound `FrameCallback` path deleted.
- **Opportunistic:** [ENG-F1-01](../ENG-F1-engine-hygiene/ENG-F1-01-constructor-size-cleanup.md)/[02](../ENG-F1-engine-hygiene/ENG-F1-02-error-handling-backlog.md)/[03](../ENG-F1-engine-hygiene/ENG-F1-03-god-module-splits.md)/[05](../ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md), [ENG-F2-02](../ENG-F2-dependency-upgrades/ENG-F2-02-egui-ui-architecture-migration.md), [ENG-F3-01](../ENG-F3-renderer-pipeline-cleanup/ENG-F3-01-bind-group-split.md)/[02](../ENG-F3-renderer-pipeline-cleanup/ENG-F3-02-frustum-culling-sorting.md), [ENG-F4](../ENG-F4-terrain-rendering-debt/_feature.md)-*,
  `noise`'s duplicate `rand`, `ini`/`phf` replacement, `crossbeam-channel` →
  `std::sync::mpsc`, `log` → `tracing`.
