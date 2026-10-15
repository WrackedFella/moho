# Moho — Roadmap

The order in which features are tackled, and why, as milestones with gates. [`README.md`](README.md)
indexes what exists; this file sequences it. Item status is not tracked here; it lives on the
[project board](https://github.com/users/WrackedFella/projects/1). Update this file when a milestone gate closes, an item moves between phases, or a decision is made.

**Now:** M0 and M1 are done. M2 is open, in waves (wave 1 started 2026-10-14): the only
milestone planned to card level (rolling wave, [`_STANDARDS.md`](_STANDARDS.md#milestones)).

## M0 — Foundation (done 2026-10-05)

**Intent:** Feature work can start safely.
**Gate:** [ENG-F8](https://github.com/WrackedFella/moho/issues/61) criteria (verified).

Exit: all four [ENG-F8](https://github.com/WrackedFella/moho/issues/61) criteria verified. Order matters: the crate table comes
first, so later moves land in the right crates. [ENG-F7-01](https://github.com/WrackedFella/moho/issues/65) has no dependency
and can run alongside 0.2–0.3.

| # | Item | Criterion | Depends on |
|---|---|---|---|
| 0.1 | Accept [ADR-0005](adr/0005-crate-lines-and-dependency-direction.md) (crate lines) and [ENG-F8](https://github.com/WrackedFella/moho/issues/61) | G2 | — |
| 0.2 | [ENG-F8-01](https://github.com/WrackedFella/moho/issues/63): `moho_sim` merged into the game crate (now `flagstone_game`) | G2 | 0.1 |
| 0.3 | [ENG-F8-02](https://github.com/WrackedFella/moho/issues/64): layering check in `just check`; `moho_core` drops `winit` | G2 | 0.2 |
| 0.4 | [ENG-F7-01](https://github.com/WrackedFella/moho/issues/65): typed chunk and actor stores (domain: tests reviewed first) | G1 | 0.1 |
| 0.5 | [ENG-F7-02](https://github.com/WrackedFella/moho/issues/66): legion removed; deny ignores name their owning cards | G1, G3 | 0.4 |
| 0.6 | [ENG-F2-01](https://github.com/WrackedFella/moho/issues/94) rewritten to [ADR-0006](adr/0006-save-format-contract.md) (decision only) | G4 | — |


## M1 — Both game lines can start (done 2026-10-14)

**Intent:** Each game line builds on the engine on its own, without forking it: the
strategy game on the shared loop, and the FPS as a prototype arena shooter
([GDD](../wiki/searchlight/game-design-document.md) §7 Phase 1).
**Gate (all must hold):**
- [ENG-F10](https://github.com/WrackedFella/moho/issues/80), [ENG-F11](https://github.com/WrackedFella/moho/issues/81) and [ENG-F12](https://github.com/WrackedFella/moho/issues/82) done.
- [ENG-F14](https://github.com/WrackedFella/moho/issues/226) (a level loads from a file), [ENG-F15](https://github.com/WrackedFella/moho/issues/227) (ray queries and hit volumes) and [ENG-F21](https://github.com/WrackedFella/moho/issues/123) (character controller and camera) done: what the arena prototype can't start without and would otherwise copy.
- [ENG-F13](https://github.com/WrackedFella/moho/issues/83) verdicts recorded.
- [ADR-0011](adr/0011-simulation-stays-network-ready.md) accepted.
- [ADR-0012](adr/0012-engine-crate-map-for-m2.md) (engine crate map) accepted.
- 1.2 and 1.6 done.

Grouped by trigger: "now" items are cheap and independent; the rest land
just before the work that needs them, not sooner.

| # | Item | When |
|---|---|---|
| 1.6 | [ENG-F3-03](https://github.com/WrackedFella/moho/issues/208): delete the unsound `FrameCallback` path | now (soundness) |
| 1.2 | [ENG-F2-03](https://github.com/WrackedFella/moho/issues/96): winit/egui-winit default features off; unused deps removed ([ADR-0008](adr/0008-keep-winit-for-windowing-and-input.md)) | now |
| 1.7 | Amend [ENG-F5](https://github.com/WrackedFella/moho/issues/232)'s gate (boundary readiness, FPS as second consumer) | now |
| 1.11 | [ENG-F13](https://github.com/WrackedFella/moho/issues/83): dependency audit | now, before ENG-F12 or any new dependency |
| 1.8 | [ENG-F10](https://github.com/WrackedFella/moho/issues/80): world geometry from any source; voxels move to the strategy line ([ADR-0010](adr/0010-world-geometry-is-a-mesh-contract.md)) | before P.2; contract and renderer first; voxel move ([ENG-F10-03](https://github.com/WrackedFella/moho/issues/140)) after [ENG-F22-02](https://github.com/WrackedFella/moho/issues/169), then [ENG-F22-06](https://github.com/WrackedFella/moho/issues/173) and [ENG-F22-07](https://github.com/WrackedFella/moho/issues/174) |
| 1.9 | [ENG-F11](https://github.com/WrackedFella/moho/issues/81): shared app loop, headless tick, engine `GameClock` | before P.2 |
| 1.10 | [ENG-F12](https://github.com/WrackedFella/moho/issues/82): game-defined input actions; replaces the former 1.5 | before P.2; after 1.11 |
| 1.12 | [ADR-0011](adr/0011-simulation-stays-network-ready.md): simulation stays network-ready | now (session B) |
| 1.1 | Save-format migration to [ADR-0006](adr/0006-save-format-contract.md) ([ENG-F2-01](https://github.com/WrackedFella/moho/issues/94)), envelope in the engine | done 2026-10-08; SG-F4's mark persistence is the first new persisted type |
| 1.13 | [ENG-F20](https://github.com/WrackedFella/moho/issues/104): a wgpu or egui upgrade touches one crate | after 1.9; before 1.16 and [ENG-F18](https://github.com/WrackedFella/moho/issues/230) |
| 1.14 | [ADR-0012](adr/0012-engine-crate-map-for-m2.md): engine crate map, modules first; names the seam list | before ENG-F11 is specced |
| 1.3 | [ENG-F9](https://github.com/WrackedFella/moho/issues/234): domain tests catch behaviour changes | carried to M2 (2.11) |
| 1.15 | [ENG-F22](https://github.com/WrackedFella/moho/issues/167): every test can fail and earns its place (part of 1.3) | waves 1–4 done in M1; the rest carried to M2 (2.12) |
| 1.16 | [ENG-F2-02](https://github.com/WrackedFella/moho/issues/95) then [ENG-F2-08](https://github.com/WrackedFella/moho/issues/101): egui's current entry points, then wgpu and egui upgraded | at M1 close (once-per-milestone upgrade); ENG-F2-08 doesn't compile before ENG-F2-02 |
| 1.17 | [ENG-F2-10](https://github.com/WrackedFella/moho/issues/105): terrain noise in-house; `noise` leaves | after 1.8 |
| 1.18 | [ENG-F14](https://github.com/WrackedFella/moho/issues/226): a game loads a static level from a file | after 1.8, 1.11 |
| 1.19 | [ENG-F15](https://github.com/WrackedFella/moho/issues/227): a ray finds the surface or body part it hits | after 1.11; alongside 1.18 |
| 1.20 | [ENG-F21](https://github.com/WrackedFella/moho/issues/123): a game gets a character controller and camera from the engine | after 1.9, 1.10, 1.19 |

1.1 and 1.3 are triggered by later work, not gate items. 1.13 and 1.15–1.17 are not gate
items either: they unblock neither line and run alongside. 1.13 lands before
[ENG-F18](https://github.com/WrackedFella/moho/issues/230) and before 1.16's wgpu upgrade,
which is its proof.

### 1.15 ENG-F22 card order

Waves follow the files each card's tech spec touches: cards in one wave share no
files and can run together; a later wave waits on the cards named in its row.

| Wave | Cards | Order inside the wave / why |
|---|---|---|
| 1 | [ENG-F22-14](https://github.com/WrackedFella/moho/issues/181), [ENG-F22-15](https://github.com/WrackedFella/moho/issues/182), [ENG-F22-16](https://github.com/WrackedFella/moho/issues/183), [ENG-F22-17](https://github.com/WrackedFella/moho/issues/184), [ENG-F22-18](https://github.com/WrackedFella/moho/issues/185) | The confirmed defects, plus the voxel deletion that shrinks ENG-F22-02 and ENG-F10-03 |
| 2 | [ENG-F22-02](https://github.com/WrackedFella/moho/issues/169), [ENG-F22-01](https://github.com/WrackedFella/moho/issues/168), [ENG-F22-03](https://github.com/WrackedFella/moho/issues/170), [ENG-F22-13](https://github.com/WrackedFella/moho/issues/180) | Removals first: each later card in its area builds on them. ENG-F22-02 drops whatever ENG-F22-18 already removed |
| 3 | [ENG-F22-06](https://github.com/WrackedFella/moho/issues/173), [ENG-F22-07](https://github.com/WrackedFella/moho/issues/174) | After ENG-F22-02 (same voxel and lighting files); ENG-F22-06 also after ENG-F22-16, which moved the blocky vertices it pins. Both before ENG-F10-03 branches |
| 4 | [ENG-F22-05](https://github.com/WrackedFella/moho/issues/172), [ENG-F22-08](https://github.com/WrackedFella/moho/issues/175), [ENG-F22-04](https://github.com/WrackedFella/moho/issues/171), [ENG-F22-09](https://github.com/WrackedFella/moho/issues/176), [ENG-F22-10](https://github.com/WrackedFella/moho/issues/177), [ENG-F22-11](https://github.com/WrackedFella/moho/issues/178) | 05 and 08 after 01; 04 after 13; 09, 10 and 11 after 03. 09 before ENG-F21 moves the controller |
| 5 | [ENG-F22-12](https://github.com/WrackedFella/moho/issues/179), [ENG-F22-20](https://github.com/WrackedFella/moho/issues/187), [ENG-F22-21](https://github.com/WrackedFella/moho/issues/245), [ENG-F22-22](https://github.com/WrackedFella/moho/issues/261), [ENG-F22-24](https://github.com/WrackedFella/moho/issues/263) | 12 after 04 and 14; 20 after 09 and 10; 21 after 10 (same physics test module as 20); 22 and 24 have no dependency |
| 6 | [ENG-F22-19](https://github.com/WrackedFella/moho/issues/186) | After 13 and 04; re-specced 2026-10-14 with no file shared with 12 or 23, so 12, 19 and 23 run in parallel |
| 7 | [ENG-F22-23](https://github.com/WrackedFella/moho/issues/262) | No dependency since the 2026-10-14 re-spec (disjoint files from 12 and 19) |

### 1.18–1.20 card order

Two lanes run side by side, and alongside [ENG-F20](https://github.com/WrackedFella/moho/issues/104).
Cards in one lane share files, so each waits on the one before it.

| Lane | Cards in order | Why |
|---|---|---|
| Level | [ENG-F14-01](https://github.com/WrackedFella/moho/issues/266) level meshes load and collide → [ENG-F14-02](https://github.com/WrackedFella/moho/issues/267) named markers | One importer; touches no physics, renderer or binary code |
| Physics and controller | [ENG-F15-01](https://github.com/WrackedFella/moho/issues/268) characters by handle → [ENG-F21-01](https://github.com/WrackedFella/moho/issues/271) walking from commands → [ENG-F15-02](https://github.com/WrackedFella/moho/issues/269) ray first hit → [ENG-F15-03](https://github.com/WrackedFella/moho/issues/270) hit volumes → [ENG-F21-02](https://github.com/WrackedFella/moho/issues/272) first-person camera | 01 changes the character API everything else builds on; the controller goes next so the strategy game's walking moves once. [ENG-F21-02](https://github.com/WrackedFella/moho/issues/272) changes the binary's camera wiring, so it also waits on [ENG-F20-02](https://github.com/WrackedFella/moho/issues/156) if their footprints meet |

## M2 — Engine MVP

**Intent:** The engine carries both games' current scopes, ranked by what the game
lines ask for.
**Gate:** whatever the games need for a V1 release. M2 runs in waves; Justin declares it
done when the games' requirements are adequately met. Each wave is planned from the
open engine requests (label `engine-request`) when the one before it nears its end.
Engine requests answered here: #302, #304–#307, #315, #316 (#303 goes to the strategy
line, as an [SG-F4](https://github.com/WrackedFella/moho/issues/310) card).

**Current wave set (decided 2026-10-14), all must hold:**
- **FPS:** [GDD](../wiki/searchlight/game-design-document.md) §7 Phase 1–2 playable on
  engine-line crates: [ENG-F16](https://github.com/WrackedFella/moho/issues/228),
  [ENG-F17](https://github.com/WrackedFella/moho/issues/229),
  [ENG-F18](https://github.com/WrackedFella/moho/issues/230) and
  [ENG-F19](https://github.com/WrackedFella/moho/issues/231) done.
- **Strategy:** [SG-F1](https://github.com/WrackedFella/moho/issues/311),
  [SG-F2](https://github.com/WrackedFella/moho/issues/308),
  [SG-F5](https://github.com/WrackedFella/moho/issues/309) and
  [SG-F4](https://github.com/WrackedFella/moho/issues/310) done.
- **Save:** saves use the [ADR-0006](adr/0006-save-format-contract.md) envelope
  ([ENG-F2-01](https://github.com/WrackedFella/moho/issues/94), done 2026-10-08).
  SG-F4's "marked block survives save and load" card adds the first new persisted type.

Not gate items, but in M2: [ENG-F25](https://github.com/WrackedFella/moho/issues/346) (before SG-F4's marks-persist card), ENG-F23, ENG-F24, [ENG-F9](https://github.com/WrackedFella/moho/issues/234),
[ENG-F6](https://github.com/WrackedFella/moho/issues/233) seeded RNG, pause and speed, and the bug cards.

### Engine waves

Items in one wave touch disjoint crates and run in parallel. A later wave starts as
its consumers near, not when the earlier wave fully closes.

| # | Item | Requests | Consumer | Wave | Depends on |
|---|---|---|---|---|---|
| 2.1 | [ENG-F21-03](https://github.com/WrackedFella/moho/issues/330): a character that leaves the ground keeps its momentum (`moho_physics`, reproduce first) | #302 | possession, FPS movement | 1 | — |
| 2.2 | [ENG-F18-01](https://github.com/WrackedFella/moho/issues/331): saved settings survive a later console command (binary, `flagstone_ui`) | #179 comment | strategy settings; FPS via ENG-F18 | 1 | — |
| 2.3 | [ENG-F18-02](https://github.com/WrackedFella/moho/issues/332): menu music plays from `assets/` (`flagstone_ui`) | #186 check | strategy start menu | 1 | — |
| 2.3a | [ENG-F18-03](https://github.com/WrackedFella/moho/issues/333): Escape pauses the game in place instead of reloading the world (binary, `flagstone_ui`) | Justin, 2026-10-14 | strategy play; FPS via ENG-F18 | 1 | — |
| 2.4 | [ENG-F16](https://github.com/WrackedFella/moho/issues/228) positional audio, with the `moho_audio` system tests deferred from ENG-F22 (priority) | — | FPS GDD §10; strategy cues | 1 | — |
| 2.5 | [ENG-F6](https://github.com/WrackedFella/moho/issues/233) seeded RNG (reopened): [ENG-F6-01](https://github.com/WrackedFella/moho/issues/335) | #316 | FPS-F2-02 (#299); strategy world gen | 1 | — |
| 2.5a | [ENG-F25](https://github.com/WrackedFella/moho/issues/346) a game line adds save files without changing the engine ([ADR-0016](adr/0016-game-lines-own-their-save-kinds.md)): [ENG-F25-01](https://github.com/WrackedFella/moho/issues/347) (priority) | — | SG-F4 marks persist; FPS saves | 1 | before SG-F4's marks-persist card |
| 2.6 | [ENG-F23](https://github.com/WrackedFella/moho/issues/327): the game knows what is under the pointer (new) | #304 | SG-F5 select; SG-F4 mark; SG-F3 place | 2 | — |
| 2.7 | [ENG-F24](https://github.com/WrackedFella/moho/issues/328): the game marks places in the world (new): markers first | #305 | SG-F5, SG-F4, SG-F3 | 2 | — |
| 2.8 | [ENG-F9](https://github.com/WrackedFella/moho/issues/234) domain tests catch behaviour changes (from M1's 1.3) | — | SG-F4 | 2–3 | before SG-F4 |
| 2.9 | [ENG-F11-04](https://github.com/WrackedFella/moho/issues/334): the simulation pauses and runs at 1x, 2x or 3x (`moho_app`) | #307 | SG-F4 follow-up; FPS pause | 3 | SG-F4's skeleton |
| 2.10 | [ENG-F24-03](https://github.com/WrackedFella/moho/issues/329): the game hides the world above a chosen height (renderer) | #306 | SG-F4 underground marks | 3 | ENG-F24's first card |
| 2.11 | [ENG-F18](https://github.com/WrackedFella/moho/issues/230) shared UI shell, widened with on-screen text | #315 | FPS Phase 2 | 4 | 2.2 |
| 2.12 | [ENG-F19](https://github.com/WrackedFella/moho/issues/231) layered content | — | FPS Phase 2 | 4 | — |
| 2.13 | [ENG-F17](https://github.com/WrackedFella/moho/issues/229) navigation over static levels (FPS uses waypoints until then) | — | FPS | 4 | — |
| 2.14 | [ENG-F26](https://github.com/WrackedFella/moho/issues/359): one key carries tap, hold, modifier and toggle actions (new; `moho_input`, settings display) | Justin, 2026-10-15 | strategy sprint toggle; FPS reload, check mag, unjam | 4 | before ENG-F18's keybind card |
| 2.15 | [ENG-F27](https://github.com/WrackedFella/moho/issues/361): the game lines are called flagstone and searchlight; one card per line ([#362](https://github.com/WrackedFella/moho/issues/362) FPS, [#363](https://github.com/WrackedFella/moho/issues/363) strategy) | Justin, 2026-10-15 | ENG-F5 split (M3) | 1–2 gaps | #362 after #358; #363 after #355, before #331 |
| 2.16 | [ENG-F28](https://github.com/WrackedFella/moho/issues/373): mouse look feels like a Source game (raw, per-frame look; sensitivity, smoothing, acceleration and vsync settings) | Justin, 2026-10-15 | FPS prototype feel; strategy first-person camera | 2 | prefs and settings cards after #331, #333, #354 |

- **Wave 1, start now:** the bugs, ENG-F16, ENG-F6 and ENG-F25. They need no other M2 work.
- **Wave 2, before SG-F5 and SG-F4:** ENG-F23 and ENG-F24 unblock SG-F5; ENG-F9 comes before SG-F4.
  ENG-F28 (mouse feel) runs alongside; its settings cards queue behind the wave-1 prefs cards.
- **Wave 3, SG-F4 follow-ups:** pause and speed, then the height slice, once SG-F4's walking skeleton exists.
- **Wave 4, before FPS Phase 2:** ENG-F18 (after 2.2), ENG-F19, ENG-F17 and ENG-F26
  (before ENG-F18's keybind card, which builds the hold and chord capture on it).

### Strategy

| # | Feature | Engine items it needs | Notes |
|---|---|---|---|
| S.1 | [SG-F1](https://github.com/WrackedFella/moho/issues/311) possessing a pawn feels right | 2.1 for feel | — |
| S.2 | [SG-F2](https://github.com/WrackedFella/moho/issues/308) a saved world comes back as I left it: [SG-F2-01](https://github.com/WrackedFella/moho/issues/341), [SG-F2-03](https://github.com/WrackedFella/moho/issues/343), [SG-F2-02](https://github.com/WrackedFella/moho/issues/342) | none | its load-path card covers the #264 gap (scene load skips releasing held keys) |
| S.3 | [SG-F5](https://github.com/WrackedFella/moho/issues/309) run the colony from above, step into a pawn | 2.6, 2.7 | block and pawn picking are strategy-line |
| S.4 | [SG-F4](https://github.com/WrackedFella/moho/issues/310) pawns dig what I mark | 2.5a, 2.7, 2.8; then 2.9, 2.10 | first new persisted type; #303 voxel routing is its card. Before SG-F3 |
| S.5 | [SG-F3](https://github.com/WrackedFella/moho/issues/312) buildings and construction (not a gate item) | 2.7 (placement preview) | after SG-F4 |
| S.6 | SG-F6 new worlds look intentional ([draft](flagstone/SG-F6-world-generation/_feature.md), not filed; not a gate item) | none | waits for the user to rank the world-generation problems |

### FPS line

| # | Feature | Engine items it needs |
|---|---|---|
| P.1 | [FPS-F1](searchlight/FPS-F1-game-design-document/_feature.md) [GDD](../wiki/searchlight/game-design-document.md) v0.2 | — |
| P.2 | [FPS-F2](https://github.com/WrackedFella/moho/issues/297) arena baseline (GDD §7 Phase 1), with the rules crate `searchlight_rules` ([ADR-0015](adr/0015-fps-rules-are-a-platform-free-crate.md)) | none; stopgaps until then: non-positional sound, waypoints, rectangle HUD, a local seeded generator |
| P.3 | GDD §7 Phase 2 (items and loadout) | 2.11 with text, 2.12 |

FPS phases after Phase 2 follow GDD §7. Each becomes a feature only after the previous
one is approved.

### Out of M2

| Idea | Why it waits | Revisit when |
|---|---|---|
| A wall impact every tick (damage needs a threshold) | No consumer | The first impact-damage rule |
| Gamepad limits and turn speed (#151) | Closed, no request | A gamepad request |
| Shared A* between strategy routes and ENG-F17 | Neither exists yet | Both exist |

## M3 — v1.0 and split

**Intent:** The engine boundary is settled and versioned.
**Gate:** the [ENG-F5](https://github.com/WrackedFella/moho/issues/232) gate: licences and the third-party
notices file; the seam list named in [ADR-0012](adr/0012-engine-crate-map-for-m2.md) is unchanged by the last two
FPS features; the `v1.0` tag.

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

[ENG-F1-01](engine/ENG-F1-engine-hygiene/ENG-F1-01-constructor-size-cleanup.md)/[02](engine/ENG-F1-engine-hygiene/ENG-F1-02-error-handling-backlog.md)/[03](engine/ENG-F1-engine-hygiene/ENG-F1-03-god-module-splits.md)/[05](engine/ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md)/[07](engine/ENG-F1-engine-hygiene/ENG-F1-07-lint-ratchet.md), [ENG-F3-01](engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-01-bind-group-split.md)/[02](engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-02-frustum-culling-sorting.md), [ENG-F4](engine/ENG-F4-terrain-rendering-debt/_feature.md)-*,
[ENG-F1-08](engine/ENG-F1-engine-hygiene/ENG-F1-08-headless-app-tests.md) (headless app wiring tests),
[ENG-F1-04](engine/ENG-F1-engine-hygiene/ENG-F1-04-keybind-test-layering.md) (keybind test tiers; drop what ENG-F22's UI test cards already cover),
[ENG-F1-10](https://github.com/WrackedFella/moho/issues/246) (key capture stops listening on a conflict; pair with ENG-F1-04).

## Decisions

| Date | Decision | Record |
|---|---|---|
| 2026-10-02 | Foundation gate defines when feature work starts | [ENG-F8](https://github.com/WrackedFella/moho/issues/61) |
| 2026-10-02 | Crates belong to one line; dependencies point game → engine; checked in `just check` | [ADR-0005](adr/0005-crate-lines-and-dependency-direction.md) |
| 2026-10-02 | Typed entity stores now; `hecs` when a consumer needs an ECS | [ADR-0004](adr/0004-entity-storage-without-a-general-ecs.md) |
| 2026-10-02 | Save format: postcard envelope; old saves break, no promise before v1.0 | [ADR-0006](adr/0006-save-format-contract.md) |
| 2026-10-02 | Closed-source commercial sale; licence policy | [ADR-0007](adr/0007-third-party-licence-policy.md) |
| 2026-10-02 | Keep winit; drop Wayland title bar to clear `ttf-parser` | [ADR-0008](adr/0008-keep-winit-for-windowing-and-input.md) |
| 2026-10-04 | [ENG-F6](https://github.com/WrackedFella/moho/issues/233) parked: simulation already steps at a fixed 1/60 s; no determinism defect found | [ENG-F6](https://github.com/WrackedFella/moho/issues/233) Notes |
| 2026-10-04 | [SG-F2-03](https://github.com/WrackedFella/moho/issues/343) is reproduce-first and no longer waits on the save-format migration | [SG-F2-03](https://github.com/WrackedFella/moho/issues/343) |
| 2026-10-04 | Simulation time comes from one fixed tick; no separate clock | [ADR-0009](adr/0009-simulation-time-is-one-fixed-tick.md) |
| 2026-10-05 | G1 amended: `paste` also arrives via `rapier3d`, so its ignore moves to ENG-F2-04 (G3 covers it); gate verified and closed | [ENG-F8](https://github.com/WrackedFella/moho/issues/61) |
| 2026-10-05 | The engine sees world geometry as meshes; voxels move to a strategy-line crate | [ADR-0010](adr/0010-world-geometry-is-a-mesh-contract.md) |
| 2026-10-05 | Repo-split gate requires an agnostic engine and the FPS as a settled second consumer | [ENG-F5](https://github.com/WrackedFella/moho/issues/232) |
| 2026-10-06 | ADR-0010 accepted; ADR-0002 superseded, ADR-0003 narrowed to engine-wide events | [ADR-0010](adr/0010-world-geometry-is-a-mesh-contract.md) |
| 2026-10-05 | Multiplayer later or never; no design may close it | [ADR-0011](adr/0011-simulation-stays-network-ready.md) |
| 2026-10-05 | Shared app loop is an engine crate ([ENG-F11](https://github.com/WrackedFella/moho/issues/81)); input is engine actions ([ENG-F12](https://github.com/WrackedFella/moho/issues/82)), replacing 1.5 | [ENG-F11](https://github.com/WrackedFella/moho/issues/81), [ENG-F12](https://github.com/WrackedFella/moho/issues/82) |
| 2026-10-05 | Mods data-only now (layered roots, override by stable id); scripted mods later, own ADR | [ENG-F19](https://github.com/WrackedFella/moho/issues/231) |
| 2026-10-05 | No dependency assumed; audit verdicts keep / replace / fork / homebrew / drop | [ENG-F13](https://github.com/WrackedFella/moho/issues/83) |
| 2026-10-05 | Map format: glTF 2.0 recommended (audit confirms); importer engine, map loader FPS-line | [ENG-F14](https://github.com/WrackedFella/moho/issues/226) |
| 2026-10-06 | Milestones with gates; only the next milestone is carded | [`_STANDARDS.md`](_STANDARDS.md#milestones) |
| 2026-10-06 | ENG-F6's tick criterion moves to ENG-F11; ENG-F6 keeps seeded RNG | [ENG-F6](https://github.com/WrackedFella/moho/issues/233) |
| 2026-10-07 | Audit gives economical and lean verdicts per crate, user calls each; large well-maintained crates green-lit; follow-ups go under ENG-F2 or the feature reworking that code | [ENG-F13](https://github.com/WrackedFella/moho/issues/83) |
| 2026-10-07 | Dependency calls recorded: `noise`, `crossbeam-channel`, `pollster`, `phf`, `log`/`env_logger` leave; `ini` stays; `rodio` + own gain/pan for ENG-F16 | [audit](engine/ENG-F13-dependency-audit/audit.md) |
| 2026-10-07 | Engine crate map decided in an ADR before ENG-F11 (modules first) | [ADR-0012](adr/0012-engine-crate-map-for-m2.md) |
| 2026-10-07 | Generic controller and camera are an engine feature | [ENG-F21](https://github.com/WrackedFella/moho/issues/123) |
| 2026-10-07 | Fast-moving stacks (wgpu/egui, rapier/glam) upgrade once per milestone close; ENG-F20 (containment) approved and placed in M1, before engine features build on the renderer | [ENG-F2](https://github.com/WrackedFella/moho/issues/93) |
| 2026-10-08 | ENG-F10's voxel move no longer waits on SG-F1-04, SG-F2-03 and SG-F2-02 (unfiled, Strategy lane planning-only); it waits on the ENG-F22 voxel cards instead | [ENG-F10](https://github.com/WrackedFella/moho/issues/80) Notes |
| 2026-10-09 | M1 means both game lines can start: ENG-F14, F15 and F21 (what the FPS arena prototype needs and would otherwise copy) join its gate from M2; ENG-F20 moves before ENG-F18 and the M1-close upgrade instead of before all engine features | M1 section |
| 2026-10-14 | M1 gate verified and closed; ENG-F9 and ENG-F22's remaining cards carried to M2 | M1 and M2 sections |
| 2026-10-14 | M2 is ranked by the game lines' engine requests and runs in waves; its gate is what the games need for V1, declared met by Justin. ENG-F16 is the priority; SG-F5 and SG-F4 join the gate; SG-F4 comes before SG-F3; #303 routing is strategy-line | M2 section |
| 2026-10-14 | New ENG-F23 (pointer to world ray, #304) and ENG-F24 (world markers and height slice, #305, #306); ENG-F18 gains on-screen text (#315) and one prefs owner; ENG-F6 reopened for seeded RNG (#316) | M2 section; [ENG-F18](https://github.com/WrackedFella/moho/issues/230), [ENG-F6](https://github.com/WrackedFella/moho/issues/233) |
| 2026-10-14 | Strategy mode is the primary game; first person is possession of one pawn. Strategy order is SG-F2, SG-F5, SG-F4, SG-F3 (pawns before buildings); SG-F1 any time, SG-F6 once ranked; gameplay UI and sound ride inside each feature | [SG-F5](https://github.com/WrackedFella/moho/issues/309), [SG-F4](https://github.com/WrackedFella/moho/issues/310), [SG-F3](https://github.com/WrackedFella/moho/issues/312) |
| 2026-10-14 | Game lines own their save kinds; the engine keeps only the envelope framing. ENG-F25 lands in wave 1, before SG-F4's marks-persist card | [ADR-0016](adr/0016-game-lines-own-their-save-kinds.md), [ENG-F25](https://github.com/WrackedFella/moho/issues/346) |
| 2026-10-15 | New ENG-F26 (tap, hold, modifier and toggle bindings) in M2 wave 4, not a gate item. Brings back the modifier chords ENG-F12 dropped on 2026-10-08, now with a consumer; hold timing counts ticks (ADR-0011) | [ENG-F26](https://github.com/WrackedFella/moho/issues/359) |
| 2026-10-15 | The strategy line is renamed flagstone and the FPS line searchlight inside the monorepo, ahead of the split; engine crates keep `moho_`, and SG-/FPS- IDs stay | [ENG-F27](https://github.com/WrackedFella/moho/issues/361) |
| 2026-10-15 | New ENG-F28 (Source-like mouse look) in M2 wave 2, not a gate item: look turns every frame ahead of the tick that consumes it (ADR-0011 holds), raw by default, settings in the same feature | [ENG-F28](https://github.com/WrackedFella/moho/issues/373) |
