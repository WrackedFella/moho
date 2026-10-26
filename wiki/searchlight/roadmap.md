# Searchlight roadmap

The FPS line's milestones, in order. The phases and their questions are in the [GDD](game-design-document.md) §7; this page adds each milestone's entry gate, exit goal and cut lines. Engine items are numbered as in the [engine roadmap](../engine/roadmap.md); dependencies on other lines are in the [roadmap](../roadmap.md). Item status lives on the [project board](https://github.com/users/WrackedFella/projects/1); this page holds order, not status.

**Rule from the GDD:** each phase becomes a feature only after the previous one is approved. So this plan fixes milestones and exit goals, not cards. Only FPS-M1 and FPS-M1.1 have cards.

Milestone names follow ISO 2145: FPS-M*n* delivers GDD Phase *n*; FPS-M1.1 and FPS-M1.2 are bridges.

## 1 Overview

| Milestone | GDD phase | Question it answers | Engine gate |
|---|---|---|---|
| FPS-M1 Arena baseline | 1 | Does the gunplay feel weighty? | none hard; stopgaps in use |
| FPS-M1.1 Menus (FPS-F3) | 1 to 2 bridge | Can a player start, pause and tune the game without the console? | engine request #444, answered by ENG-F18-11 #695, -12 #696, -17 #697 |
| FPS-M1.2 Stance and body (FPS-F4) | 1.2 | Does posture change where I stand and when I peek? | #544 with ENG-F33 #492; #543 for the rig; all by FPS-M2 |
| FPS-M2 Items and loadout | 2 | Does one item model carry everything? | ENG-F18-09 #407, ENG-F18-18 #698 item lists (base for the inventory and search screens), ENG-F19 #231, ENG-F26 #359; #600 loose objects (FPS-F9); #543 for reload animation (FPS-F8) |
| FPS-M3 Wear, repair, ledger | 3 | Does wear drive scavenging? | none new |
| FPS-M3.1 Attachments | 3.1 | Can a weapon be rebuilt in the field without special cases? | sockets, zoom and reticle, beam and dot (engine requests) |
| FPS-M4 Wounds and healing | 4 | Does limb damage create good downtime? | none new |
| FPS-M4.1 Grenades (mocked) | 4.1 | Does a throwable fit the item model? | none beyond runtime spawn and positional audio |
| FPS-M5 Time, survival, rest | 5 | Do survival stats add pressure, not chores? | game clock scaling; save and load |
| FPS-M6 Hub and jobs | 6 | Does profit-as-score change behavior? | level switching; save and load |
| FPS-M7 Director | 7 | Does the world feel alive on little machinery? | headless sim; navigation |

Order is the GDD's: systems arrive in the order they create costs, so the ledger has something to measure. Enemy AI lands in slices alongside the phases (GDD §7).

**Prototype gate:** FPS-M1 through FPS-M5 with FPS-M1.2, FPS-M3.1 and FPS-M4.1, group tactics, placeholder art and animation, and test levels with scripted states ([section 13](#13-prototype-gate)). The systems are the deliverable; attachment and grenade items are mocked.

## 2 Features

| # | Feature | Engine items it needs |
|---|---|---|
| P.1 | [FPS-F1](../../_todo/searchlight/FPS-F1-game-design-document/_feature.md) [GDD](game-design-document.md) v0.3 | none |
| P.2 | [FPS-F2](https://github.com/WrackedFella/moho/issues/297) arena baseline (GDD §7 Phase 1), with the rules crate `searchlight_rules` ([ADR-0015](../../_todo/adr/0015-fps-rules-are-a-platform-free-crate.md)) | none; stopgaps: non-positional sound, a rectangle HUD, a local seeded generator. Navigation (ENG-F17-01 and -02) has landed for chase and patrol |
| P.3 | GDD §7 Phase 2 (items and loadout) | 2.11.3 on-screen text, 2.12 layered content, 2.14 tap and hold keys |

## 3 FPS-M1 Arena baseline (FPS-F2 #297)

- **Exit goal:** a player fights escalating waves in the baked arena, dies and restarts, with weighty rifle feel and per-body-part damage. Justin playtests and approves; that opens FPS-M2, enemy grades slice 1 and FPS-M1.2 slice 1.
- **Cut lines:** per-enemy routes, wave banners, pauses, high scores, crouch, physics projectiles.
- **Known gaps accepted:** non-positional sound (ENG-F16 #228), title-bar HUD instead of text (#315 / ENG-F18-09 #407), local seeded RNG instead of #316 / ENG-F6-01 #335.
- **TL risk:** the title-bar HUD stops being enough once FPS-M2 needs inventory. That gates FPS-M2, not FPS-M1.

## 4 FPS-M1.1 Menus (FPS-F3 #457)

- **Outcome:** main menu at launch, pause menu on Escape, settings screen (mouse, volume, graphics, window). Cards #458 main menu, #459 pause, #460 settings with Apply; build in that order.
- **Entry gate:** feature approved; engine request #444 answered. Specs can run in parallel with FPS-M1; building waits for it (shared game loop).
- **Engine answer (ENG-F18):** #695 -11 menu screens and their stack replace the game-side `Menu` rows; #696 -12 settings controls cover every row of #460 except head bob, so #460 is expected to close when -12 merges; #697 -17 adds the head-bob row after #609 builds the setting. Build order -11, -12, -17.
- **Defaults:** pause stops the sim; no return to main menu; no save or continue. Settings are staged, Apply persists, Back discards.
- **Key rebinding:** no FPS card yet. The engine's keybind capture waits for one, and ENG-F26 #359 supplies key actions.
- **Cut lines:** key rebinding, return to main menu (FPS-M6), save and continue (FPS-M5), reset and apply-all buttons, backdrop, gamepad.

## 5 FPS-M1.2 Stance and body (FPS-F4)

Runs in slices parallel to FPS-M2; needs no items or UI beyond one slider in the FPS-F3 settings screen.

| Slice | Content | Needs from the engine |
|---|---|---|
| 1 Lean and head bob | Lean with clearance, hit volumes follow the lean, head-bob camera and slider | nothing new |
| 2 Crouch and prone | Both stances with timed transitions and speed rules; prone's oriented hull; hit volumes and enemy aim follow the stance | ENG-F33 #492 and #544 (ENG-M3) |
| 3 Visible body | Rigged first-person body and shadow; enemies on the same rigs; hit volumes ride the bones | #543 skinned characters, needed by FPS-M2 |

- **All three slices land by FPS-M2's exit** (Justin, 2026-10-23). That asks the engine lane to schedule #492's crouch and clearance slices, #544 and #543 earlier than their current slots (ENG-M3, ENG-M3 and ENG-M5+).
- **Placeholder body before the rig:** slice 1 adds a segmented player body (one box per body part, as the enemies have) that follows lean and stance and casts a shadow, plus a console toggle that draws the hit volumes ([FPS-F4-04](https://github.com/WrackedFella/moho/issues/611)). It needs no engine work and gives visual feedback until #543 lands; slice 3 replaces it.
- **Exit:** in the arena a player crouched behind the 1.4 m crates is hit only when leaning, on head or arm; a prone player behind the same crate is not seen at all. The shadow follows the lean (slice 1); looking down shows the legs (slice 3).
- **Cut lines:** weapon collision against the ground, prone dive, enemies going prone, variable crouch height, side-step, blind fire, mantle, slide, vault. If #543 can't give the sim deterministic bone poses, hit volumes follow a per-stance pose table instead of the bones and the rig only shows things; cover behaves the same.
- **Held actions:** hold or toggle per keybinding lands with ENG-F26; until then lean is hold and crouch is toggle.

## 6 FPS-M2 Items and loadout (GDD Phase 2)

- **Outcome:** the player carries a rig and a backpack, enemies drop loot through their bodies, and three weapons in two calibers come from one data-driven item model. Magazines are items holding an ordered stack of rounds, mixed types allowed.
- **Scope notes (GDD §4.1, §4.12):** item size and container cells in the first data file with a list view over them; three item classes; armor slots present with no items; the stat layer is born here with carry capacity, movement speed and stamina; a weight readout.
- **Features:** [FPS-F6](https://github.com/WrackedFella/moho/issues/590) scavenging loop (looting for rounds), then [FPS-F7](https://github.com/WrackedFella/moho/issues/591) loadout and load, beside [FPS-F9](https://github.com/WrackedFella/moho/issues/616) world objects and interactables and [FPS-F10](https://github.com/WrackedFella/moho/issues/617) test levels and a proving ground. FPS-F8 (hands and reload animation) lands in this milestone too, after FPS-F4 slice 3; it is drafted once #543 is scheduled.
- **World objects (FPS-F9):** dropped items, and the old magazine from FPS-F7's fast reload (FPS-F7-06, GDD §4.2), are loose objects in the world, picked up by looking at them; a magazine keeps its rounds. FPS-F9-03 drops them static on the floor under the drop point; FPS-F9-04 makes them fall, tumble and settle, on engine request [#600](https://github.com/WrackedFella/moho/issues/600). Ragdoll bodies (FPS-F9-05, engine request [#615](https://github.com/WrackedFella/moho/issues/615)) are deferred past FPS-M2.
- **Proving ground (FPS-F10):** a test level with distance targets and cover heights, a sandbox mode, and spawn and reset controls, so stance, weapons and world objects are tested outside the waves.
- **Reload animation (FPS-F8):** first-person hands and weapon with a clip per reload variant (tactical, empty-chamber, fast drop); the magazine moves from weapon to hand and, on a fast reload, leaves the hand as an FPS-F9-04 loose object; the hand takes a picked-up item (on FPS-F9-03). The sim never waits on animation except for reload, whose time the sim decides and the clip shows. Engine: #543. Until then reloads stay invisible.
- **Multiplayer fit:** loose objects are host-simulated and pick-ups host-resolved, like hits; hit volumes come from sim state, never a client's animation.
- **Entry gate:** FPS-M1 approved. Engine: on-screen text (#407), layered content (#231), tap and hold keys (#359); #600 for FPS-F9-04; #543 for FPS-F8.
- **Exit goal:** one item model carries a weapon, a magazine, rounds and a consumable with no special-case code; a data file adds an item without code. A fast reload drops a magazine that lands, settles and can be picked up with its rounds; the player's model reloads it.
- **Cut lines:** the grid view (after the list is played, GDD §9 Q17), stash, a dedicated mixing UI, rarity or affixes. Loose objects: no pushing or kicking them (ENG-F34 #493), no shooting them, no impact sounds until contact events exist; the arena caps their number and removes the oldest. Animation: no IK, ragdolls, procedural recoil, root motion, retargeting or facial animation; enemies reload invisibly to start. If #543 or the reload art lags, FPS-F8 is the first thing to move to FPS-M3.
- **Art risk:** CC0 placeholder rigs and locomotion clips exist (ADR-0007: Quaternius, Kenney, Khronos samples); CC0 first-person reload clips mostly don't. Expect to hand-key two or three crude clips; that is authoring work, not engine work.
- **Risk:** the largest scope jump. Split in two features if the first card set is too big: (1) item model and loot, (2) inventory UI. The Tech Lead decides.

## 7 FPS-M3 Wear, repair, ledger (Phase 3)

- **Outcome:** four components per weapon with condition, jams, repair parts, a workbench, dismantling, limited-use cleaning kits, armor items with wear, the toolkit, and an end-of-round ledger valuing rounds and wear.
- **Entry gate:** FPS-M2 approved. Needs a second baked map for a range (authoring, not engine).
- **Exit goal:** after a fight the ledger shows a cost, and a player chooses to scavenge for parts because of it.
- **Added to scope (2026-10-23):** armor items give base rig slots and heavier armor slows the player further (GDD §4.1, §4.6, open question Q27: settle it when armor is carded, together with FPS-F7-02 [#604](https://github.com/WrackedFella/moho/issues/604)); a tube-fed shotgun, so the line shows several reload styles (GDD §4.2); round penetration against armor condition; weapon components are items that swap by fit rules.
- **Cut lines:** more than four components, per-weapon unique parts, ammo crafting (GDD "Later"). Attachments move to FPS-M3.1.
- **Enemy AI slice 2** (group alerts, leader lift, fire and maneuver, suppressive fire) lands before this milestone. It is a feature of its own, to be filed as the next free FPS feature; engine needs are #228 and #229 plus flanking queries.

## 7.1 FPS-M3.1 Attachments (GDD Phase 3.1)

- **Outcome:** the attachment system: slots and mount types in data, fit rules, swap a scope and/or a laser sight at run time. The items are mocked, one scope and one laser. Weapon components swap by the same rules.
- **Entry gate:** FPS-M3 approved (components are items).
- **Exit goal:** a player swaps the scope or laser on a weapon, and the sight picture or beam changes; a new attachment is a data file.
- **Cut lines:** conversion kits, caliber swaps, rail variety past the first mount types, weapon-mounted light, magnified second view.
- **Engine requests:** weapon assembled from sockets, field of view and reticle, beam and dot (running list kept with the project).

## 8 FPS-M4 Wounds and healing (Phase 4)

- **Outcome:** the §4.7 first cut: damage-scaled penalties, leg compounding, bleeding with bandages, first and secondary aid, splint, perishable meds, recovery timers on a minimal game clock, an inspect screen.
- **Exit goal:** a leg or arm wound changes what the player does next, and treating it costs time and items.
- **To taste, in order:** enemy debuffs, painkiller masking, tourniquet, temporary head statuses, arm compounding and left or right arms, surgical kit and med crafting.
- **Cut lines:** radiation; disease and wasting injuries until the core proves out.
- **Screen and audio effects** driven by wound state (vignette, blur, sway, muffling) and a ground clip for a leg-broken character (forced prone, §4.7) are part of the milestone.

## 8.1 FPS-M4.1 Grenades, mocked (GDD Phase 4.1)

- **Outcome:** a grenade is an item; throwing it spawns an object on the ground where the player looks, plays a sound cue and despawns it.
- **Exit goal:** a throwable goes through the same item, use and host-resolved command path as everything else, with no special case.
- **Cut lines:** area damage, thrown-body physics, AI grenades. First thing to cut if time presses.
- **Engine:** none beyond runtime spawn and positional audio (#228).

## 9 FPS-M5 Time, survival, rest (Phase 5)

- **Outcome:** game clock scaling and skipping, hunger, thirst, fatigue, sleep at a safe room with healing timers advancing.
- **Entry gate:** game clock scaled over the tick (GDD §10 item 8; pause and speed #334). Save and load for world state (§10 item 9; ENG-F25 #346) is not needed for the prototype gate, which has no continue; it gates FPS-M6.
- **Decisions due:** GDD §9 Q8 (difficulty as complexity toggles) and Q2 (co-op timing).
- **Cut lines:** temperature, disease, spoilage.

## 10 FPS-M6 Hub and jobs (Phase 6)

- **Outcome:** trader hub, job board, currency, full per-job ledger; the arena becomes the job site and waves and score retire from the default experience.
- **Entry gate:** level switching carrying player state (§10 item 10), save and load. Needs a hub and one or two zone maps.
- **Decisions due:** GDD §9 Q1 (setting), Q3 (pivot point), Q15 (starting backgrounds).
- **Earliest playable game:** the first milestone that is a game rather than a prototype; the cross-line v1 gate in the [roadmap](../roadmap.md#milestone-gates-that-span-lines) uses it.

## 11 FPS-M7 Director (Phase 7)

- **Outcome:** zone graph, factions, offline dice sim, daily planning, live handover; groups as the Director's unit.
- **Entry gate:** headless sim (§10 item 11, ENG-F6 #233 related), navigation for live actors.
- **Cut lines:** dialogue and diplomacy, emergent quests, more than about five zones. Highest pivot risk, so last.

## 12 Engine requests from this line

The engine lane designs the answers.

| Request | Engine item | Needed by |
|---|---|---|
| #444 menus and settings controls | ENG-F18 settings screen (2.11.6) | FPS-M1.1 build |
| #315 on-screen text | ENG-F18-09 #407 | FPS-M2 entry |
| #316 seeded replayable RNG | ENG-F6-01 #335 | before FPS-M2 if it matters |
| ENG-F16 #228 positional audio | ENG-F16 | enemy AI slice 2 (gunshots alert groups) |
| ENG-F17 #229 navigation and cover queries | ENG-F17-01 and -02 (navigation); cover and exposure queries | enemy AI slice 2 |
| ENG-F19 #231 layered content | ENG-F19 | FPS-M2 entry |
| ENG-F26 #359 key actions | ENG-F26 | FPS-M2; held-action toggles |
| #543 skinned animated characters, bone attachments, sim-timed and layered clips | #543 | FPS-M2: FPS-M1.2 slice 3, FPS-F8 |
| #544 lean and stance transitions, prone hull | #544 with ENG-F33 #492 (crouch and clearance) | FPS-M2: FPS-M1.2 slice 2 |
| #600 loose objects fall, settle and are picked up | #600 (ENG-F34 #493 is related) | FPS-M2: FPS-F9-04 |
| #615 ragdoll bodies | #615 | after FPS-M2: FPS-F9-05 (deferred) |

New requests come from FPS-M3 onward (workbench interaction, save kinds, level switching). File them when a milestone's feature is drafted.

## 13 Prototype gate

- **Scope:** FPS-M1 to FPS-M5, plus FPS-M1.2, FPS-M3.1 and FPS-M4.1; enemy grades and group tactics; placeholder art and animation (weapons, magazines, gear, player and enemy models; stand, crouch, prone, walk, run, crawl, lean, a ground clip for the injured) from free or generated assets; test levels with scripted states (FPS-F9, FPS-F10, plus a debug action per milestone: set a limb, set a survival meter, advance the clock).
- **At the gate:** playtest, promote `dev` to `main`, and take the split to the Engine project. The plan is to split at FPS-M5 completion; the cross-line gate in the [roadmap](../roadmap.md#milestone-gates-that-span-lines) still names FPS-M6 and changes only when the Engine project agrees.
- **Cut order if time presses:** grenades (FPS-M4.1), then attachments (FPS-M3.1), then the fifth archetype.
- **Critical path:** the engine requests #543, #544, #492 and #600 for FPS-M2.
