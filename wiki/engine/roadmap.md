# Engine roadmap

The order in which engine work is tackled, as milestones with gates. Milestones are named `ENG-M<n>`. Gates and
dependencies that span lines are in the [cross-line roadmap](../roadmap.md). [`_todo/README.md`](../../_todo/README.md)
indexes what exists; this file sequences it. Item status is not tracked here; it lives on the
[project board](https://github.com/users/WrackedFella/projects/1).

ENG-M0 and ENG-M1 are done. ENG-M2 is open, in waves, and is the only milestone planned to card level (rolling wave,
[`_STANDARDS.md`](../../_todo/_STANDARDS.md#5-milestones)).

## ENG-M0 — Foundation (done)

**Intent:** Feature work can start safely.
**Gate:** [ENG-F8](https://github.com/WrackedFella/moho/issues/61) criteria (verified).

Exit: all four [ENG-F8](https://github.com/WrackedFella/moho/issues/61) criteria verified. Order matters: the crate table comes
first, so later moves land in the right crates. [ENG-F7-01](https://github.com/WrackedFella/moho/issues/65) has no dependency
and can run alongside 0.2–0.3.

| # | Item | Criterion | Depends on |
|---|---|---|---|
| 0.1 | Accept [ADR-0005](../../_todo/adr/0005-crate-lines-and-dependency-direction.md) (crate lines) and [ENG-F8](https://github.com/WrackedFella/moho/issues/61) | G2 | — |
| 0.2 | [ENG-F8-01](https://github.com/WrackedFella/moho/issues/63): `moho_sim` merged into the game crate (now `flagstone_game`) | G2 | 0.1 |
| 0.3 | [ENG-F8-02](https://github.com/WrackedFella/moho/issues/64): layering check in `just check`; `moho_core` drops `winit` | G2 | 0.2 |
| 0.4 | [ENG-F7-01](https://github.com/WrackedFella/moho/issues/65): typed chunk and actor stores (domain: tests reviewed first) | G1 | 0.1 |
| 0.5 | [ENG-F7-02](https://github.com/WrackedFella/moho/issues/66): legion removed; deny ignores name their owning cards | G1, G3 | 0.4 |
| 0.6 | [ENG-F2-01](https://github.com/WrackedFella/moho/issues/94) rewritten to [ADR-0006](../../_todo/adr/0006-save-format-contract.md) (decision only) | G4 | — |


## ENG-M1 — Both game lines can start (done)

**Intent:** Each game line builds on the engine on its own, without forking it: the
strategy game on the shared loop, and the FPS as a prototype arena shooter
([GDD](../searchlight/game-design-document.md) §7 Phase 1).
**Gate (all must hold):**
- [ENG-F10](https://github.com/WrackedFella/moho/issues/80), [ENG-F11](https://github.com/WrackedFella/moho/issues/81) and [ENG-F12](https://github.com/WrackedFella/moho/issues/82) done.
- [ENG-F14](https://github.com/WrackedFella/moho/issues/226) (a level loads from a file), [ENG-F15](https://github.com/WrackedFella/moho/issues/227) (ray queries and hit volumes) and [ENG-F21](https://github.com/WrackedFella/moho/issues/123) (character controller and camera) done: what the arena prototype can't start without and would otherwise copy.
- [ENG-F13](https://github.com/WrackedFella/moho/issues/83) verdicts recorded.
- [ADR-0011](../../_todo/adr/0011-simulation-stays-network-ready.md) accepted.
- [ADR-0012](../../_todo/adr/0012-engine-crate-map-for-m2.md) (engine crate map) accepted.
- 1.2 and 1.6 done.

Grouped by trigger: "now" items are cheap and independent; the rest land
just before the work that needs them, not sooner.

| # | Item | When |
|---|---|---|
| 1.6 | [ENG-F3-03](https://github.com/WrackedFella/moho/issues/208): delete the unsound `FrameCallback` path | now (soundness) |
| 1.2 | [ENG-F2-03](https://github.com/WrackedFella/moho/issues/96): winit/egui-winit default features off; unused deps removed ([ADR-0008](../../_todo/adr/0008-keep-winit-for-windowing-and-input.md)) | now |
| 1.7 | Amend [ENG-F5](https://github.com/WrackedFella/moho/issues/232)'s gate (boundary readiness, FPS as second consumer) | now |
| 1.11 | [ENG-F13](https://github.com/WrackedFella/moho/issues/83): dependency audit | now, before ENG-F12 or any new dependency |
| 1.8 | [ENG-F10](https://github.com/WrackedFella/moho/issues/80): world geometry from any source; voxels move to the strategy line ([ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md)) | before FPS-F2; contract and renderer first; voxel move ([ENG-F10-03](https://github.com/WrackedFella/moho/issues/140)) after [ENG-F22-02](https://github.com/WrackedFella/moho/issues/169), then [ENG-F22-06](https://github.com/WrackedFella/moho/issues/173) and [ENG-F22-07](https://github.com/WrackedFella/moho/issues/174) |
| 1.9 | [ENG-F11](https://github.com/WrackedFella/moho/issues/81): shared app loop, headless tick, engine `GameClock` | before FPS-F2 |
| 1.10 | [ENG-F12](https://github.com/WrackedFella/moho/issues/82): game-defined input actions; replaces the former 1.5 | before FPS-F2; after 1.11 |
| 1.12 | [ADR-0011](../../_todo/adr/0011-simulation-stays-network-ready.md): simulation stays network-ready | now (session B) |
| 1.1 | Save-format migration to [ADR-0006](../../_todo/adr/0006-save-format-contract.md) ([ENG-F2-01](https://github.com/WrackedFella/moho/issues/94)), envelope in the engine | done; SG-F4's mark persistence is the first new persisted type |
| 1.13 | [ENG-F20](https://github.com/WrackedFella/moho/issues/104): a wgpu or egui upgrade touches one crate | after 1.9; before 1.16 and [ENG-F18](https://github.com/WrackedFella/moho/issues/230) |
| 1.14 | [ADR-0012](../../_todo/adr/0012-engine-crate-map-for-m2.md): engine crate map, modules first; names the seam list | before ENG-F11 is specced |
| 1.3 | [ENG-F9](https://github.com/WrackedFella/moho/issues/234): domain tests catch behaviour changes | carried to ENG-M2 (2.11) |
| 1.15 | [ENG-F22](https://github.com/WrackedFella/moho/issues/167): every test can fail and earns its place (part of 1.3) | waves 1–4 done in ENG-M1; the rest carried to ENG-M2 (2.12) |
| 1.16 | [ENG-F2-02](https://github.com/WrackedFella/moho/issues/95) then [ENG-F2-08](https://github.com/WrackedFella/moho/issues/101): egui's current entry points, then wgpu and egui upgraded | at ENG-M1 close (once-per-milestone upgrade); ENG-F2-08 doesn't compile before ENG-F2-02 |
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
| 6 | [ENG-F22-19](https://github.com/WrackedFella/moho/issues/186) | After 13 and 04; no file shared with 12 or 23, so 12, 19 and 23 run in parallel |
| 7 | [ENG-F22-23](https://github.com/WrackedFella/moho/issues/262) | No dependency (disjoint files from 12 and 19) |

### 1.18 Card order (1.18–1.20)

Two lanes run side by side, and alongside [ENG-F20](https://github.com/WrackedFella/moho/issues/104).
Cards in one lane share files, so each waits on the one before it.

| Lane | Cards in order | Why |
|---|---|---|
| Level | [ENG-F14-01](https://github.com/WrackedFella/moho/issues/266) level meshes load and collide → [ENG-F14-02](https://github.com/WrackedFella/moho/issues/267) named markers | One importer; touches no physics, renderer or binary code |
| Physics and controller | [ENG-F15-01](https://github.com/WrackedFella/moho/issues/268) characters by handle → [ENG-F21-01](https://github.com/WrackedFella/moho/issues/271) walking from commands → [ENG-F15-02](https://github.com/WrackedFella/moho/issues/269) ray first hit → [ENG-F15-03](https://github.com/WrackedFella/moho/issues/270) hit volumes → [ENG-F21-02](https://github.com/WrackedFella/moho/issues/272) first-person camera | 01 changes the character API everything else builds on; the controller goes next so the strategy game's walking moves once. [ENG-F21-02](https://github.com/WrackedFella/moho/issues/272) changes the binary's camera wiring, so it also waits on [ENG-F20-02](https://github.com/WrackedFella/moho/issues/156) if their footprints meet |

## ENG-M2 — Engine MVP

**Intent:** The engine carries both games' current scopes, ranked by what the game
lines ask for.
**Gate:** the engine meets what the games need for a V1 release (cross-line gate:
[roadmap](../roadmap.md#milestone-gates-that-span-lines)). ENG-M2 runs in waves; each wave is planned from the
open engine requests (label `engine-request`) when the one before it nears its end.
Engine requests answered here: #302, #304–#307, #315, #316, #402, #425 (answered by #428), #435. #303 (routing over
changing terrain) is strategy-line work, not an engine item.

**Gate items, all must hold:**
- [ENG-F16](https://github.com/WrackedFella/moho/issues/228),
  [ENG-F17](https://github.com/WrackedFella/moho/issues/229),
  [ENG-F18](https://github.com/WrackedFella/moho/issues/230) and
  [ENG-F19](https://github.com/WrackedFella/moho/issues/231) done.
- **Save:** saves use the [ADR-0006](../../_todo/adr/0006-save-format-contract.md) envelope
  ([ENG-F2-01](https://github.com/WrackedFella/moho/issues/94), done); each game line adds its own save kinds
  ([ADR-0016](../../_todo/adr/0016-game-lines-own-their-save-kinds.md)).

Not gate items, but in ENG-M2: [ENG-F25](https://github.com/WrackedFella/moho/issues/346) (before SG-F4's marks-persist card), ENG-F23, ENG-F24, [ENG-F9](https://github.com/WrackedFella/moho/issues/234),
[ENG-F6](https://github.com/WrackedFella/moho/issues/233) seeded RNG, pause and speed, the bug cards, and the engine's own goals
(graphics and physics are goals in their own right): ENG-F28, ENG-F29, ENG-F31 and
[ENG-F32](https://github.com/WrackedFella/moho/issues/491) movement momentum and footing. Perf criteria are measured on the lowest target GPU, a GTX 1080.

### Engine waves

Items in one wave touch disjoint crates and run in parallel. A later wave starts as
its consumers near, not when the earlier wave fully closes.

| # | Item | Requests | Consumer | Wave | Depends on |
|---|---|---|---|---|---|
| 2.1 | [ENG-F21-03](https://github.com/WrackedFella/moho/issues/330): a character that leaves the ground keeps its momentum (`moho_physics`, reproduce first) | #302 | possession, FPS movement | 1 | — |
| 2.2 | [ENG-F18-01](https://github.com/WrackedFella/moho/issues/331): saved settings survive a later console command (binary, `flagstone_ui`) | #179 comment | strategy settings; FPS via ENG-F18 | 1 | — |
| 2.3 | [ENG-F18-02](https://github.com/WrackedFella/moho/issues/332): menu music plays from `assets/` (`flagstone_ui`) | #186 check | strategy start menu | 1 | — |
| 2.3.1 | [ENG-F18-03](https://github.com/WrackedFella/moho/issues/333): Escape pauses the game in place instead of reloading the world (binary, `flagstone_ui`) | Justin | strategy play; FPS via ENG-F18 | 1 | — |
| 2.4 | [ENG-F16](https://github.com/WrackedFella/moho/issues/228) positional audio, with the `moho_audio` system tests deferred from ENG-F22 (priority) | — | FPS GDD §10; strategy cues | 1 | — |
| 2.5 | [ENG-F6](https://github.com/WrackedFella/moho/issues/233) seeded RNG (reopened): [ENG-F6-01](https://github.com/WrackedFella/moho/issues/335) | #316 | FPS-F2-02 (#299); strategy world gen | 1 | — |
| 2.5.1 | [ENG-F25](https://github.com/WrackedFella/moho/issues/346) a game line adds save files without changing the engine ([ADR-0016](../../_todo/adr/0016-game-lines-own-their-save-kinds.md)): [ENG-F25-01](https://github.com/WrackedFella/moho/issues/347) (priority) | — | SG-F4 marks persist; FPS saves | 1 | before SG-F4's marks-persist card |
| 2.6 | [ENG-F23](https://github.com/WrackedFella/moho/issues/327): the game knows what is under the pointer (new) | #304 | SG-F5 select; SG-F4 mark; SG-F3 place | 2 | — |
| 2.7 | [ENG-F24](https://github.com/WrackedFella/moho/issues/328): the game marks places in the world (new): markers first | #305 | SG-F5, SG-F4, SG-F3 | 2 | — |
| 2.8 | [ENG-F9](https://github.com/WrackedFella/moho/issues/234) domain tests catch behaviour changes (from ENG-M1's 1.3); done (#478) | — | SG-F4 | 2–3 | before SG-F4 |
| 2.9 | [ENG-F11-04](https://github.com/WrackedFella/moho/issues/334): the simulation pauses and runs at 1x, 2x or 3x (`moho_app`) | #307 | SG-F4 follow-up; FPS pause | 3 | SG-F4's skeleton |
| 2.10 | [ENG-F24-03](https://github.com/WrackedFella/moho/issues/329): the game hides the world above a chosen height (renderer) | #306 | SG-F4 underground marks | 3 | ENG-F24's first card |
| 2.11 | [ENG-F18](https://github.com/WrackedFella/moho/issues/230) console (priority): [ENG-F18-05](https://github.com/WrackedFella/moho/issues/403) the FPS opens a console and runs its own commands, then [ENG-F18-06](https://github.com/WrackedFella/moho/issues/404) renderer and sound commands | #402 | FPS-F2-06; FPS feature testing | 2 | #403 after #401 (FPS-F2-03, shares searchlight's game module) |
| 2.11.1 | [ENG-F18-07](https://github.com/WrackedFella/moho/issues/405): a game's own setting survives a relaunch (priority) | #402 | FPS-F2-06 `tgm` | 2 | #354; before #377 and #379 (shared prefs) |
| 2.11.2 | [ENG-F18-08](https://github.com/WrackedFella/moho/issues/406): strategy on the shared console | — | strategy console | 4 | #403, #404 |
| 2.11.3 | [ENG-F18-09](https://github.com/WrackedFella/moho/issues/407): a game draws text on screen from its own crates (pulled forward) | #315 | FPS Phase 2 HUD | 2 | #403; after #376; rebases on #406 if that lands first (`moho_ui_shell` host) |
| 2.11.4 | [ENG-F18-10](https://github.com/WrackedFella/moho/issues/428): a game prints a line in its console after it acts on a command | #425 | FPS-F2-06 `tgm` | 2 | #403 |
| 2.11.5 | [ENG-F18-16](https://github.com/WrackedFella/moho/issues/447): the player sets how often the world autosaves | #435 | SG-F2-03 (#343) | 1–2 gaps | flagstone's read after #343 |
| 2.11.6 | [ENG-F18](https://github.com/WrackedFella/moho/issues/230) menu screens and their stack ([ENG-F18-11](https://github.com/WrackedFella/moho/issues/695)), engine settings controls ([-12](https://github.com/WrackedFella/moho/issues/696)), a game's own settings rows ([-17](https://github.com/WrackedFella/moho/issues/697)), item lists ([-18](https://github.com/WrackedFella/moho/issues/698)); specified by [#444](https://github.com/WrackedFella/moho/issues/444) and [FPS-F3](https://github.com/WrackedFella/moho/issues/457) | #444 | FPS-F3-03 [#460](https://github.com/WrackedFella/moho/issues/460); -17 FPS-F4-03 [#609](https://github.com/WrackedFella/moho/issues/609); -18 FPS-M2 inventory | 4, beside 5 | #695 → #696 → #697 (after #609); #698 after #695; keybind cards held for a rebinding consumer |
| 2.12 | [ENG-F19](https://github.com/WrackedFella/moho/issues/231) layered content | — | FPS Phase 2 | 4 | — |
| 2.13 | [ENG-F17](https://github.com/WrackedFella/moho/issues/229) navigation over static levels (done; cover, exposure and hidden-route queries move to ENG-F42, ENG-M3 row 3.4c) | — | FPS enemy chase and patrol | 4 | — |
| 2.14 | [ENG-F26](https://github.com/WrackedFella/moho/issues/359): one key carries tap, hold, modifier and toggle actions (new; `moho_input`, settings display) | Justin | strategy sprint toggle; FPS reload, check mag, unjam | 4 | before ENG-F18's keybind card |
| 2.15 | [ENG-F27](https://github.com/WrackedFella/moho/issues/361): the game lines are called flagstone and searchlight; one card per line ([#362](https://github.com/WrackedFella/moho/issues/362) FPS, [#363](https://github.com/WrackedFella/moho/issues/363) strategy) | Justin | ENG-F5 split (ENG-M3) | 1–2 gaps | cards done; #362 after #358; #363 after #355, before #331 |
| 2.16 | [ENG-F28](https://github.com/WrackedFella/moho/issues/373): mouse look feels like a Source game (raw, per-frame look; sensitivity, smoothing, acceleration and vsync settings) | Justin | FPS prototype feel; strategy first-person camera | 2 | prefs and settings cards after #331, #333, #354, #405 |
| 2.17 | [ENG-F29](https://github.com/WrackedFella/moho/issues/479): light reads true in caves, at night and under open sky (light composition; not a gate item) | — | SG-F7-05 and #474 checks; searchlight arena ambient | 2 | -01 lands with SG-F7-05; -02 after #469; -03 and -04 follow; -04 then [-05 bloom](https://github.com/WrackedFella/moho/issues/503) by the end of ENG-M2; [-06 auto-exposure](https://github.com/WrackedFella/moho/issues/504) opens ENG-M3, tuned once beside ENG-F30 |
| 2.18 | [ENG-F31](https://github.com/WrackedFella/moho/issues/481): mirrors and water reflect the world (mirrors, SSR, water; not a gate item) | Justin (must-have) | searchlight arena mirror and pool | 4 | after ENG-F29-04; its plane declaration is a new seam (ADR), settled before the ENG-F5 split gate |
| 2.19 | [ENG-F32](https://github.com/WrackedFella/moho/issues/491): movement keeps its momentum and its footing (Source `accelerate`, half-step gravity, shape-cast ground and stick-to-floor, crease clip, queued effects with the ADR superseding [ADR-0014](../../_todo/adr/0014-characters-move-by-velocity.md), jump re-press, one gravity, hull as game data, replay determinism test; not a gate item) | Justin | searchlight feel; flagstone possession; ENG-F33, ENG-F34 and the ENG-M3 ADR-0011 proof build on it | 2–3 | [#494](https://github.com/WrackedFella/moho/issues/494), [#495](https://github.com/WrackedFella/moho/issues/495), [#496](https://github.com/WrackedFella/moho/issues/496) first (one TL thread, shared `movement.rs`), then [#497](https://github.com/WrackedFella/moho/issues/497), then the rest; `moho_physics` only, so it runs beside every renderer and UI item |
| 2.20 | [ENG-F38](https://github.com/WrackedFella/moho/issues/634): meshes move without being uploaded again (transform per mesh handle, instances; amends seam 2) | Searchlight list 2.4 | enemies, loose objects, models, effects | 5 | first in wave 5 |
| 2.21 | [#600](https://github.com/WrackedFella/moho/issues/600): loose objects fall, tumble, settle and are picked up | #600 | FPS-F9-04 | 5 | 2.20 |
| 2.22 | [ENG-F39](https://github.com/WrackedFella/moho/issues/635) slices 1 to 3: models with named nodes and textured materials, sockets, rig and clips with sim-readable bone poses | [#543](https://github.com/WrackedFella/moho/issues/543); list 2.6, 2.12, 2.13 | FPS-F12-01, FPS-F4 slice 3, FPS-F12-02 | 5 | 2.20; beside 2.21 |
| 2.23 | [ENG-F40](https://github.com/WrackedFella/moho/issues/636) slice 1: muzzle flash, tracer and impact marks expire on their own | list 2.9 | FPS-M2 gunplay feel | 5 | 2.20 |
| 2.24 | [ENG-F14-03](https://github.com/WrackedFella/moho/issues/639): markers carry their size | list 2.2 | FPS-F10 test levels, FPS-M5 safe rooms | 5 | — |
| 2.25 | [ENG-F43](https://github.com/WrackedFella/moho/issues/642): characters crouch, go prone and lean, with clearance (hit volumes the game poses, crouch with a clearance query, prone through crouch, lean; answers [#544](https://github.com/WrackedFella/moho/issues/544) and takes crouch from [ENG-F33](https://github.com/WrackedFella/moho/issues/492)) | #544 | FPS-M1.2 slice 2 | 5 | ENG-F32 through [#501](https://github.com/WrackedFella/moho/issues/501) and [#502](https://github.com/WrackedFella/moho/issues/502); one chain with ENG-F32's last cards: #497 → #498 → #499 → #500 → #643 → #501 → #502 → #644 → #645 → #646 |

- **Wave 1:** the bugs, ENG-F16, ENG-F6 and ENG-F25. They need no other ENG-M2 work.
- **Wave 2, before SG-F5 and SG-F4:** ENG-F23 and ENG-F24 unblock SG-F5; ENG-F9 comes before SG-F4.
  ENG-F28 (mouse feel) runs alongside; its settings cards queue behind the wave-1 prefs cards and ENG-F18-07.
  The ENG-F18 console (2.11, 2.11.1, 2.11.4) is pulled into wave 2 so the FPS can test features with it,
  and on-screen text (2.11.3) so the FPS HUD doesn't wait for the rest of ENG-F18.
- **Wave 3, SG-F4 follow-ups:** pause and speed, then the height slice, once SG-F4's walking skeleton exists.
  ENG-F32 (2.19) spans waves 2 and 3 in `moho_physics`, which nothing else in ENG-M2 touches; its API cards
  (#494–#497) go first because every later physics card would otherwise add callers.
- **Wave 4, before FPS Phase 2:** the rest of ENG-F18 (2.11.2 and 2.11.6), ENG-F19, ENG-F17 and ENG-F26.
  2.11.6 runs beside wave 5: its prerequisites are done and it touches `moho_ui_shell` and `searchlight`,
  not the physics or renderer crates wave 5 is in. It includes the inventory list, two panes and tooltips
  from #444's 2026-10-23 comment; ENG-F18's keybind capture waits for a rebinding consumer.
- **Wave 5, during FPS Phase 2:** what FPS-M2 needs after its entry (FPS Phase 2 is in ENG-M2's cross-line gate).
  ENG-F38 goes first, since loose objects, models and effects draw through it; then #600 and ENG-F39 in parallel.
  #543 moves here from ENG-M5+, and #544 with ENG-F33's crouch as ENG-F43, as the FPS line asked (2026-10-23).

### Out of ENG-M2

| Idea | Why it waits | Revisit when |
|---|---|---|
| A wall impact every tick (damage needs a threshold) | No consumer | The first impact-damage rule |
| Gamepad limits and turn speed (#151) | Closed, no request | A gamepad request |
| Shared A* between strategy routes and ENG-F17 | Neither exists yet | Both exist |
| Source-style baked FPS lighting tier | A second pipeline for one line | A measured FPS interior quality gap, or a target GPU that can't afford the dynamic path |
| DDGI-style probes | Leaks and blur aren't measured yet | ENG-F30 shows unacceptable leaks or blur |
| Box-projected room probes; Hi-Z, stochastic and temporal SSR | SSR cost, noise and indoor misses not measured | SSR exists and misses or noise look wrong |
| One global Low/Medium/High graphics preset | Too few Video rows yet | Four or more renderer quality rows (after ENG-F30) |
| Auto-exposure ([ENG-F29-06](https://github.com/WrackedFella/moho/issues/504)) | Tuned against the scene's energy, which ENG-F30 changes; doing it first means tuning twice | Start of ENG-M3, beside the ENG-F30 skeleton |
| Movement rules as data: modes, braking split, jump assists, air control, surface friction ([ENG-F33](https://github.com/WrackedFella/moho/issues/492); crouch and the clearance query are [ENG-F43](https://github.com/WrackedFella/moho/issues/642), wave 5, row 2.25) | Builds on ENG-F32's step order; a feature on its own | ENG-M3 |
| Characters and bodies move each other: push, weight, moving ground, char-vs-char, lookahead, contact events, CCD ([ENG-F34](https://github.com/WrackedFella/moho/issues/493)) | The biggest physics effort; needs ENG-F32's ground collider and effects | ENG-M3 |
| Fire-time fraction (Q1) and sub-tick jump (Q2, last, only if Q1 proves useful) | Q2 changes the step's shape; neither is needed for feel at one fixed rate | ENG-M5+ |
| Netcode ADR and a co-op session | Co-op for up to 4 players on both lines; ADR-0011, ENG-F32-04 and -09 and the per-slot tick input (ENG-F37, 3.4a) keep it open at near-zero cost | ENG-M5+, pulled by the first co-op GDD |
| Engine perception (sight, hearing, factions) | One consumer | Strategy pawns fight |
| AI and nav debug draw | No request | First AI or nav debugging pain on either line |
| Per-tick AI budget | Not measured | Cost at 50 pawns or Director handover |

## ENG-M3 — Seams land, then v1.0 and split

**Intent:** The engine's remaining seams land, then the boundary is settled and versioned.
A seam added after the split crosses three repos, so the features that add one come first
(sweeping changes first).
**Gate (all must hold):**
- The [ENG-F5](https://github.com/WrackedFella/moho/issues/232) gate: licences and the third-party
  notices file; the seam list named in [ADR-0012](../../_todo/adr/0012-engine-crate-map-for-m2.md) is unchanged by the last two
  FPS features; the `v1.0` tag.
- The seam list includes the ENG-F31 plane declaration, the ENG-F30 volume feed, the ENG-F35
  light-model seam and ENG-F34's collision policy and contact events.
- [ADR-0011](../../_todo/adr/0011-simulation-stays-network-ready.md) holds: two headless simulations fed the same recorded command and
  effect stream stay bit-equal over a session (ENG-F32-09 grown by one test; no transport).

Carded when ENG-M2 closes, in this order of dependency; none is a gate item on its own:

| # | Item | After | Note |
|---|---|---|---|
| 3.1 | [ENG-F29-06](https://github.com/WrackedFella/moho/issues/504) exposure adapts to the scene | #485 | Opens ENG-M3 so it is tuned once against the final energy |
| 3.2 | [ENG-F30](https://github.com/WrackedFella/moho/issues/480) light bounces off what's around it: skeleton on strategy terrain, then sun, emissive, FPS voxelisation | #485; GPU named (GTX 1080, 2 ms at 1080p) | Researched when carded; adds the volume-feed seam |
| 3.2.1 | [ENG-F35](https://github.com/WrackedFella/moho/issues/509) the sim knows how lit a place is (FPS stealth; direct light from sim data through rays, ADR-0011) | — (slice 4, a coarse bounce grid fed by static lights so a bounce-lit room reads as lit, after ENG-F30's FPS voxelisation slice) | Adds the light-model seam (own ADR in its first card); carded with ENG-M3 |
| 3.3 | [ENG-F33](https://github.com/WrackedFella/moho/issues/492) movement rules are game data | ENG-F32 | Modes, unstick, braking split, jump assists, air control, surface friction (crouch is ENG-F43, row 2.25) |
| 3.4 | [ENG-F34](https://github.com/WrackedFella/moho/issues/493) characters and bodies move each other | ENG-F32-03, -04 | Absorbs ENG-F21's deferred props row; adds the collision-policy and contact-event seams |
| 3.4a | [ENG-F37](https://github.com/WrackedFella/moho/issues/592) the tick takes one command per player slot (`moho_app::Game`, seam 1 in ADR-0012) | — | Changes an existing seam, so it lands before the split; games keep reading slot 0 |
| 3.4b | [ENG-F39](https://github.com/WrackedFella/moho/issues/635) slices 4 to 6: blends, layers, sim-timed and interruptible clips; bone-to-bone and detach; first-person view | 2.22 | FPS-F8, FPS-M4 writhe, FPS-M3.1 |
| 3.4c | [ENG-F42](https://github.com/WrackedFella/moho/issues/638) AI asks the level for hidden routes and cover (extends seam 11) | ENG-F17 | Early: before FPS-F11-03 (#630) |
| 3.4d | [ENG-F40](https://github.com/WrackedFella/moho/issues/636) slices 2 and 3: beam and dot, then particles | 2.23; R.8a for slice 3 | FPS-M3.1 laser; smoke and bursts |
| 3.4e | [ENG-F41](https://github.com/WrackedFella/moho/issues/637) screen and sound follow game values | R.3 | Before FPS-M4 |
| 3.4f | Level unload and switch (ENG-F14's Deferred row) | — | FPS-M6 is the named consumer; lands before the split so scene import (seam 8) doesn't change across repos |
| 3.4g | #444's drag-and-drop grid | 2.11.6 | After the FPS list view is playtested |
| 3.4h | Strategy's Video, Audio and mouse settings rows on the shell's controls (ENG-F18 Deferred) | 2.11.6 (-12) | One implementation of the engine's own settings before the split; strategy keeps its own screens and HUDs |
| 3.5 | ADR-0011 two-sim proof | ENG-F32-09, 3.4a | One test that records per-slot commands; before any netcode ADR |
| 3.6 | [ENG-F5](https://github.com/WrackedFella/moho/issues/232) split | 3.2–3.5 (with 3.4a to 3.4h), ENG-F35; FPS-M5 complete | Gate; the later of this row's items and FPS-M5 completion ([roadmap](../roadmap.md#milestone-gates-that-span-lines)) |

The global graphics preset (Out of ENG-M2) is revisited once 3.2 lands and four or more Video rows exist.

## ENG-M4 — Production-ready

**Intent:** A build could ship to players.
**Gate:** defined when ENG-M3 closes. Likely: per-platform perf budgets (measured on the
lowest target GPU, a GTX 1080), crash reporting, packaging and installers, a
save-compatibility promise, all-OS CI on every PR, one global graphics preset.

## ENG-M5+ — Engine depth

**Intent:** Features found in mature engines, each pulled by a named consumer.
**Gate:** each is its own feature, in no set order: in-engine editor,
rendering options (forward+/deferred, quality tiers), skeletal animation
tooling, scripted-mods ADR, netcode ADR and a co-op session (transport, trusted host,
join-in-progress from the save envelope; co-op for up to 4 players, pulled by the first co-op GDD),
fire-time fraction then sub-tick jump timing (Q1, Q2; Q2 last and only if Q1 proves
useful), further movement modes (ladder, swim, slide, wall-run, mantle) as a game
writes them, and the triggered graphics items in the Out of ENG-M2 table (DDGI probes,
baked FPS tier, Hi-Z and temporal SSR, rough reflections from the GI volume, waves).

Guardrail until then: don't close the door. Scenes stay data (an editor
edits data); ADR-0011's rules cover netcode, and after ENG-F32-04 every change to a
character's state is a recorded command or effect. Voxel ids that still come from
process-wide counters (ADR-0011 rule 3) are fixed where SG-F4 and SG-F7 rework that code.

### Research queue

Each topic is researched before it is carded, in this order (R.8a, split from R.8, runs after R.3). A topic ends in findings and, where it changes a seam, an
ADR; none is a gate item. The lowest target GPU (a GTX 1080) is a floor, never a filter: an option it cannot run is
recorded as an optional high-end tier instead of being dropped.

| # | Topic | Question | Lands in |
|---|---|---|---|
| R.1 | Netcode for host-authoritative co-op | Which model fits ADR-0011: host runs AI, actors replicate behaviour to clients | Netcode ADR, ENG-M5+ |
| R.2 | Determinism | What bit-equality costs across platforms (rapier `enhanced-determinism`, float order) beyond the ENG-M3 two-sim proof | Feeds R.1 |
| R.3 | Audio | Whether rodio carries occlusion, reverb, spatial mixing, per-sound handles and a filtered mix bus, or a mixer sits above it | Before ENG-F41 |
| R.8a | Particles and decals | CPU or GPU particles, decal method, cost on a GTX 1080 | Before ENG-F40 slice 3 |
| R.4 | Mod scripting | WASM, Lua or data-only extension; sandbox and API surface | Scripted-mods ADR, ENG-M5+ |
| R.5 | Asset pipeline and streaming | Import, bake, hot reload, async load at level scale | Before ENG-M4 packaging |
| R.6 | Strategy-scale pathfinding | Flow fields and hierarchical A* for many pawns; shared with ENG-F17 or separate | When the shared-A* idea row fires |
| R.7 | Voxel terrain at scale | Transvoxel-style seams and LOD beyond the current chunk set | Flagstone line request |
| R.8 | Post-processing | Temporal anti-aliasing, upscaling, shadows beyond cascades, volumetrics | ENG-M4 |

### Optional high-end tier

Hardware ray tracing (for example Bevy Solari) is an optional tier above the dynamic GI path, never a requirement. The
GTX 1080 floor keeps the software path (ENG-F30) as the default; the tier is carded after ENG-M2 and sized as ENG-M3 or
ENG-M4 work once ENG-F30 shows what the software path leaves on the table.

## Backlog (opportunistic, when touching the area)

[ENG-F1-01](../../_todo/engine/ENG-F1-engine-hygiene/ENG-F1-01-constructor-size-cleanup.md)/[02](../../_todo/engine/ENG-F1-engine-hygiene/ENG-F1-02-error-handling-backlog.md)/[03](../../_todo/engine/ENG-F1-engine-hygiene/ENG-F1-03-god-module-splits.md)/[05](../../_todo/engine/ENG-F1-engine-hygiene/ENG-F1-05-material-model-revisit.md)/[07](../../_todo/engine/ENG-F1-engine-hygiene/ENG-F1-07-lint-ratchet.md), [ENG-F3-01](../../_todo/engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-01-bind-group-split.md)/[02](../../_todo/engine/ENG-F3-renderer-pipeline-cleanup/ENG-F3-02-frustum-culling-sorting.md), [ENG-F4](../../_todo/engine/ENG-F4-terrain-rendering-debt/_feature.md)-*,
[ENG-F1-08](../../_todo/engine/ENG-F1-engine-hygiene/ENG-F1-08-headless-app-tests.md) (headless app wiring tests),
[ENG-F1-10](https://github.com/WrackedFella/moho/issues/246) (key capture stops listening on a conflict; pair with [#392](https://github.com/WrackedFella/moho/issues/392), which collapses the keybind test tiers, ex ENG-F1-04).

## Engine rules

Standing rules that order the work above. Each links its record.

**Structure and dependencies**
- Crates belong to one line; dependencies point game to engine; `just check` enforces it ([ADR-0005](../../_todo/adr/0005-crate-lines-and-dependency-direction.md)).
- Entities live in typed stores; `hecs` arrives when a consumer needs an ECS ([ADR-0004](../../_todo/adr/0004-entity-storage-without-a-general-ecs.md)).
- The engine crate map is decided in an ADR, modules first ([ADR-0012](../../_todo/adr/0012-engine-crate-map-for-m2.md)). The shared app loop is an engine crate ([ENG-F11](https://github.com/WrackedFella/moho/issues/81)); input is game-defined actions ([ENG-F12](https://github.com/WrackedFella/moho/issues/82)), with modifier chords returning through [ENG-F26](https://github.com/WrackedFella/moho/issues/359), where hold timing counts ticks.
- The engine sees world geometry as meshes; voxels live in a strategy-line crate ([ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md), which supersedes ADR-0002 and narrows ADR-0003 to engine-wide events).
- Map format is glTF 2.0; the importer is an engine feature and the map loader belongs to the line ([ENG-F14](https://github.com/WrackedFella/moho/issues/226)).
- A generic character controller and camera are an engine feature ([ENG-F21](https://github.com/WrackedFella/moho/issues/123)).
- The lines are named flagstone and searchlight inside the monorepo; engine crates keep `moho_`, and SG-/FPS- IDs stay ([ENG-F27](https://github.com/WrackedFella/moho/issues/361)).
- The repo split gate (ENG-M3) requires an agnostic engine and a settled second consumer ([ENG-F5](https://github.com/WrackedFella/moho/issues/232)).

**Simulation, saves and multiplayer**
- Simulation time comes from one fixed 1/60 s tick, with no separate clock ([ADR-0009](../../_todo/adr/0009-simulation-time-is-one-fixed-tick.md)); the tick criterion belongs to ENG-F11, and [ENG-F6](https://github.com/WrackedFella/moho/issues/233) holds seeded RNG.
- The simulation stays network-ready ([ADR-0011](../../_todo/adr/0011-simulation-stays-network-ready.md)). Multiplayer is co-op on both lines for up to 4 players, with a trusted host that runs the AI; a casual competitive mode on private servers stays possible and nothing is built for it. No design may close either. The tick takes one command per player slot before the split (ENG-F37, 3.4a). Its enablers are ENG-F32-04 and -09, the per-slot tick input and the ENG-M3 two-simulation proof; the netcode ADR and a co-op session are ENG-M5+.
- Saves use a postcard envelope; old saves break and no compatibility is promised before v1.0 ([ADR-0006](../../_todo/adr/0006-save-format-contract.md)). Game lines own their save kinds; the engine keeps only the envelope framing ([ADR-0016](../../_todo/adr/0016-game-lines-own-their-save-kinds.md)).
- Look turns every frame ahead of the tick that consumes it, raw by default, with its settings in the same feature ([ENG-F28](https://github.com/WrackedFella/moho/issues/373)).

**Dependencies and licences**
- Third-party licences allow closed-source commercial sale ([ADR-0007](../../_todo/adr/0007-third-party-licence-policy.md)). winit stays, with the Wayland title bar dropped to clear `ttf-parser` ([ADR-0008](../../_todo/adr/0008-keep-winit-for-windowing-and-input.md)).
- No dependency is assumed. The audit gives a keep, replace, fork, homebrew or drop verdict per crate and large well-maintained crates are green-lit; follow-ups go under ENG-F2 or the feature reworking that code ([ENG-F13](https://github.com/WrackedFella/moho/issues/83)). `noise`, `crossbeam-channel`, `pollster`, `phf` and `log`/`env_logger` leave; `ini` stays; ENG-F16 uses `rodio` with its own gain and pan ([audit](../../_todo/engine/ENG-F13-dependency-audit/audit.md)).
- Fast-moving stacks (wgpu/egui, rapier/glam) upgrade once per milestone close, behind the containment in [ENG-F20](https://github.com/WrackedFella/moho/issues/104). The `paste` ignore sits with ENG-F2-04 ([ENG-F8](https://github.com/WrackedFella/moho/issues/61), G3).

**Content, console and UI**
- Mods are data-only (layered roots, override by stable id); scripted mods get their own ADR later ([ENG-F19](https://github.com/WrackedFella/moho/issues/231)). Saves record the active root list once a game both saves and loads content.
- A game inherits the engine's console commands and adds its own; a taken name is refused. Game settings persist in each game's own prefs file ([ENG-F18](https://github.com/WrackedFella/moho/issues/230)). A game describes its menu screens; the engine draws and stacks them and supplies its own settings as ready-made controls ([#444](https://github.com/WrackedFella/moho/issues/444)). Keybind capture waits for a rebinding consumer.

**Graphics and physics**
- Lowest target GPU is a GTX 1080; every renderer perf criterion is measured on it at 1080p. It is a floor, never a filter.
- Lighting is one dynamic engine path; baked lightmaps exist only as a line-specific tier on a measured gap. Every effect has a player-facing on/off and detail setting in the Video tab ([ENG-F29](https://github.com/WrackedFella/moho/issues/479), [ENG-F30](https://github.com/WrackedFella/moho/issues/480), [ENG-F31](https://github.com/WrackedFella/moho/issues/481)).
- Game AI stays in the lines; the engine offers queries (navigation, rays, gameplay light). Gameplay light reaches the simulation through ENG-F35.
- Movement follows Source `accelerate` (reversing #271 deviation 1) with overbounce 1.0; queued effects supersede [ADR-0014](../../_todo/adr/0014-characters-move-by-velocity.md) in one card ([ENG-F32](https://github.com/WrackedFella/moho/issues/491)). Movement rules as data ([ENG-F33](https://github.com/WrackedFella/moho/issues/492)) and characters and bodies ([ENG-F34](https://github.com/WrackedFella/moho/issues/493)) are ENG-M3.

**Planning**
- Milestones carry gates and only the next one is carded ([`_STANDARDS.md`](../../_todo/_STANDARDS.md#5-milestones)).
