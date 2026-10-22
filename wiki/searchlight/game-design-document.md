# FPS-F1 — Game Design Document (v0.2)

**Status:** draft v0.2, approved for Phase 1. Writing this page is
[FPS-F1](../../_todo/searchlight/FPS-F1-game-design-document/_feature.md)'s deliverable. Open questions are in §9.
**Working pitch:** a survival-focused looter shooter with realistic gunplay, where your gear is your character and every fight is a line on a ledger.
**Working target:** single-player first, co-op later, mod support a priority. Pivots (Destiny-like, Arma-like) are open questions, not commitments (§9).

---

## 1 Core pillars

Each pillar is a filter: a feature that serves none of them is cut.

1. **Gear is your character.** No XP, no skill trees. Capability comes from what you carry and its condition.
2. **Every encounter has a price.** Rounds, component wear, armor damage, meds, food, time. Success is measured as profit, not kills or survival.
3. **Difficulty comes from systems, not sponges.** Enemies die to a few well-placed rounds; danger comes from jams, wounds, scarcity and bad decisions. Armor is the only legitimate source of tankiness.
4. **The world doesn't wait for you.** Factions, creatures and independents pursue their own goals; you arrive into consequences.
5. **The Zone is unknowable.** (Area X / Roadside Picnic.) Anomalies and artifacts are never fully explained; wrongness is ambient, not a jump scare.

## 2 Genre and setting

- **Genre:** first-person hardcore survival looter shooter (STALKER Anomaly / GAMMA lineage).
- **Perspective:** first-person only.
- **Setting:** an original, vaguely dystopian sci-fi exclusion zone around an unexplained event. Emphasis on Area X tone: beauty and dread, nature reclaiming, rules of reality that bend locally.
- **Still open (deliberately):** whether the Zone is a neglected corner of a sophisticated world, or the world is truly post-apocalyptic. Weapons stay real-world-inspired either way. Not blocking until Phase 6 (§9 Q1).

## 3 Core loop

### 3.1 Macro loop (the hamster wheel)

1. **Take a job** (fetch, hunt, escort, recover, deliver) with a known payout.
2. **Prepare:** spend money and items to kit out (ammo, mags, meds, food, repairs).
3. **Travel** into danger (zone transitions).
4. **Encounter:** combat, evasion, or waiting it out. Resources drain.
5. **Recover:** wounds force downtime (healing timers); survival stats force rest and food.
6. **Take stock:** repair, craft ammo/food/meds, sell loot.
7. **Ledger:** did income beat expenditure? Profit is the score.

Secondary loops (run away, wait it out) exist but are smaller and obvious.

### 3.2 Rules vs modes

- **Core rules** (weapons, items, wounds, survival, economy, Director) know nothing about score, rounds, waves or match timers.
- **Arena scaffolding** (waves, score, round timer, restart) lives in a separate **mode layer** used to test the core rules in Phases 1–5. It is retired as the default experience in Phase 6, when the arena becomes a job site.
- **Arena as a long-term mode** stays possible on the same layer: an optional mode (an in-world fighting pit, or a standalone arena mode) with its own scoring. It is not a commitment.

### 3.3 Micro loop (moment to moment)

- **Movement:** grounded and weighty. Walk / sprint / crouch / prone, with lean as the peek in every stance. Stance changes take time and pass through crouch; prone crawls, cannot sprint or jump and turns slowly. Stamina limits sprint; carry weight limits stamina. No slide, no double jump, no mantle in Phase 1.
- **Your body is in the world:** the player's hit volumes follow stance and lean, so what sticks out of cover is what gets hit (the cover rule applied to the player). Head bob is a setting; every held action (sprint, crouch, prone, lean) is hold or toggle by a checkbox beside its keybinding.
- **Combat:** few rounds to kill, unforgiving positional damage. Reloading is a commitment (mag in hand, round in chamber). Weapons can fail.
- **Information:** limited HUD. Ammo count is not shown as a number; you check the mag (an action). Condition is inspected, not displayed constantly.

## 4 Systems

Each system states the question its prototype must answer, the design, and its cut line.

### 4.1 Items and loadout (keystone, not in your list but everything depends on it)

- **Question:** can one data-driven item model carry weapons, mags, rounds, meds, food and repair kits without special cases?
- Items are defined in data files (moddable), not code.
- **Loadout slots:** weapon slots (2 primary + sidearm), **belt/vest** slots (fast access: mags, meds, grenades), **backpack** (slow: open inventory screen, time passes, you're vulnerable).
- Carry weight drives stamina and speed.
- **Cut line:** grid/Tetris inventory is out; weight plus slot count only.

### 4.2 Firearms and magazines

- **Question:** does a physical-magazine model make reloading a meaningful decision without being tedious?
- A magazine is an item with a caliber, a capacity and an **ordered stack of rounds** (last in, first out).
- The **chamber is a one-round magazine** on the weapon. Firing pulls from chamber, then cycles from the inserted mag.
- **Reload (key):** swaps to the next compatible mag on belt/vest. Two variants:
  - **Fast reload:** current mag drops to the ground (recoverable).
  - **Retention reload:** slower; current mag goes to a belt/vest slot or backpack.
- **Load mags:** filling mags from loose rounds takes time and happens out of combat (or very riskily in it).
- **Check mag:** an action that gives an approximate count (full / about half / nearly empty), exact when out of combat.
- **Cut lines:** mixed ammo types in one mag (the ordered stack allows it, but no UI for it in the prototype); clips/stripper clips; dual-feed weapons.

### 4.3 Weapon components, durability and repair

- **Question:** does component wear create a reason to scavenge, without micromanagement?
- Each weapon has **4 components**, each with condition 0–100%:

| Component | What wear does |
|---|---|
| Barrel | Accuracy spread grows; at low condition, chance of a squib/stuck round |
| Action (bolt / slide) | Jam chance (clear with an action) |
| Receiver / frame | Jam and misfeed chance; slows wear repair cap |
| Furniture (stock / grip) | Recoil and sway grow |

- Wear per shot is driven by the round type and the weapon's own quality.
- **Repair parts** are keyed by `(component, weapon class)`, and some by caliber: e.g. "rifle bolt parts", "5.45 barrel". Generic toolkits raise the cap of what can be repaired; parts restore condition.
- Repair at a workbench (safe location) is better than field repair.
- Weapons are also **sources of parts**: dismantle a junk weapon to salvage its components.
- **Cut line:** more than 4 components; per-weapon unique parts.

### 4.4 Attachments

- **Question:** can attachments be slot-based and still feel like a real rail system?
- Slots: muzzle, optic, under-barrel, side rail, stock, mag well. Compatibility by mount type (rail, dovetail, proprietary).
- Attachments are items and do not wear (initially).
- **Cut line (deferred until after Phase 3):** "extensions" (conversion kits, caliber swaps, attachments with their own components).

### 4.5 Ammunition crafting

- **Question:** does breaking down rounds give scavenging a second axis without becoming an accounting chore?
- A round breaks down into **casing (caliber-specific)**, **projectile (caliber-specific)**, **powder** and **primer** (both generic, by size class).
- Crafting needs a press (workbench) and components. Powder and primers are the scarce, transferable resources.
- **Cut line:** hand-loads of variable quality; specialist rounds (AP, HP) beyond one tier.

### 4.6 Armor

- **Recommendation: simplify to a single condition value per armor piece** for the prototype.
- Armor has a protection class per body region it covers (head, torso). Hits reduce condition; protection falls with condition.
- **Deferred:** damage types (burn, chemical, psi, shock) until a weapon or anomaly produces them; multi-component armor.

### 4.7 Wounds and healing

- **Question:** does separating limb health from life health create interesting downtime decisions?
- **Body parts:** head, torso, left arm, right arm, left leg, right leg. Each has its own HP.
- **Player HP** is separate. Hits damage the part, and a share flows to Player HP (head and torso a larger share). Player HP at 0 is death. Limbs at 0 are **disabled**, not fatal.
- **Location effects (initial):**
  - Head: vision blur, aim sway; at 0 = death
  - Torso: stamina cap and regen
  - Arms: sway and reload speed; disabled arm forces one-handed (pistol only)
  - Legs: speed, no sprint; disabled leg = limp
- **First aid (quick):** adds **temporary HP** to a part. Temp HP decays over time. Does **not** restore Player HP.
- **Secondary aid (post):** **locks in** current temp HP as real HP, applies a debuff (dizziness, faster hunger/thirst/fatigue), and starts a **heal over time** on Player HP.
- **Player HP regen sources (always over time):** secondary aid, resting at a safe location, food, some meds.
- **Meds are data:** each defines target parts (head+torso, arms, legs, all), aid type (first / secondary), potency, duration, side effects, buffs. Quality tiers vary the combo.
- **Buff meds:** caffeine (fatigue down, thirst up), painkillers (damage-effect suppression, not real healing; masks wound penalties), etc.
- **Bleeding (must-have, Phase 4):** wounds can bleed, draining Player HP until treated. Severity scales with the hit. Bandages and tourniquets stop it; first aid alone may not.
- **Compounding injuries (nice-to-have):** using a damaged limb worsens it: sprinting on a broken leg, looting or reloading with a broken hand, aiming with a wounded arm. Turns "keep going" into a cost decision.
- **Disease and wasting injuries (nice-to-have):** untreated wounds that slowly degrade (infection, wasting), cutting max HP or survival stats until treated.
- **Cut lines:** radiation (until anomalies exist). Nice-to-haves come after Phase 4's core proves out.

### 4.8 Survival stats

- **Question:** do hunger, thirst and fatigue add pressure to the loop, or just chores?
- Three stats, drained by **game time** (not real time), modified by exertion and meds.
- Thresholds apply penalties (stamina, sway, regen), not damage, until the extreme end.
- **Sleep** restores fatigue, passes time (healing timers advance), and is the moment the **Director plans the next day** (§4.10). Sleep only in safe spots.
- **Cut lines:** temperature, disease, food spoilage.

### 4.9 Economy and jobs

- **Question:** does a visible profit ledger make players weigh every fight?
- Currency plus barter value on every item. Traders buy below and sell above value; faction standing shifts prices.
- **Ledger:** after every job, a breakdown: payout and loot value against rounds fired, component wear (valued at repair cost), armor wear, meds and food used. Net profit is the headline number.
- **Jobs:** templated (fetch, hunt, recover, deliver, escort), generated from the Director's world state, not hand-written quests.
- **Cut lines:** player-owned stash economy, investments, "fund things" (TBD in your notes) until the core ledger proves out.

### 4.10 Living world: the Director (A-Life equivalent, name TBD)

- **Question:** can a daily planner plus a cheap offline sim produce a world that feels alive?
- **World shape:** several discrete zones (maps) linked by **boundary points**. NPCs spawn at boundaries and travel between zones through them.
- **Actors:** belong to a faction (Freedom-like, Duty-like, Ecologist-like, Bandits, Mutants, Independents). Faction relations form a matrix (hostile / neutral / friendly), including toward the player.
- **Goals:** each actor's goal list = faction goals ∪ individual goals, merged into one weighted list.
- **Daily planning (at sleep or day roll-over):** the Director enumerates actors, picks objectives from their goal lists, assigns routes and targets for the day, and handles population (spawns, culling, faction balance).
- **Two fidelity levels:**
  - **Offline zones:** abstract sim. Travel is timed movement along the zone graph; encounters resolve by dice rolls weighted by strength and gear; loot changes hands abstractly.
  - **Active zone (player present):** actors are live: real pathing, combat and looting. They can clear loot ahead of you or be there when you arrive.
- **Handover:** an actor entering the active zone is materialized at the boundary or at its sim position; an actor leaving is folded back into the abstract sim with its current state.
- **Cut lines:** diplomacy and dialogue trees (faction standing changes only); emergent quests beyond templated jobs; more than ~5 zones.

### 4.11 Enemy AI: grades, groups and factions

- **Question:** can enemies who differ in skill, and act as units, produce fights that read as people, not turrets?
- **Rule that bounds everything:** skill changes *behavior*, never toughness. Enemies of every grade take the same hits to kill (§6, no bullet sponges). Difficulty may change the mix of grades, not their damage or health.
- **Grade (veterancy):** each human enemy has one of five grades: **rookie, novice, experienced, veteran, professional**. A sixth, **legend**, is reserved for named NPCs and deferred. A grade is one data profile with three layers:
  - *Execution:* aim error, reaction time from spotting to first shot, how often it pauses before or between bursts.
  - *Tactics:* how early it takes cover, whether it relocates under fire, whether it pushes or holds. Rookies stand in the open and freeze; veterans cover, flank and use suppressing fire.
  - *Strength:* one number derived from the profile, used by the offline sim (§4.10) so live fights and dice-rolled fights agree on who is tougher.
- **Readability:** the player can tell grades two or more steps apart from behavior and gear in the first fight, without a label. Neighbouring grades show over several fights. Tactics are announced (callouts, visible moves), so a push or a flank is fair, not clairvoyant.
- **Fixed for life** until Phase 7. Enemies ranking up over time is a nice-to-have for the Director; players will rarely see it.
- **Encounter group:** a small unit (2 to 4 at first) that acts as one. Members share alerts; a gunshot or a sighting alerts nearby groups. A group coordinator chooses the group's stance (push, hold, fall back) from its strength against the threat and its state (ammo, wounded). Fire and maneuver is the base tactic: some members suppress while others move.
- **Leader:** one member per group, normally its highest grade. While a member is near and in sight of the leader, its effective grade is raised one step, never above the leader's own grade. Kill the leader and the lift ends at once.
- **Two tiers:** the Director (§4.10) is strategic: it decides which groups exist, where, and what they are after. The group coordinator is tactical and runs only while the group is live. A group is one object in both: live behavior when the player is present, one strength-weighted dice roll when not.
- **Factions:** a faction sets a group's relations (the §4.10 matrix) and its grade mix and gear. Relations first matter between faction and player. Enemy-versus-enemy fights come with the live zone in Phase 7.
- **Wildlife:** the same group model, simplified: a pack with an alpha as leader, surround and flank, flee when the alpha dies or the pack is hurt. No cover, grenades or suppression. Packs differ by species data, not code.
- **Grenades:** veterans use them to flush a player from cover. Gated on thrown objects and area damage, so cut until after Phase 4.
- **Cut lines:** morale and surrender, dialogue, voice lines beyond a few callouts, enemies that open doors or use vehicles, learning from the player, a player-side suppression effect (first version is AI behavior only), predator-prey ecosystems.

## 5 Modes and levels: how maps are made

| Mode / level | Map source | Notes |
|---|---|---|
| Phase 1 arena | **Pre-made, baked file** | One compact arena with cover, sightlines and verticality; authored outside the engine |
| Testing ranges (weapons, wounds) | **Pre-made, baked file** | Flat range with targets at known distances |
| Zones (Phase 6+) | **Pre-made, baked files** | Hand-authored terrain and structures. Spawn points, loot points, boundaries and safe spots are named markers inside the map file |
| Loot and spawns | **Generated at load / daily** | Procedural placement onto authored markers, driven by the Director |
| Underground / interiors | **Pre-made** (maybe modular later) | Modular generated interiors are a cut candidate |

No procedural geometry in scope. Generation is limited to *what* is placed *where* on authored maps.

## 6 Scope boundaries: what this game is NOT

- Not multiplayer in the prototype (co-op is a later decision; see §9 Q2).
- Not a voxel or destructible-terrain game.
- Not a seamless open world: discrete zones with transitions.
- Not a score-chasing game: score, waves and match timers exist only in the arena mode layer.
- Not story-driven: no authored campaign, no dialogue trees in scope.
- Not an RPG with levels: no XP, no skill trees, no stat points for the player. (Enemy grades, §4.11, are not player levels.)
- No vehicles, base building, or crafting of weapons from scratch.
- No bullet sponges and no damage scaling with difficulty by default.

## 7 Recommended prototyping order

Principle: build the cheapest playable baseline first, then add systems in the order they **create costs**, so the ledger has something to measure by the time it arrives. Each phase ends playable and answers its question from §4.

Milestones follow the phases: FPS-M*n* delivers Phase *n*, and FPS-M1.1 (menus) sits between Phases 1 and 2. The plan is `milestone-plan.md` in the project files.

| Phase | Name | Adds | Answers | Map |
|---|---|---|---|---|
| 1 | **Arena baseline** | Movement, one rifle, **mags + chamber from day one**, **per-body-part hit volumes from day one**, basic AI (patrol, take cover, shoot, chase), waves, death/restart | Does the gunplay feel weighty? | Pre-made arena |
| 1.2 | **Stance and body** | Crouch, prone, lean in every stance, head bob, hit volumes follow posture; a visible first-person body once the engine has skinned characters | Does posture change where I stand and when I peek? | Arena |
| 2 | **Items and loadout** | Data-driven items, belt/vest vs backpack, loot drops from enemies, 3 weapons in 2 calibers | Does one item model carry everything? | Arena |
| 3 | **Weapon wear and repair** | 4 components, jams, repair parts and workbench, dismantling; **end-of-round ledger** (rounds + wear valued) | Does wear drive scavenging? | Arena + range |
| 4 | **Wounds and healing** | Limb HP, Player HP, bleeding, first/secondary aid, meds as data, buff meds | Does limb damage create good downtime? | Arena + range |
| 5 | **Time, survival, rest** | Game clock/day cycle, hunger/thirst/fatigue, sleep at a safe spot, healing timers advance on sleep | Do survival stats add pressure, not chores? | Arena + safe room |
| 6 | **Hub and jobs** | Trader hub, job board, currency, full ledger per job; arena becomes "the job site" | Does profit-as-score change player behavior? | Hub + 1–2 pre-made zones |
| 7 | **Director** | Zone graph, factions, offline dice sim, daily planning, live handover | Does the world feel alive with this little machinery? | 3–5 pre-made zones |
| Later | Depth | Attachments, ammo crafting, armor depth, damage types, anomalies/artifacts | — | — |

Stance and body (1.2) runs in slices parallel to Phase 2, like enemy AI: lean and head bob first (nothing new from the engine), crouch and prone when the engine's per-mode hulls land, the visible body when skinned characters do. Plan: `stance-lean-player-body-proposal.md` in the project files.

Enemy AI (§4.11) is not a phase of its own; it lands in slices alongside the phases:
- **After Phase 1 approval, parallel to Phase 2:** enemy grades (five grades as data; accuracy, reaction time, pauses, cover use; waves shift up the ladder). Needs no items, UI or navigation.
- **Before Phase 3:** group alerts, leader lift, fire and maneuver. Needs cover and exposure queries over the map (§10 item 4).
- **Phase 6:** faction relations toward the player, wildlife packs with the first mutant type.
- **Phase 7:** groups as the Director's unit, enemy-versus-enemy fights, optional rank-up.
- **After Phase 4:** grenades.

Why this order:
- **Mags and hit volumes in Phase 1:** both are expensive to retrofit (they shape weapon code and character setup) and cheap to include at the start.
- **Items before wear:** repair parts, mags and meds are all items; building them without the item model means rebuilding them.
- **Wear before wounds:** wear is testable on a range with no AI changes; wounds need the AI to hit you meaningfully, which Phase 1–2 AI already does.
- **Survival after wounds:** survival stats only matter once time passes and downtime exists, which healing timers create.
- **Ledger early (Phase 3), economy later (Phase 6):** a post-round cost report tests pillar 2 cheaply before building traders and jobs.
- **Arena scaffolding is disposable:** score, waves and timers sit in the mode layer (§3), so dropping them in Phase 6 touches no core rules.
- **Director last:** it needs zones, factions and AI to sim; it's also the system most at risk from a pivot (§9 Q2).

## 8 Rough content plan

| Content | Phase 1 | By Phase 6 | Full target (single-player) |
|---|---|---|---|
| Maps | 1 arena | 1 hub + 2 zones + range | 1 hub + 4–5 zones |
| Weapons | 1 rifle | 5 (pistol, SMG, 2 rifles, shotgun) in 3 calibers | 12–15 across 5–6 calibers |
| Enemy types | 1 human | 2 humans + 1 mutant | 4 human factions + 4–5 mutant types |
| Enemy grades (§4.11) | 5 as data on a type, once approved | 5 | 5 (+ legend for named NPCs) |
| Meds | — | 6–8 | 15–20 |
| Food/drink | — | 4–6 | 10–15 |
| Jobs | — | 3 templates | 6–8 templates |

## 9 Open questions

1. **Setting:** ~~fictional or real?~~ Decided: original, vaguely dystopian sci-fi. Still open: neglected corner of a sophisticated world, or truly post-apocalyptic? (Not blocking until Phase 6.)
2. **Co-op timing:** if co-op is real, it constrains the Director and sim authority early. Recommend: design single-player, keep sim state serializable, decide co-op after Phase 5.
3. **Pivot decision point:** recommend revisiting Destiny-like vs Arma-like vs single-player after Phase 6, when the ledger and jobs are playable.
4. ~~**Bleeding:**~~ Decided: must-have, Phase 4.
5. ~~**Head HP at 0:**~~ Decided: instant death.
6. ~~**Death penalty:**~~ Decided: a difficulty option, either reload on death or permadeath. Phase 1 uses restart.
7. ~~**Ballistics:**~~ Decided: hitscan in Phase 1; physics-based projectiles are a long-term goal.
8. **Difficulty:** your idea of difficulty toggling complexity rather than damage fits pillar 3; sliders per system make modding easier too. Confirm before Phase 5.
9. **Director name:** placeholder "the Director".
10. ~~**Enemy grades:**~~ Decided: five (rookie, novice, experienced, veteran, professional), fixed for life until Phase 7; legend for named NPCs deferred; grades change behavior, never HP or damage.
11. **Suppression:** AI behavior only at first. Does it get a player-side effect (blurred view, spread penalty) once wounds exist (Phase 4)?
12. **Difficulty and AI:** is AI sophistication (grade mix, tactics on or off) part of the complexity toggles in Q8? Decide with Q8.
13. **Rank-up:** may the Director raise an enemy's grade over time (Phase 7 nice-to-have)?

## 10 Engine fit and engine requests

Available from the engine: wgpu rendering (cascaded shadows, SSAO, skybox), a rapier3d
character controller and triangle-mesh colliders, audio, input mapping, the event bus,
and a fixed 60 Hz tick ([ADR-0009](../../_todo/adr/0009-simulation-time-is-one-fixed-tick.md)).
The engine takes world geometry as meshes keyed by a handle
([ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md)); a map format,
loader or generator is FPS-line work. The FPS line never depends on strategy crates
([ADR-0005](../../_todo/adr/0005-crate-lines-and-dependency-direction.md)).

What the game must be able to do (needs, not solutions). Generic needs belong in the engine; FPS-specific ones go in FPS crates.

1. **Load a static level from a file:** render meshes, build colliders, and expose named markers (spawns, loot points, boundaries, safe spots). Meshes and colliders are covered by [ENG-F10](https://github.com/WrackedFella/moho/issues/80); the file format and markers are not. *(Phase 1)*
2. **Hit a specific body part:** raycast against world geometry and against named hit volumes attached to a character, returning which part was hit. *(Phase 1)*
3. **Render characters:** at minimum rigid segmented bodies (Phase 1); skinned, animated characters with bone attachments later, filed as [#543](https://github.com/WrackedFella/moho/issues/543). *(Phase 1 / later)*
4. **AI navigation on non-voxel maps:** compute walkable paths and cover points over an arbitrary static level, and tell whether a spot is exposed to a given position (group tactics, §4.11). *(Phase 1 stopgap; group tactics need it)*
5. **Positional 3D audio:** gunshots and footsteps located and attenuated in space, so players can place fire and callouts and enemies can hear shots. *(Phase 1)*
6. **FPS-owned UI:** HUD, inventory, and menus that don't depend on the strategy game's UI; the generic parts move to the engine. *(Phase 1–2)*
7. **Data-driven definitions from files:** load item, weapon, med and faction definitions from data, so mods can add or override them. *(Phase 2)*
8. **Game time separate from the sim tick:** a game clock that can scale and skip (sleep fast-forward) on top of the fixed 60 Hz tick (ADR-0009). *(Phase 5)*
9. **Save and load world state:** player, inventory, zone and actor state (ADR-0006 contract). *(Phase 5–6)*
10. **Switch between levels:** unload one zone and load another, carrying player state across. *(Phase 6)*
11. **Headless simulation:** run the offline world sim without rendering (ENG-F6 is related). *(Phase 7)*
12. **Posture:** crouch and prone hulls with clearance, lean with clearance in every stance, stance changes that take ticks. Crouch is [ENG-F33](https://github.com/WrackedFella/moho/issues/492); lean and prone are filed as [#544](https://github.com/WrackedFella/moho/issues/544). *(Phase 1.2)*
