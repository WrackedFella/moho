# FPS-F1 — Game Design Document (v0.2)

**Status:** approved draft; open questions in §9
**Working pitch:** a survival-focused looter shooter with realistic gunplay, where your gear is your character and every fight is a line on a ledger.
**Working target:** single-player first, co-op later, mod support a priority. Pivots (Destiny-like, Arma-like) are open questions, not commitments (§9).

---

## 1. Core pillars

Each pillar is a filter: a feature that serves none of them is cut.

1. **Gear is your character.** No XP, no skill trees. Capability comes from what you carry and its condition.
2. **Every encounter has a price.** Rounds, component wear, armor damage, meds, food, time. Success is measured as profit, not kills or survival.
3. **Difficulty comes from systems, not sponges.** Enemies die to a few well-placed rounds; danger comes from jams, wounds, scarcity and bad decisions. Armor is the only legitimate source of tankiness.
4. **The world doesn't wait for you.** Factions, creatures and independents pursue their own goals; you arrive into consequences.
5. **The Zone is unknowable.** (Area X / Roadside Picnic.) Anomalies and artifacts are never fully explained; wrongness is ambient, not a jump scare.

## 2. Genre and setting

- **Genre:** first-person hardcore survival looter shooter (STALKER Anomaly / GAMMA lineage).
- **Perspective:** first-person only.
- **Setting:** an original, vaguely dystopian sci-fi exclusion zone around an unexplained event. Emphasis on Area X tone: beauty and dread, nature reclaiming, rules of reality that bend locally.
- **Still open (deliberately):** whether the Zone is a neglected corner of a sophisticated world, or the world is truly post-apocalyptic. Weapons stay real-world-inspired either way. Not blocking until Phase 6 (§9 Q1).

## 3. Core loop

### Macro loop (the hamster wheel)

1. **Take a job** (fetch, hunt, escort, recover, deliver) with a known payout.
2. **Prepare:** spend money and items to kit out (ammo, mags, meds, food, repairs).
3. **Travel** into danger (zone transitions).
4. **Encounter:** combat, evasion, or waiting it out. Resources drain.
5. **Recover:** wounds force downtime (healing timers); survival stats force rest and food.
6. **Take stock:** repair, craft ammo/food/meds, sell loot.
7. **Ledger:** did income beat expenditure? Profit is the score.

Secondary loops (run away, wait it out) exist but are smaller and obvious.

### Rules vs modes

- **Core rules** (weapons, items, wounds, survival, economy, Director) know nothing about score, rounds, waves or match timers.
- **Arena scaffolding** (waves, score, round timer, restart) lives in a separate **mode layer** used to test the core rules in Phases 1–5. It is retired as the default experience in Phase 6, when the arena becomes a job site.
- **Arena as a long-term mode** stays possible on the same layer: an optional mode (an in-world fighting pit, or a standalone arena mode) with its own scoring. It is not a commitment.

### Micro loop (moment to moment)

- **Movement:** grounded and weighty. Walk / sprint / crouch / prone (prone is a cut candidate). Stamina limits sprint; carry weight limits stamina. No slide, no double jump, no mantle in Phase 1.
- **Combat:** few rounds to kill, unforgiving positional damage. Reloading is a commitment (mag in hand, round in chamber). Weapons can fail.
- **Information:** limited HUD. Ammo count is not shown as a number; you check the mag (an action). Condition is inspected, not displayed constantly.

## 4. Systems

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

## 5. Modes and levels: how maps are made

| Mode / level | Map source | Notes |
|---|---|---|
| Phase 1 arena | **Pre-made, baked file** | One compact arena with cover, sightlines and verticality; authored outside the engine |
| Testing ranges (weapons, wounds) | **Pre-made, baked file** | Flat range with targets at known distances |
| Zones (Phase 6+) | **Pre-made, baked files** | Hand-authored terrain and structures. Spawn points, loot points, boundaries and safe spots are named markers inside the map file |
| Loot and spawns | **Generated at load / daily** | Procedural placement onto authored markers, driven by the Director |
| Underground / interiors | **Pre-made** (maybe modular later) | Modular generated interiors are a cut candidate |

No procedural geometry in scope. Generation is limited to *what* is placed *where* on authored maps.

## 6. Scope boundaries: what this game is NOT

- Not multiplayer in the prototype (co-op is a later decision; see §9 Q2).
- Not a voxel or destructible-terrain game.
- Not a seamless open world: discrete zones with transitions.
- Not a score-chasing game: score, waves and match timers exist only in the arena mode layer.
- Not story-driven: no authored campaign, no dialogue trees in scope.
- Not an RPG with levels: no XP, no skill trees, no stat points.
- No vehicles, base building, or crafting of weapons from scratch.
- No bullet sponges and no damage scaling with difficulty by default.

## 7. Recommended prototyping order

Principle: build the cheapest playable baseline first, then add systems in the order they **create costs**, so the ledger has something to measure by the time it arrives. Each phase ends playable and answers its question from §4.

| Phase | Name | Adds | Answers | Map |
|---|---|---|---|---|
| 1 | **Arena baseline** | Movement, one rifle, **mags + chamber from day one**, **per-body-part hit volumes from day one**, basic AI (patrol, take cover, shoot, chase), waves, death/restart | Does the gunplay feel weighty? | Pre-made arena |
| 2 | **Items and loadout** | Data-driven items, belt/vest vs backpack, loot drops from enemies, 3 weapons in 2 calibers | Does one item model carry everything? | Arena |
| 3 | **Weapon wear and repair** | 4 components, jams, repair parts and workbench, dismantling; **end-of-round ledger** (rounds + wear valued) | Does wear drive scavenging? | Arena + range |
| 4 | **Wounds and healing** | Limb HP, Player HP, bleeding, first/secondary aid, meds as data, buff meds | Does limb damage create good downtime? | Arena + range |
| 5 | **Time, survival, rest** | Game clock/day cycle, hunger/thirst/fatigue, sleep at a safe spot, healing timers advance on sleep | Do survival stats add pressure, not chores? | Arena + safe room |
| 6 | **Hub and jobs** | Trader hub, job board, currency, full ledger per job; arena becomes "the job site" | Does profit-as-score change player behavior? | Hub + 1–2 pre-made zones |
| 7 | **Director** | Zone graph, factions, offline dice sim, daily planning, live handover | Does the world feel alive with this little machinery? | 3–5 pre-made zones |
| Later | Depth | Attachments, ammo crafting, armor depth, damage types, anomalies/artifacts | — | — |

Why this order:
- **Mags and hit volumes in Phase 1:** both are expensive to retrofit (they shape weapon code and character setup) and cheap to include at the start.
- **Items before wear:** repair parts, mags and meds are all items; building them without the item model means rebuilding them.
- **Wear before wounds:** wear is testable on a range with no AI changes; wounds need the AI to hit you meaningfully, which Phase 1–2 AI already does.
- **Survival after wounds:** survival stats only matter once time passes and downtime exists, which healing timers create.
- **Ledger early (Phase 3), economy later (Phase 6):** a post-round cost report tests pillar 2 cheaply before building traders and jobs.
- **Arena scaffolding is disposable:** score, waves and timers sit in the mode layer (§3), so dropping them in Phase 6 touches no core rules.
- **Director last:** it needs zones, factions and AI to sim; it's also the system most at risk from a pivot (§9 Q2).

## 8. Rough content plan

| Content | Phase 1 | By Phase 6 | Full target (single-player) |
|---|---|---|---|
| Maps | 1 arena | 1 hub + 2 zones + range | 1 hub + 4–5 zones |
| Weapons | 1 rifle | 5 (pistol, SMG, 2 rifles, shotgun) in 3 calibers | 12–15 across 5–6 calibers |
| Enemy types | 1 human | 2 humans + 1 mutant | 4 human factions + 4–5 mutant types |
| Meds | — | 6–8 | 15–20 |
| Food/drink | — | 4–6 | 10–15 |
| Jobs | — | 3 templates | 6–8 templates |

## 9. Open questions

1. **Setting:** ~~fictional or real?~~ Decided: original, vaguely dystopian sci-fi. Still open: neglected corner of a sophisticated world, or truly post-apocalyptic? (Not blocking until Phase 6.)
2. **Co-op timing:** if co-op is real, it constrains the Director and sim authority early. Recommend: design single-player, keep sim state serializable, decide co-op after Phase 5.
3. **Pivot decision point:** recommend revisiting Destiny-like vs Arma-like vs single-player after Phase 6, when the ledger and jobs are playable.
4. ~~**Bleeding:**~~ Decided: must-have, Phase 4.
5. ~~**Head HP at 0:**~~ Decided: instant death.
6. ~~**Death penalty:**~~ Decided: a difficulty option, either reload on death or permadeath. Phase 1 uses restart.
7. ~~**Ballistics:**~~ Decided: hitscan in Phase 1; physics-based projectiles are a long-term goal.
8. **Difficulty:** your idea of difficulty toggling complexity rather than damage fits pillar 3; sliders per system make modding easier too. Confirm before Phase 5.
9. **Director name:** placeholder "the Director".

## 10. Engine requests

What the game must be able to do (needs, not solutions). Generic needs belong in the engine; FPS-specific ones go in FPS crates.

1. **Load a static level from a file:** render meshes, build colliders, and expose named markers (spawns, loot points, boundaries, safe spots). Meshes and colliders are covered by [ENG-F10](../../_todo/engine/ENG-F10-world-geometry-from-any-source/_feature.md); the file format and markers are not. *(Phase 1)*
2. **Hit a specific body part:** raycast against world geometry and against named hit volumes attached to a character, returning which part was hit. *(Phase 1)*
3. **Render characters:** at minimum rigid segmented bodies (Phase 1); skeletal animation later. *(Phase 1 / later)*
4. **AI navigation on non-voxel maps:** compute walkable paths and cover points over an arbitrary static level. *(Phase 1)*
5. **Positional 3D audio:** gunshots and footsteps located and attenuated in space. *(Phase 1)*
6. **FPS-owned UI:** HUD, inventory, and menus that don't depend on strategy-game UI; the generic parts move to the engine. *(Phase 1–2)*
7. **Data-driven definitions from files:** load item, weapon, med and faction definitions from data, so mods can add or override them. *(Phase 2)*
8. **Game time separate from the sim tick:** a game clock that can scale and skip (sleep fast-forward) on top of the fixed 60 Hz tick (ADR-0009). *(Phase 5)*
9. **Save and load world state:** player, inventory, zone and actor state (ADR-0006 contract). *(Phase 5–6)*
10. **Switch between levels:** unload one zone and load another, carrying player state across. *(Phase 6)*
11. **Headless simulation:** run the offline world sim without rendering (ENG-F6 is related). *(Phase 7)*
