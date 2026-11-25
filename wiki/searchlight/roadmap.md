# Searchlight roadmap

The FPS line's milestones, in order. The phases and their questions are in the [GDD](game-design-document.md) §7; this page adds each milestone's entry gate, exit goal and cut lines. Engine items are numbered as in the [engine roadmap](../engine/roadmap.md); dependencies on other lines are in the [roadmap](../roadmap.md). Item status lives on the [project board](https://github.com/users/WrackedFella/projects/1); this page holds order, not status.

**Rule from the GDD:** each phase becomes a feature only after the previous one is approved. So this plan fixes milestones and exit goals; cards live in the feature issues listed in [section 2](#2-features).

Milestone names follow ISO 2145: FPS-M*n* delivers GDD Phase *n*; FPS-M1.1, FPS-M1.2 and FPS-M1.3 are bridges.

## 1 Overview

| Milestone | GDD phase | Question it answers | Engine gate |
|---|---|---|---|
| FPS-M1 Arena baseline | 1 | Does the gunplay feel weighty? | none hard; stopgaps in use |
| FPS-M1.1 Menus (FPS-F3) | 1 to 2 bridge | Can a player start, pause and tune the game without the console? | engine request #444, answered by ENG-F18-11 #695, -12 #696, -17 #697 |
| FPS-M1.2 Stance and body (FPS-F4) | 1.2 | Does posture change where I stand and when I peek? | #544 with ENG-F33 #492; #543 for the rig; all by FPS-M2 |
| FPS-M1.3 Bullets fly (FPS-F15) | 1.3 | Does a round in flight make distance a decision? | none; ENG-F40 #636 tracer following a game-owned position for presentation |
| FPS-M2 Items and loadout | 2 | Does one item model carry everything? | ENG-F18-09 #407, ENG-F18-18 #698 item lists (base for the inventory and search screens), ENG-F19 #231, ENG-F26 #359; #600 loose objects (FPS-F9); #543 for reload animation (FPS-F8) |
| FPS-M3 Wear, repair, ledger | 3 | Does wear drive scavenging? | none new |
| FPS-M3.1 Attachments | 3.1 | Can a weapon be rebuilt in the field without special cases? | sockets, zoom and reticle, beam and dot (engine requests) |
| FPS-M4 Wounds and healing | 4 | Does limb damage create good downtime? | none new |
| FPS-M4.1 Grenades (mocked) | 4.1 | Does a throwable fit the item model? | none beyond runtime spawn and positional audio |
| FPS-M5 Time, survival, rest | 5 | Do survival stats add pressure, not chores? | game clock scaling; save and load |
| FPS-M6 Hub and missions | 6 | Does profit-as-score change behavior? | level switching; save and load |
| FPS-M7 Director | 7 | Does the world feel alive on little machinery? | headless sim; navigation |
| Later: generated maps | Later | Does assembly add variety the hand-made pool lacks? | assemble a level from pieces (GDD §10 item 13) |

Order is the GDD's: systems arrive in the order they create costs, so the ledger has something to measure. Enemy AI lands in slices alongside the phases (GDD §7).

**Prototype gate:** FPS-M1 through FPS-M5 with FPS-M1.2, FPS-M1.3, FPS-M3.1 and FPS-M4.1, group tactics, placeholder art and animation, and test levels with scripted states ([section 13](#13-prototype-gate)). The systems are the deliverable; attachment and grenade items are mocked.

## 2 Features

| # | Feature | Engine items it needs |
|---|---|---|
| P.1 | [FPS-F1](../../_todo/searchlight/FPS-F1-game-design-document/_feature.md) [GDD](game-design-document.md) v0.3 | none |
| P.2 | [FPS-F2](https://github.com/WrackedFella/moho/issues/297) arena baseline (GDD §7 Phase 1), with the rules crate `searchlight_rules` ([ADR-0015](../../_todo/adr/0015-fps-rules-are-a-platform-free-crate.md)) | none; stopgaps: non-positional sound, a rectangle HUD, a local seeded generator. Navigation (ENG-F17-01 and -02) has landed for chase and patrol |
| P.3 | [FPS-F3](https://github.com/WrackedFella/moho/issues/457) menus (FPS-M1.1); [FPS-F14](https://github.com/WrackedFella/moho/issues/718) key rebinding (FPS-M1.1); [FPS-F17](https://github.com/WrackedFella/moho/issues/810) menu polish (beside FPS-M2) | ENG-F18 menu screens, settings controls and rebinding |
| P.4 | [FPS-F4](https://github.com/WrackedFella/moho/issues/602) stance and body (FPS-M1.2); [FPS-F18](https://github.com/WrackedFella/moho/issues/877) settled shots (beside FPS-M1.2) | ENG-F43 crouch, prone and lean (done); ENG-F39 rig; #917 shadow-only mesh |
| P.5 | [FPS-F15](https://github.com/WrackedFella/moho/issues/745) bullets fly (FPS-M1.3) | ENG-F40 tracer (done) |
| P.6 | [FPS-F5](https://github.com/WrackedFella/moho/issues/584) enemy grades (AI slice 1, beside FPS-M2) | none |
| P.7 | FPS-M2: [FPS-F6](https://github.com/WrackedFella/moho/issues/590) scavenging, [FPS-F7](https://github.com/WrackedFella/moho/issues/591) loadout and load, [FPS-F9](https://github.com/WrackedFella/moho/issues/616) world objects, [FPS-F10](https://github.com/WrackedFella/moho/issues/617) test levels, [FPS-F13](https://github.com/WrackedFella/moho/issues/673) enemies on moving meshes, [FPS-F20](https://github.com/WrackedFella/moho/issues/921) ammo readout (beside, not an exit item), FPS-F8 hands and reload animation (not yet drafted) | 2.11.3 on-screen text, 2.12 layered content, 2.14 tap and hold keys, ENG-F18-18 and -22 item lists and row actions, ENG-F38, ENG-F48, ENG-F39 |
| P.8 | [FPS-F16](https://github.com/WrackedFella/moho/issues/790) Blender map authoring (by the FPS-M3 entry gate) | none |
| P.9 | [FPS-F11](https://github.com/WrackedFella/moho/issues/630) group tactics (AI slice 2, before FPS-M3); [FPS-F12](https://github.com/WrackedFella/moho/issues/631) placeholder art and animation (FPS-M2 to FPS-M4) | ENG-F16 positional audio (done), ENG-F42 hidden routes and cover; ENG-F39 |
| P.10 | [FPS-F19](https://github.com/WrackedFella/moho/issues/930) grid inventory (after the FPS-M2 list playtest) | ENG-F18-23 #935, -24 #936 (row 3.4g) |
| P.11 | FPS-F21 difficulty page and presets (draft; after the prototype gate, [section 12.1](#121-difficulty-settings-gdd-415)) | none |

## 3 FPS-M1 Arena baseline (FPS-F2 #297)

- **Exit goal:** a player fights escalating waves in the baked arena, dies and restarts, with weighty rifle feel and per-body-part damage. Justin playtests and approves; that opens FPS-M2, enemy grades slice 1 and FPS-M1.2 slice 1.
- **Cut lines:** per-enemy routes, wave banners, pauses, high scores, crouch, physics projectiles (now FPS-M1.3).
- **Known gaps accepted:** non-positional sound (ENG-F16 #228), title-bar HUD instead of text (#315 / ENG-F18-09 #407), local seeded RNG instead of #316 / ENG-F6-01 #335.
- **TL risk:** the title-bar HUD stops being enough once FPS-M2 needs inventory. That gates FPS-M2, not FPS-M1.

## 4 FPS-M1.1 Menus (FPS-F3 #457)

- **Outcome:** main menu at launch, pause menu on Escape, settings screen (mouse, volume, graphics, window). Cards #458 main menu, #459 pause, #460 settings with Apply; build in that order.
- **Entry gate:** feature approved; engine request #444 answered. Specs can run in parallel with FPS-M1; building waits for it (shared game loop).
- **Engine answer (ENG-F18):** #695 -11 menu screens and their stack replace the game-side `Menu` rows; #696 -12 settings controls cover every row of #460 except head bob, so #460 is expected to close when -12 merges; #697 -17 adds the game slider row and `[game]` storage, then #609 builds the head-bob setting and its row on them. Build order -11, -12, -17, then #609.
- **Defaults:** pause stops the sim; no return to main menu; no save or continue. Settings are staged, Apply persists, Back discards.
- **Key rebinding ([FPS-F14](https://github.com/WrackedFella/moho/issues/718)):** a Controls page in Settings, after #696. Primary and secondary input per action, a confirm dialog on conflict, clear and reset; a per-action expander sets hold, double press or a Ctrl/Shift/Alt modifier per slot; held actions get a hold/toggle checkbox. Cards #719 to #722 first; #723 hold and chord, #724 double press and #725 hold/toggle are the cut line (their engine parts, #747 to #749, have landed). It is the consumer the engine's keybind capture waits for; ENG-F26 #359 supplies key actions, and double press is a new engine request.
- **Cut lines:** return to main menu (FPS-M6), save and continue (FPS-M5), settings-wide reset and apply-all buttons (Controls has its own reset), gamepad (also out of FPS-F14), key hints in prompts.
- **Menu polish ([FPS-F17](https://github.com/WrackedFella/moho/issues/810)):** a medium-priority follow-up beside FPS-M2; it doesn't gate FPS-M2 or the prototype gate, except #812, whose Game page FPS-M2's reload setting (#924) and FPS-F20's readout row (#922) need. #811 menus readable over any scene (engine request #808); #812 settings on six category pages (Controls, Mouse, Audio, Graphics, Display, Game) with field of view, crosshair and master volume (engine request #809), after FPS-F14-01 #719; #813 the main menu shows the launch level through cycling camera views, from `menu_camera_NN` markers. Deferred, and not needed to close the feature: #814 level pick from the main menu (waits for more levels and #741), #815 save and load stub buttons. New settings arrive with the feature they tune (settings catalog linked from #810).

## 5 FPS-M1.2 Stance and body (FPS-F4)

Runs in slices parallel to FPS-M2; needs no items or UI beyond one slider in the FPS-F3 settings screen.

| Slice | Content | Needs from the engine |
|---|---|---|
| 1 Lean and head bob | Lean with clearance, hit volumes follow the lean, head-bob camera and slider | nothing new |
| 2 Crouch and prone | Both stances with timed transitions and speed rules; prone's oriented hull; hit volumes and enemy aim follow the stance | ENG-F33 #492 and #544 (ENG-M3) |
| 3 Visible body | Rigged first-person body and shadow; enemies on the same rigs; hit volumes ride the bones | #543 skinned characters, needed by FPS-M2 |

- **All three slices land by FPS-M2's exit** (Justin, 2026-10-23). That asks the engine lane to schedule #492's crouch and clearance slices, #544 and #543 earlier than their current slots (ENG-M3, ENG-M3 and ENG-M5+).
- **Placeholder body before the rig:** a console toggle draws every character's hit volumes ([FPS-F4-04](https://github.com/WrackedFella/moho/issues/611)). A segmented player body (one box per body part, as the enemies have) that casts a shadow but is not drawn in the player's own view waits on [#946](https://github.com/WrackedFella/moho/issues/946) (engine request #917, accepted 2026-11-25), and is carded next; slice 3 replaces it with the rig.
- **Exit:** in the arena a player crouched behind the 1.4 m crates is hit only when leaning, on head or arm; a prone player behind the same crate is not seen at all. The shadow follows the lean (slice 1, on #946); looking down shows the legs (slice 3). Slice 2's engine side (ENG-F43) has landed; its cards are next to draft.
- **Cut lines:** weapon collision against the ground, prone dive, enemies going prone, variable crouch height, side-step, blind fire, mantle, slide, vault. If #543 can't give the sim deterministic bone poses, hit volumes follow a per-stance pose table instead of the bones and the rig only shows things; cover behaves the same.
- **Settled shots ([FPS-F18](https://github.com/WrackedFella/moho/issues/877)):** movement and rapid fire widen the player's spread, the crosshair's gap shows it, and settling closes it; the crosshair is always the shot centre. Runs beside FPS-F4 and needs no engine work. Slice 2's crouch and prone tighten it; arm wounds (FPS-M4) and barrel wear (FPS-M3) widen it later.
- **Held actions:** hold or toggle per keybinding lands with ENG-F26 and its checkbox with FPS-F14-07 (#725); until then lean is hold and crouch is toggle.

## 5.1 FPS-M1.3 Bullets fly (FPS-F15)

Ballistics is a core feature (Justin, 2026-10-26; GDD §4.14, §9 Q7). This milestone is its floor, tier 1; the higher tiers ride later milestones. Research report: [Searchlight Ballistics](https://claude.ai/artifact/8HSFWzmixNjzK6us87AM9J).

- **Outcome:** a fired round takes time to arrive, falls over distance, bounces off a hard surface at a shallow angle, and hits for less the further it has flown. Enemies fire the same rounds and lead a moving player by grade. Hitscan stays as the Simple toggle for A/B playtests, and a gravity scale lets drop be read in the arena.
- **Entry gate:** FPS-M1 approved (passed 2026-10-22). Runs beside FPS-M2 and must land before FPS-M2 fixes its round data files, since rounds gain muzzle velocity, drag and penetration fields. Needs no items, UI or engine work.
- **Exit goal:** a player leads a running enemy and bounces a round off a wall; a far target takes less damage than a near one; a replay with the same seed and commands matches every impact.
- **Cut lines:** materials and penetration (tier 2, FPS-M3), wind and zeroing (tier 3, FPS-M3.1), breath and heart rate (tier 4, FPS-M5), more than one ricochet, ricochet off bodies, visible bullet meshes, bullets moving loose objects, audio beyond a ricochet sound, rewinding shots for co-op.
- **Engine:** nothing blocking. The tracer should follow a position the game owns (ENG-F40 #636 slice 1 spawns fixed-lifetime effects); state it on #636 before that slice is built. Until then rounds fly undrawn or with the stopgap effect.
- **Scale:** cost grows with rounds in flight, not map area, so the larger hand-made maps (GDD §4.13) are no burden; they are where drop and wind become real.
- **Next step:** `/devflow:business-analyst FPS-F15` (Justin starts it).

## 6 FPS-M2 Items and loadout (GDD Phase 2)

- **Outcome:** the player wears armor, which is the rig (magazine slots and pockets), and carries a backpack; enemies drop loot through their bodies, and three weapons in two calibers come from one data-driven item model. Magazines are items holding an ordered stack of rounds, mixed types allowed.
- **Scope notes (GDD §4.1, §4.2, §4.12):** item size in the first data file, with a list view; weight is the only limit, and every pack (worn, dropped or inside another) has its own maximum; three item classes; reloads draw only from the armor's magazine slots, a normal reload stows the spent magazine and a quick reload (opt-in, unbound by default) drops it; the stat layer is born here with carry capacity, movement speed and stamina; a weight readout. Armor protection and wear wait for FPS-M3.
- **Features:** [FPS-F6](https://github.com/WrackedFella/moho/issues/590) scavenging loop (looting for rounds), then [FPS-F7](https://github.com/WrackedFella/moho/issues/591) loadout and load, beside [FPS-F9](https://github.com/WrackedFella/moho/issues/616) world objects and interactables and [FPS-F10](https://github.com/WrackedFella/moho/issues/617) test levels and a proving ground, and [FPS-F13](https://github.com/WrackedFella/moho/issues/673) enemies drawn through ENG-F38 and proven at fifty (#674); its CI slowdown guard #929 moves to FPS-M3 (Justin, 2026-11-25). Beside them, not exit items: [FPS-F17](https://github.com/WrackedFella/moho/issues/810) menu polish and [FPS-F20](https://github.com/WrackedFella/moho/issues/921) ammo readout (its default, pips, is judged in the FPS-M2 playtest). FPS-F8 (hands and reload animation) lands in this milestone too, after FPS-F4 slice 3; it is being drafted against #943 to #945.
- **World objects (FPS-F9):** dropped items, and the old magazine from FPS-F7's quick reload (FPS-F7-06, GDD §4.2), are loose objects in the world, picked up by looking at them; a magazine keeps its rounds. FPS-F9-03 drops them static on the floor under the drop point; FPS-F9-04 makes them fall, tumble and settle, on engine request [#600](https://github.com/WrackedFella/moho/issues/600). Ragdoll bodies (FPS-F9-05, engine request [#615](https://github.com/WrackedFella/moho/issues/615)) are deferred past FPS-M2.
- **Map authoring ([FPS-F16](https://github.com/WrackedFella/moho/issues/790)):** the map editor is Blender with a Searchlight add-on (GDD §5 Authoring). First slice: the add-on places today's map components (player and enemy spawns, waypoints, cover with its direction, dummies), exports the level, and launches the game on it. Second slice: a check lists the problems the game would find in the open map, inside Blender. The components are defined once and read by both the add-on and the game. From then on, **every feature that adds a map component adds it to the add-on** (loot points and building slots in FPS-M6, safe spots in FPS-M5, anomalies later). Independent of the other FPS-M2 features; it has to be done by the FPS-M3 entry gate.
  - **Cut lines:** kit pieces (until GDD §9 Q31 fixes the metrics), custom gizmos beyond Blender's empties and arrows, game behaviour inside Blender, more than one Blender version (one LTS), packaging for modders (§9 Q34), an in-game editor, an in-game marker overlay and live reload (add to taste after the first map is built).
  - **TL:** ADR-0007 bans copyleft, and Blender add-ons that use its Python API are GPL. The add-on ships nothing into the game; amend the ADR to exempt tools outside the shipped build before the first card is built. Also: the format and home of the shared component definition; whether the proving ground moves from its generator script to a Blender file.
- **Proving ground (FPS-F10):** a test level with distance targets and cover heights, and spawn and reset controls, so stance, weapons and world objects are tested outside the waves.
- **Reload animation (FPS-F8):** first-person hands and weapon with a clip per reload variant (tactical, empty-chamber, quick drop); the magazine moves from weapon to hand and, on a fast reload, leaves the hand as an FPS-F9-04 loose object; the hand takes a picked-up item (on FPS-F9-03). The sim never waits on animation except for reload, whose time the sim decides and the clip shows. Engine: #543, fast-tracked into ENG-M2 (2026-11-25) as #943 sim-timed clip, #944 upper-body layer and #945 bone to bone or into the world; the first-person view stays in ENG-M3 row 3.4b, so until then the hands can poke through a wall the player hugs. Until FPS-F8 lands reloads stay invisible.
- **Multiplayer fit:** loose objects are host-simulated and pick-ups host-resolved, like hits; hit volumes come from sim state, never a client's animation.
- **Entry gate:** FPS-M1 approved. Engine: on-screen text (#407), layered content (#231), tap and hold keys (#359); #600 for FPS-F9-04; #543 for FPS-F8.
- **Exit goal:** one item model carries a weapon, a magazine, rounds, a pack and a consumable with no special-case code; a data file adds an item without code. The player reloads only from the armor's magazine slots: a normal reload stows the spent magazine in the pockets or backpack, and a quick reload drops it where it lands, settles and can be picked up with its rounds; the player's hands show both. Every pack stays within its own weight, and what the player carries slows them and caps sprint. Justin playtests the list inventory and the default ammo check.
- **Cut lines:** the grid view ([FPS-F19](https://github.com/WrackedFella/moho/issues/930), after the list is played, GDD §9 Q17), stash, a dedicated mixing UI, rarity or affixes. Loose objects: no pushing or kicking them (ENG-F34 #493), no shooting them, no impact sounds until contact events exist; the arena caps their number and removes the oldest. Animation: no IK, ragdolls, procedural recoil, root motion, retargeting or facial animation; enemies reload invisibly to start. If #543 or the reload art lags despite the fast-track, FPS-F8 is the first thing to move to FPS-M3.
- **Art risk:** CC0 placeholder rigs and locomotion clips exist (ADR-0007: Quaternius, Kenney, Khronos samples); CC0 first-person reload clips mostly don't. Expect to hand-key two or three crude clips; that is authoring work, not engine work.
- **Risk:** the largest scope jump. Split in two features if the first card set is too big: (1) item model and loot, (2) inventory UI. The Tech Lead decides.

## 7 FPS-M3 Wear, repair, ledger (Phase 3)

- **Outcome:** wear on weapons and armor only (magazine wear is a later option, GDD §4.15): four components per weapon with condition (averaged into the weapon's; below 60 % a component counts double), jams, repair parts, a workbench, dismantling, limited-use cleaning kits, armor items with wear, the toolkit, and an end-of-round ledger valuing rounds and wear.
- **Entry gate:** FPS-M2 approved. Needs the first large map: a second baked level on the modular kit, larger than the arena, with 300 m plus sightlines and a range with targets at known distances (GDD §5). Authoring, not engine: built in Blender with the FPS-F16 add-on, which is done before this map starts; it fixes the kit's metrics (GDD §9 Q31) and replaces the arena as the home map, the arena staying as a test level.
- **Exit goal:** after a fight the ledger shows a cost, and a player chooses to scavenge for parts because of it.
- **Added to scope (2026-10-23):** armor protection and wear on the armor that is already the rig (its magazine slots and pockets land in FPS-M2; Q27 answered 2026-11-25), and heavier armor slows the player further (GDD §4.1, §4.6); a tube-fed shotgun, so the line shows several reload styles (GDD §4.2); surface materials and round penetration through cover and armor by penetration against resistance and condition (ballistics tier 2, GDD §4.14); weapon components are items that swap by fit rules.
- **Cut lines:** more than four components, per-weapon unique parts, ammo crafting (GDD "Later"). Attachments move to FPS-M3.1.
- **Fifty-enemy slowdown guard (FPS-F13-02 #929):** a CI test that fails when the fifty-enemy fight gets slower; arrives with the first large map, on ENG-F46-01's headless frame measure (row 3.4i).
- **Grid inventory ([FPS-F19](https://github.com/WrackedFella/moho/issues/930)):** the grid view over FPS-M2's list (unbounded, sorted on demand, coloured status per cell) lands after the FPS-M2 list playtest, beside this milestone; engine side ENG-F18-23 #935 and -24 #936 (row 3.4g).
- **Enemy AI slice 2** (group alerts, leader lift, fire and maneuver, suppressive fire) lands before this milestone. It is [FPS-F11](https://github.com/WrackedFella/moho/issues/630); engine needs are ENG-F16 #228 (done) and ENG-F42 #638 hidden routes and cover.

## 7.1 FPS-M3.1 Attachments (GDD Phase 3.1)

- **Outcome:** the attachment system: slots and mount types in data, fit rules, swap a scope and/or a laser sight at run time. The items are mocked, one scope and one laser. Weapon components swap by the same rules. Wind and scope zeroing (ballistics tier 3) arrive with the scope.
- **Entry gate:** FPS-M3 approved (components are items).
- **Exit goal:** a player swaps the scope or laser on a weapon, and the sight picture or beam changes; a new attachment is a data file.
- **Cut lines:** conversion kits, caliber swaps, rail variety past the first mount types, weapon-mounted light, magnified second view.
- **Engine requests:** weapon assembled from sockets, field of view and reticle, beam and dot (running list kept with the project).

## 8 FPS-M4 Wounds and healing (Phase 4)

- **Outcome:** the §4.7 first cut: damage-scaled penalties, leg compounding, bleeding with bandages, first and secondary aid, splint, perishable meds, recovery timers on a minimal game clock, an inspect screen.
- **Exit goal:** a leg or arm wound changes what the player does next, and treating it costs time and items.
- **To taste, in order:** enemy debuffs, painkiller masking, tourniquet, temporary head statuses, arm compounding and left or right arms, surgical kit and med crafting.
- **Cut lines:** radiation; disease and wasting injuries until the core proves out.
- **Screen and audio effects** driven by wound state (vignette, blur, sway, muffling, a limp in the head bob: the wounded leg's loop shortens and the sideways swing narrows, GDD §4.7) and a ground clip for a leg-broken character (forced prone, §4.7) are part of the milestone.

## 8.1 FPS-M4.1 Grenades, mocked (GDD Phase 4.1)

- **Outcome:** a grenade is an item; throwing it spawns an object on the ground where the player looks, plays a sound cue and despawns it.
- **Exit goal:** a throwable goes through the same item, use and host-resolved command path as everything else, with no special case.
- **Cut lines:** area damage, thrown-body physics, AI grenades. First thing to cut if time presses.
- **Engine:** none beyond runtime spawn and positional audio (#228).

## 9 FPS-M5 Time, survival, rest (Phase 5)

- **Outcome:** game clock scaling and skipping, hunger, thirst, fatigue, sleep at a safe room with healing timers advancing. Heart rate and breath sway the aim (ballistics tier 4).
- **Entry gate:** game clock scaled over the tick (GDD §10 item 8; pause and speed #334). Save and load for world state (§10 item 9; ENG-F25 #346) is not needed for the prototype gate, which has no continue; it gates FPS-M6.
- **Decisions due:** Q2 (co-op timing) and Q32 (breath and heart rate). Q8 (difficulty) is settled by GDD §4.15.
- **Cut lines:** temperature, disease, spoilage.

## 10 FPS-M6 Hub and missions (Phase 6)

- **Outcome:** trader hub, job board offering missions with their conditions shown, currency, full per-job ledger, each mission rated by Threat (GDD §4.13); the arena becomes the job site and waves and score retire from the default experience. Missions play on hand-made maps built on the modular kit, with random starts, exits, spawns and building slots, and loot by the GDD §4.13 rule (the place decides the pool, difficulty decides amount and ceiling).
- **Entry gate:** level switching carrying player state (§10 item 10), save and load. Needs a hub and two mission maps, blocked out by hand on the kit in Blender, with their loot points and building slots added to the FPS-F16 add-on; the kit's metrics are fixed with the first map (GDD §9 Q31).
- **Decisions due:** GDD §9 Q1 (setting), Q3 (pivot point), Q15 (starting backgrounds), Q28 (mission persistence), Q29 (failed mission cost), Q30 (hub upgrades).
- **Cut lines:** generated geometry (the later phase below), free parameter sliders, hub upgrades that raise stats, bespoke art.
- **Earliest playable game:** the first milestone that is a game rather than a prototype; the cross-line v1 gate in the [roadmap](../roadmap.md#milestone-gates-that-span-lines) uses it.

## 11 FPS-M7 Director (Phase 7)

- **Outcome:** zone graph, factions, offline dice sim, daily planning, live handover; groups as the Director's unit.
- **Entry gate:** headless sim (§10 item 11, ENG-F6 #233 related), navigation for live actors.
- **Cut lines:** dialogue and diplomacy, emergent quests, more than about five zones. Highest pivot risk, so last.

## 11.1 Later: generated maps

- **Outcome:** mission maps assembled at load from kit modules cut from the hand-made maps (GDD §4.13, §5).
- **Entry gate:** FPS-M6 played with the hand-made pool. Before cards, a spike assembles a few hundred metres of modules, builds navigation, times the load and lists seam failures.
- **Engine:** assemble a level from pieces (GDD §10 item 13), filed as an engine request when this is drafted.
- **Cut lines:** generated terrain, free-form layout outside the kit's connection points.

## 12 Engine requests from this line

The engine lane designs the answers.

| Request | Engine item | Needed by |
|---|---|---|
| #444 menus and settings controls | ENG-F18 settings screen (2.11.6) | FPS-M1.1 build |
| #315 on-screen text | ENG-F18-09 #407 | FPS-M2 entry |
| #316 seeded replayable RNG | ENG-F6-01 #335 | before FPS-M2 if it matters |
| ENG-F16 #228 positional audio | ENG-F16 | enemy AI slice 2 (gunshots alert groups) |
| ENG-F17 #229 navigation and cover queries | ENG-F17 navigation done; #229 is closed. Hidden routes and cover candidates are ENG-F42 [#638](https://github.com/WrackedFella/moho/issues/638) | enemy AI slice 2: FPS-F11-03 |
| ENG-F19 #231 layered content | ENG-F19 | FPS-M2 entry |
| ENG-F26 #359 key actions | ENG-F26 | FPS-M2; held-action toggles |
| #543 skinned animated characters, bone attachments, sim-timed and layered clips | ENG-F39 [#635](https://github.com/WrackedFella/moho/issues/635): slices 1 to 3 (models with named nodes and textured materials, sockets, rig and clips, sim-readable bone poses #655) for FPS-M2; fast-tracked into ENG-M2 wave 5 (row 2.22.1, 2026-11-25): sim-timed and cancellable clip [#943](https://github.com/WrackedFella/moho/issues/943), upper-body layer [#944](https://github.com/WrackedFella/moho/issues/944), part moves bone to bone or leaves into the world [#945](https://github.com/WrackedFella/moho/issues/945). Speed and direction blends, the additive lean layer and the first-person view (no wall poke) stay in ENG-M3 row 3.4b | FPS-M2: FPS-M1.2 slice 3, FPS-F8, FPS-F12 |
| Meshes that move without re-upload (runtime spawn) | ENG-F38 [#634](https://github.com/WrackedFella/moho/issues/634), first in the ENG-M2 wave | FPS-M2: enemies, loose objects, models |
| Muzzle flash, tracer and impact marks; later beam and dot, particles | ENG-F40 [#636](https://github.com/WrackedFella/moho/issues/636) | FPS-M2 gunplay feel; FPS-M3.1 laser |
| A tracer that follows a game-owned round position each tick (GDD §10 item 14); not yet raised | ENG-F40 [#636](https://github.com/WrackedFella/moho/issues/636) | FPS-M1.3 presentation |
| Ray entry and exit through a surface, for penetration (GDD §10 item 14); optional, a reverse ray works | not filed | FPS-M3 |
| Screen and sound follow game values (vignette, blur, handles, low-pass) | ENG-F41 [#637](https://github.com/WrackedFella/moho/issues/637) | FPS-M4 |
| Markers carry their size, for volumes | ENG-F14-03 [#639](https://github.com/WrackedFella/moho/issues/639) | FPS-F10 test levels, FPS-M5 safe rooms |
| Levels load whole and switch with a loading screen and a load budget; a level carries a readable boundary (GDD §5, §10 item 10) | 2.27 ENG-F14-04 #750, -05 #752 (unload and switch); 3.4i ENG-F46 #746 (budget, loading screen); ENG-F14-06 #751 (boundary) | FPS-M2 first level switch; FPS-M3 first large map (one load); FPS-M6 carries state across a switch |
| List widgets with icons and counts, two panes, tooltips | comment on #444; drag grid is a later card | FPS-M2 list screens |
| #544 lean and stance transitions, prone hull | #544 with ENG-F33 #492 (crouch and clearance) | FPS-M2: FPS-M1.2 slice 2 |
| #600 loose objects fall, settle and are picked up | ENG-F48 [#773](https://github.com/WrackedFella/moho/issues/773), queued #777 settle, then #778 look-to-identify and remove by id, then #779 smooth drawing (ENG-F34 #493 is related) | FPS-M2: FPS-F9-03 #626, FPS-F7-06 #632 and FPS-F7-08 #925 on #778; FPS-F9-04 #628 on #779; FPS-F10-08 #926 on #777 |
| #615 ragdoll bodies | #615 | after FPS-M2: FPS-F9-05 (deferred) |
| [#917](https://github.com/WrackedFella/moho/issues/917) a mesh that casts a shadow but is not drawn in the camera | ENG-F38-02 [#946](https://github.com/WrackedFella/moho/issues/946) (accepted 2026-11-25) | FPS-M1.2 slice 1: the placeholder body's shadow (card not yet drafted; blocked by #946) |
| #893 an action on an item in a list | ENG-F18-22 #898 (done) | FPS-M2 inventory rows |

New requests come from FPS-M3 onward (workbench interaction, save kinds, level switching). File them when a milestone's feature is drafted.

## 12.1 Difficulty settings (GDD §4.15)

Difficulty is a set of per-system complexity settings (FPS-F21, not yet filed). It is not a block of work: the prototype builds Hard, and each system's feature carries the pluggable shapes of GDD §4.15 as Tech Lead notes, because they are cheap now and costly to retrofit.

| Milestone | Carries now (Hard) | After the prototype gate |
|---|---|---|
| FPS-M2 items | Run settings read from one place (every run Hard); loot tables per setting; reload asks one source for its feed; a magazine may carry a condition | Ammunition Pooled (the dearest setting) |
| FPS-M2 follow-up | The wheel that picks among filled magazines (Medium and Hard) | — |
| FPS-M1.3 ballistics | Hitscan already a toggle | The ballistics row |
| FPS-M3 wear | Condition asked per effect; one jam formula over named inputs; broken and missing components; magazine wear and armor parts stubbed | Jams Off and Forgiving, repair None and Simple, magazine wear On |
| FPS-M4 wounds | Hits resolve in two steps; meds as aid roles; med crafting stubbed | Damage Health bar and Limbs |
| FPS-M6 hub | Run settings in the save; missions rated by Threat | — |
| FPS-F21 | — | The Difficulty page (also the New Game screen), presets in data, HUD preset row, permadeath toggle |

## 13 Prototype gate

- **Scope:** FPS-M1 to FPS-M5, plus FPS-M1.2, FPS-M1.3, FPS-M3.1 and FPS-M4.1; enemy grades and group tactics; placeholder art and animation (weapons, magazines, gear, player and enemy models; stand, crouch, prone, walk, run, crawl, lean, a ground clip for the injured) from free or generated assets; test levels with scripted states (FPS-F9, FPS-F10, plus a debug action per milestone: set a limb, set a survival meter, advance the clock).
- **Difficulty:** the prototype builds the Hard line of GDD §4.15 only; Easy, Medium and the FPS-F21 page come after the gate ([section 12.1](#121-difficulty-settings-gdd-415)).
- **Beside the milestones, not gate items:** FPS-F17 menu polish (except #812), FPS-F20 ammo readout, FPS-F19 grid inventory, and the FPS-F14 cut line (#723 to #725).
- **Engine blockers are fast-tracked** (Justin, 2026-11-25): when a feature on the way to the gate meets an engine requirement, the Engine lane raises unblocking it in its own roadmap. This lane says so on the request and in a relay note.
- **At the gate:** playtest, promote `dev` to `main`, and take the split to the Engine project. The plan is to split at FPS-M5 completion; the Engine project agreed (2026-10-23): the last two FPS features before the split are FPS-M4.1 and FPS-M5, and the split is the later of FPS-M5 completion and the engine's own ENG-M3 gate ([roadmap](../roadmap.md#milestone-gates-that-span-lines)). Every engine item the prototype uses lands by FPS-M4, and level unload for FPS-M6 lands before the split.
- **Cut order if time presses:** grenades (FPS-M4.1), then attachments (FPS-M3.1), then the fifth archetype.
- **Critical path:** the engine wave for FPS-M2: ENG-F38 first, then #600 and ENG-F39 slices 1 to 3 in parallel, plus #544 and #492's crouch and clearance slices, pulled into ENG-M2.
- **Settled in the lane triage:** marker values, the look-at ray, console commands (a slot-0 command under ENG-F37), scope zoom and reticle, and tagged volumes need no engine request; they are line work or already exist.
