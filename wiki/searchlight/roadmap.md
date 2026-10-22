# Searchlight roadmap

The FPS line's milestones, in order. The phases and their questions are in the [GDD](game-design-document.md) §7; this page adds each milestone's entry gate, exit goal and cut lines. Engine items are numbered as in the [engine roadmap](../engine/roadmap.md). Item status lives on the [project board](https://github.com/users/WrackedFella/projects/1); this page holds order, not status.

**Rule from the GDD:** each phase becomes a feature only after the previous one is approved. So this plan fixes milestones and exit goals, not cards. Only FPS-M1 and FPS-M1.1 have cards.

Milestone names follow ISO 2145: FPS-M*n* delivers GDD Phase *n*; FPS-M1.1 and FPS-M1.2 are bridges.

## 1 Overview

| Milestone | GDD phase | Question it answers | Engine gate | State (2026-10-22) |
|---|---|---|---|---|
| FPS-M1 Arena baseline | 1 | Does the gunplay feel weighty? | none hard; stopgaps in use | All cards merged; Justin's waves and score playtest is the exit |
| FPS-M1.1 Menus (FPS-F3) | 1 to 2 bridge | Can a player start, pause and tune the game without the console? | engine request #444 (ENG-F18) | Feature #457; cards #458 to #460 await build, #512 mouse look merged |
| FPS-M1.2 Stance and body (FPS-F4) | 1.2 | Does posture change where I stand and when I peek? | #544 with ENG-F33; #543 for the body | Direction agreed; no cards |
| FPS-M2 Items and loadout | 2 | Does one item model carry everything? | ENG-F18-09 #407, ENG-F19 #231, ENG-F26 #359 | Not started |
| FPS-M3 Wear, repair, ledger | 3 | Does wear drive scavenging? | none new | Not started |
| FPS-M4 Wounds and healing | 4 | Does limb damage create good downtime? | none new | Not started |
| FPS-M5 Time, survival, rest | 5 | Do survival stats add pressure, not chores? | game clock scaling; save and load | Not started |
| FPS-M6 Hub and jobs | 6 | Does profit-as-score change behavior? | level switching; save and load | Not started |
| FPS-M7 Director | 7 | Does the world feel alive on little machinery? | headless sim; navigation | Not started |

Order is the GDD's: systems arrive in the order they create costs, so the ledger has something to measure. Enemy AI lands in slices alongside the phases (GDD §7).

## 2 Features

| # | Feature | Engine items it needs |
|---|---|---|
| P.1 | [FPS-F1](../../_todo/searchlight/FPS-F1-game-design-document/_feature.md) [GDD](game-design-document.md) v0.3 | none |
| P.2 | [FPS-F2](https://github.com/WrackedFella/moho/issues/297) arena baseline (GDD §7 Phase 1), with the rules crate `searchlight_rules` ([ADR-0015](../../_todo/adr/0015-fps-rules-are-a-platform-free-crate.md)) | none; stopgaps: non-positional sound, a rectangle HUD, a local seeded generator. Navigation (ENG-F17-01 and -02) has landed for chase and patrol |
| P.3 | GDD §7 Phase 2 (items and loadout) | 2.11.3 on-screen text, 2.12 layered content, 2.14 tap and hold keys |

## 3 FPS-M1 Arena baseline (FPS-F2 #297)

- **Exit goal:** a player fights escalating waves in the baked arena, dies and restarts, with weighty rifle feel and per-body-part damage. Justin playtests and approves; that opens FPS-M2, enemy grades slice 1 and FPS-M1.2 slice 1.
- **Cut lines (decided):** per-enemy routes, wave banners, pauses, high scores, crouch, physics projectiles.
- **Known gaps accepted:** non-positional sound (ENG-F16 #228), title-bar HUD instead of text (#315 / ENG-F18-09 #407), local seeded RNG instead of #316 / ENG-F6-01 #335.
- **TL risk:** the title-bar HUD stops being enough once FPS-M2 needs inventory. That gates FPS-M2, not FPS-M1.

## 4 FPS-M1.1 Menus (FPS-F3 #457)

- **Outcome:** main menu at launch, pause menu on Escape, settings screen (mouse, volume, graphics, window). Cards #458 main menu, #459 pause, #460 settings with Apply; build in that order.
- **Entry gate:** feature approved; engine request #444 answered. Specs can run in parallel with FPS-M1; building waits for it (shared game loop).
- **Defaults:** pause stops the sim; no return to main menu; no save or continue; key rebinding waits for ENG-F26 #359. Settings are staged, Apply persists, Back discards.
- **Cut lines:** key rebinding, return to main menu (FPS-M6), save and continue (FPS-M5), reset and apply-all buttons, backdrop, gamepad.

## 5 FPS-M1.2 Stance and body (FPS-F4)

Runs in slices parallel to FPS-M2; needs no items or UI beyond one slider in the FPS-F3 settings screen.

| Slice | Content | Needs from the engine |
|---|---|---|
| 1 Lean and head bob | Lean with clearance, hit volumes follow the lean, head-bob camera and slider | nothing new |
| 2 Crouch and prone | Both stances with timed transitions and speed rules; prone's oriented hull; hit volumes and enemy aim follow the stance | ENG-F33 #492 and #544 (ENG-M3) |
| 3 Visible body | Rigged first-person body and shadow; enemies on the same rigs; hit volumes ride the bones | #543 skinned characters (ENG-M5+ unless pulled forward) |

- **Exit:** in the arena a player crouched behind the 1.4 m crates is hit only when leaning, on head or arm; a prone player behind the same crate is not seen at all.
- **Cut lines:** weapon collision against the ground, prone dive, enemies going prone, variable crouch height, side-step, blind fire, mantle, slide, vault.
- **Held actions:** hold or toggle per keybinding lands with ENG-F26; until then lean is hold and crouch is toggle.

## 6 FPS-M2 Items and loadout (GDD Phase 2)

- **Outcome:** the player carries a rig and a backpack, enemies drop loot through their bodies, and three weapons in two calibers come from one data-driven item model. Magazines are items holding an ordered stack of rounds, mixed types allowed.
- **Scope notes (GDD §4.1, §4.12):** item size and container cells in the first data file with a list view over them; three item classes; armor slots present with no items; the stat layer is born here with carry capacity, movement speed and stamina; a weight readout.
- **Entry gate:** FPS-M1 approved. Engine: on-screen text (#407), layered content (#231), tap and hold keys (#359).
- **Exit goal:** one item model carries a weapon, a magazine, rounds and a consumable with no special-case code; a data file adds an item without code.
- **Cut lines:** the grid view (after the list is played, GDD §9 Q17), stash, a dedicated mixing UI, rarity or affixes.
- **Risk:** the largest scope jump. Split in two features if the first card set is too big: (1) item model and loot, (2) inventory UI. The Tech Lead decides.

## 7 FPS-M3 Wear, repair, ledger (Phase 3)

- **Outcome:** four components per weapon with condition, jams, repair parts, a workbench, dismantling, limited-use cleaning kits, armor items with wear, the toolkit, and an end-of-round ledger valuing rounds and wear.
- **Entry gate:** FPS-M2 approved. Needs a second baked map for a range (authoring, not engine).
- **Exit goal:** after a fight the ledger shows a cost, and a player chooses to scavenge for parts because of it.
- **Cut lines:** more than four components, per-weapon unique parts, attachments and ammo crafting (GDD "Later").
- **Enemy AI slice 2** (group alerts, leader lift, fire and maneuver) lands before this milestone.

## 8 FPS-M4 Wounds and healing (Phase 4)

- **Outcome:** the §4.7 first cut: damage-scaled penalties, leg compounding, bleeding with bandages, first and secondary aid, splint, perishable meds, recovery timers on a minimal game clock, an inspect screen.
- **Exit goal:** a leg or arm wound changes what the player does next, and treating it costs time and items.
- **To taste, in order:** enemy debuffs, painkiller masking, tourniquet, temporary head statuses, arm compounding and left or right arms, surgical kit and med crafting.
- **Cut lines:** radiation; disease and wasting injuries until the core proves out.

## 9 FPS-M5 Time, survival, rest (Phase 5)

- **Outcome:** game clock scaling and skipping, hunger, thirst, fatigue, sleep at a safe room with healing timers advancing.
- **Entry gate:** game clock scaled over the tick (GDD §10 item 8; pause and speed #334), save and load for world state (§10 item 9; ENG-F25 #346).
- **Decisions due:** GDD §9 Q8 (difficulty as complexity toggles) and Q2 (co-op timing).
- **Cut lines:** temperature, disease, spoilage.

## 10 FPS-M6 Hub and jobs (Phase 6)

- **Outcome:** trader hub, job board, currency, full per-job ledger; the arena becomes the job site and waves and score retire from the default experience.
- **Entry gate:** level switching carrying player state (§10 item 10), save and load. Needs a hub and one or two zone maps.
- **Decisions due:** GDD §9 Q1 (setting), Q3 (pivot point), Q15 (starting backgrounds).
- **Natural v1 candidate:** the engine roadmap's ENG-M3 gate wants the seam list unchanged by the last two FPS features; this is the earliest playable game rather than a prototype.

## 11 FPS-M7 Director (Phase 7)

- **Outcome:** zone graph, factions, offline dice sim, daily planning, live handover; groups as the Director's unit.
- **Entry gate:** headless sim (§10 item 11, ENG-F6 #233 related), navigation for live actors.
- **Cut lines:** dialogue and diplomacy, emergent quests, more than about five zones. Highest pivot risk, so last.

## 12 Engine requests from this line

Flagged to Justin; the Engine project designs them.

| Request | State (2026-10-22, confirm on the board) | Needed by |
|---|---|---|
| #444 menus and settings controls | answered by ENG-F18 settings screen (2.11.6) | FPS-M1.1 build |
| #315 on-screen text | answered by ENG-F18-09 #407, not built | FPS-M2 entry |
| #316 seeded replayable RNG | open; ENG-F6-01 #335 | before FPS-M2 if it matters |
| ENG-F16 #228 positional audio | open | enemy AI slice 2 (gunshots alert groups) |
| ENG-F17 #229 navigation and cover queries | navigation landed (ENG-F17-01, -02); cover and exposure queries open | enemy AI slice 2 |
| ENG-F19 #231 layered content | open | FPS-M2 entry |
| ENG-F26 #359 key actions | open | FPS-M2; held-action toggles |
| #543 skinned animated characters | open | FPS-M1.2 slice 3 |
| #544 lean and stance transitions, prone hull | open | FPS-M1.2 slice 2 |

New requests come from FPS-M3 onward (workbench interaction, save kinds, level switching). File them when a milestone's feature is drafted.

## 13 Next steps

1. Playtest and approve FPS-M1; record the result.
2. Ask the Engine project to schedule ENG-F18-09, ENG-F19 and ENG-F26, which gate FPS-M2.
3. Open FPS-M2 as a feature proposal via `/devflow:business-analyst`; start the enemy grades slice 1 and FPS-M1.2 slice 1 in parallel.

## 14 Decisions

| Date | Decision | Record |
|---|---|---|
| 2026-10-19 | Milestone plan approved: one milestone per GDD phase, cards only for the next | this page |
| 2026-10-21 | Milestones are FPS-M*n* with ISO 2145 numbering; M-A to M-G retired | [`_STANDARDS.md`](../../_todo/_STANDARDS.md) |
| 2026-10-22 | Roadmaps live in the wiki under their line; `_todo/ROADMAP.md` is an index | [roadmap index](../../_todo/ROADMAP.md) |
