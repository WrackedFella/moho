# FPS-F1 — Game Design Document (v0.3)

**Status:** draft v0.3, approved for Phase 1 (v0.3 adds pillar 2, the item lifecycle rule, §4.12 player and weapon stats, the planning rule in §7 and the §4.7 wound penalties). Writing this page is
[FPS-F1](../../_todo/searchlight/FPS-F1-game-design-document/_feature.md)'s deliverable. Open questions are in §9.
**Working pitch:** a survival-focused looter shooter with realistic gunplay, where your gear is your character, your rig is the game, and every fight is a line on a ledger.
**Working target:** single-player first, co-op later, mod support a priority. Pivots (Destiny-like, Arma-like) are open questions, not commitments (§9).

---

## 1 Core pillars

Each pillar is a filter: a feature that serves none of them is cut.

1. **Gear is your character.** No levels, no stat points. Capability comes from what you carry and its condition, on a body whose attributes are fixed (§4.12). Skills that grow with use are a deferred layer that buffs gear, never replaces it (§9 Q14).
2. **You are always short of something.** Managing what you carry, where it sits, what condition it is in and how many uses it has left is core play, not a menu chore. Non-renewable items set the pace: the player is constantly scavenging for the next thing that runs out.
3. **Every encounter has a price.** Rounds, component wear, armor damage, meds, food, time. Success is measured as profit, not kills or survival.
4. **Difficulty comes from systems, not sponges.** Enemies die to a few well-placed rounds; danger comes from jams, wounds, scarcity and bad decisions. Armor is the only legitimate source of tankiness.
5. **The world doesn't wait for you.** Factions, creatures and independents pursue their own goals; you arrive into consequences.
6. **The Zone is unknowable.** (Area X / Roadside Picnic.) Anomalies and artifacts are never fully explained; wrongness is ambient, not a jump scare.

## 2 Genre and setting

- **Genre:** first-person hardcore survival looter shooter (STALKER Anomaly / GAMMA lineage).
- **Perspective:** first-person only.
- **References:** STALKER Anomaly and GAMMA (systems, A-Life), Escape from Tarkov, Road to Vostok and Project Silverfish (inventory, wounds, stance), Area X and Roadside Picnic (tone). Design docs cite these by name instead of listing them again.
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

The wheel turns on scarcity: you grind resources to improve your rig so you can grind better resources, until death ends the run (permadeath at the hardcore end of §9 Q6). Every system below either creates a shortage or lets the player answer one.

Secondary loops (run away, wait it out) exist but are smaller and obvious.

### 3.2 Rules vs modes

- **Core rules** (weapons, items, wounds, survival, economy, Director) know nothing about score, rounds, waves or match timers.
- **Arena scaffolding** (waves, score, round timer, restart) lives in a separate **mode layer** used to test the core rules in Phases 1–5. It is retired as the default experience in Phase 6, when the arena becomes a job site.
- **Arena as a long-term mode** stays possible on the same layer: an optional mode (an in-world fighting pit, or a standalone arena mode) with its own scoring. It is not a commitment.

### 3.3 Micro loop (moment to moment)

- **Movement:** grounded and weighty. Walk / sprint / crouch / prone, with lean as the peek in every stance. Stance changes take a fixed time per step and pass through crouch (modifiers from stats or items are a later revisit); prone crawls, cannot sprint or jump and turns slowly. Stamina limits sprint; carry weight limits stamina. No slide, no double jump, no mantle in Phase 1.
- **Your body is in the world:** the player's hit volumes follow stance and lean, so what sticks out of cover is what gets hit (the cover rule applied to the player). Head bob is a setting; every held action (sprint, crouch, prone, lean) is hold or toggle by a checkbox beside its keybinding.
- **Combat:** few rounds to kill, unforgiving positional damage. Reloading is a commitment (mag in hand, round in chamber). Weapons can fail.
- **Information:** limited HUD. Ammo count is not shown as a number; you check the mag (an action). Condition is inspected, not displayed constantly. The rig's contents show as a small strip that appears after a reload, use or weapon swap and fades; holding the check-gear key shows it on demand. Load is one icon: white for normal, yellow for overloaded, red for overburdened (cannot move).

## 4 Systems

Each system states the question its prototype must answer, the design, and its cut line.

### 4.1 Items and loadout (keystone: everything depends on it)

- **Question:** can one data-driven item model carry weapons, mags, rounds, meds, food, repair kits and tools without special cases?
- Items are defined in data files (moddable), not code. Three classes: **weapons** (held, fire or strike), **consumables** (used up: rounds, meds, food, parts, kits) and **tools** (held or applied: toolkit, crowbar, detector). The tool class exists from Phase 2; the first tool item, the toolkit, arrives in Phase 3 (§4.3), and the rest wait until the world has something for them to act on. Items differ by make, condition, contents and attachments, never by rarity tier or random affix.
- **Lifecycle rule:** an item with a **condition (%)** can be repaired; an item with a **number of uses (#)** cannot, and leaves the game when spent. Never both on one item; an item with neither (attachments, magazines for now) does not change. Balancing the spawn rate, job rewards and burn rate of the #-items is the game loop's main tuning knob (pillar 2).
- **Three layers of access:** *hands* (what you hold), the *rig* (belt and vest: fast slots reached by hotkeys, no screen) and the *backpack* (everything else, reached through the inventory screen while time keeps passing). Anything used in a fight must already be on the rig; reload never looks in the backpack.
- **Gear defines your containers:** the rig and the backpack are items. A rig sets how many fast slots you have and of which kind; a backpack sets how much you can carry. Changing what you wear changes the shape of your inventory. Worn gear weighs what it weighs.
- **Equipment slots:** primary 1, primary 2, sidearm, rig, backpack, body armor, helmet. A suit slot arrives with anomalies.
- **Grid inventory is the destination, not the start.** Items carry a size and containers a cell count from the first data file, so a Tarkov-style grid can be added without reshaping the data. The first inventory screen is a list over that data (capacity enforced by cells and weight, no dragging); the grid view is added to taste after playtest (§7, §9 Q17).
- **Looting:** bodies and containers open a side-by-side screen and take time. Pickups go to the backpack unless the player puts them on the rig. The player's own dropped magazine returns to a free rig slot on pick-up; a gameplay setting (backpack or rig) comes later.
- Carry weight against the player's carry capacity (§4.12) drives stamina and speed through load thresholds: fine, slowed, no sprint, cannot move. The HUD icon has three states (§3.3): white is fine, yellow covers slowed and no sprint, red is cannot move. The inventory and character screens show the numbers (kg against capacity) with the thresholds marked (§9 Q16).
- **Special equipment (later):** sci-fi items that are situational and carry a drawback (a gravity gun, an x-ray sight as a weapon component). Hard rule: never an overpowered weapon or armor. Shape is open (§9 Q18).
- **Cut lines:** stashes until the ledger proves out (§4.9); rarity, colour or affix loot; detectors and artifact slots until anomalies exist; the grid view until the list has been playtested.

### 4.2 Firearms and magazines

- **Question:** does a physical-magazine model make reloading a meaningful decision without being tedious?
- A magazine is an item with a caliber, a capacity and an **ordered stack of rounds** (last in, first out).
- The **chamber is a one-round magazine** on the weapon. Firing pulls from chamber, then cycles from the inserted mag.
- **Reload (key):** swaps to the next compatible mag on the rig. Two variants:
  - **Fast reload:** current mag drops to the ground (recoverable).
  - **Retention reload:** slower; current mag goes to a rig slot or the backpack.
- **Load mags:** filling mags from loose rounds takes time and happens out of combat (or very riskily in it).
- **Check mag:** an action that gives an approximate count (full / about half / nearly empty), exact when out of combat.
- **Mixed rounds:** round types are data (FMJ, AP, HP and so on), and the ordered stack holds any mix. Loading from different loose stacks in sequence is how a player mixes a mag; the rules fire them in order. This is a standing test case for the magazine model.
- **Cut lines:** a dedicated mixing UI beyond choosing which stack to load from; clips/stripper clips; dual-feed weapons.

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
- **Cleaning is weapon repair.** A cleaning kit restores condition to a whole weapon or to one component; the kit has a number of uses and is gone when spent (§4.1 lifecycle rule). Kits are crafted, bought or found; their ingredients are items that can also be used on their own.
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
- Armor occupies the body armor and helmet slots (§4.1); the rig is a separate slot, so protection and fast access are bought separately. The slots exist in Phase 2 and ship no items; armor items arrive in Phase 3 with their wear, so armor damage joins the first ledger.
- Armor is repaired like weapons are cleaned (§4.3): a repair kit with limited uses, applied to the whole piece or, once armor has parts, to one part.
- **Deferred:** damage types (burn, chemical, psi, shock) until a weapon or anomaly produces them; multi-component armor.

### 4.7 Wounds and healing

- **Question:** does separating limb health from life health create interesting downtime decisions?
- **Body parts:** head, torso, left arm, right arm, left leg, right leg. Head and limbs each have a status pool; the torso has none.
- **Player HP** is separate. Hits damage the part, and a share flows to Player HP (head and torso a larger share). Player HP at 0 is death. Limbs at 0 are **disabled**, not fatal; head at 0 is death.
- **Penalties scale with damage:** a part's debuffs start once its status falls to 80 % and grow in inverse proportion to what is left, so the last few points hurt far more than the first. One curve shape for every part except the torso; at 0 a limb is **Broken** (disabled). Each part declares stat changes against §4.12 by its status, plus effects that are not stats; it never writes a stat directly, so meds can mask it and the character screen can explain it. Penalties of different parts add; for two limbs of a kind the worse limb sets the penalty and the second adds a flat extra once both are below 80 %.
  - Head: steadiness down; tunnel vision (vignette) and blur as effects, worsening with damage until sight is near impossible close to 0. At 0 = death. Temporary statuses such as a concussion right after a blow to the head, with dizziness or blackouts, are deferred (§9 Q20).
  - Torso: a special case with no debuffs. Torso hits drain Player HP, carry the larger share, and are the main source of bleeding.
  - Arms: steadiness, recoil control and handling down. Broken: that arm holds nothing; one Broken arm caps the player to one-handed weapons, two Broken arms to no aiming or firing (moving, inventory and meds still work). No left/right distinction yet (§9 Q19).
  - Legs: sprint speed and stamina while sprinting down first, then walk speed; a limp in camera and footstep rhythm grows with the penalty. **Running on a leg below 60 % causes more damage**, to the leg and to Player HP: the "keep going" cost decision is core, not a nice-to-have. Broken (0 %): the player is forced prone, the one place a wound restricts stance; no sprint, walk capped low, no jump or climb, more fall damage. Both Broken: crawl pace. Short of 0 %, a leg only changes speed and never limits stance; a Splinted leg is not Broken and lets the player stand again. The player can always move.
- **Reading the body:** world first, menu second (§3.3). A limp, a grunt on sprint, trembling sights and blur tell the player before the inspect screen (a body silhouette coloured by part status) confirms it.
- **Healing always reaches max.** Nothing in the base game lowers a pool's maximum (wasting injuries, below, are the nice-to-have exception). Limbs can be healed on the spot with the right first- and secondary-aid combination. **Player HP never heals on the spot:** it returns over game time (§10 item 8) through **recovery timers**, fastest while resting or asleep at a safe spot (§4.8), and the more damage, the longer the wait. High-tier, debuff-heavy meds can speed it for a fight, but the intended experience is nursing wounds: waiting out the timer by crafting, trading or other downtime. The decision this creates is the one the game is built on: sit out the timer and pay for food and lodging, or take a job at half strength.
- **Treatment is layered;** each layer is its own item, so the rig decides what you can fix in the field:
  - **Bleeding (must-have, Phase 4):** wounds can bleed, draining Player HP until treated. Severity scales with the hit. Bandages and tourniquets stop it and do not restore status; a tourniquet caps its limb's effective status at 60 % while applied. First aid alone may not stop a bleed.
  - **First aid (quick):** fills the part with **temporary status** to full, so its penalties stop (or ease, for weaker kits) while the temp status lasts. Temp status is shown in its own colour (GAMMA's yellow against white for real status); it decays and the penalties return. Does **not** restore Player HP. The cheap fix that buys time.
  - **Secondary aid (post):** **locks in** the part's current temp status as real status, applies a debuff (dizziness or a temporary stat penalty; faster hunger, thirst and fatigue once §4.8 exists) and starts a **heal over time** on Player HP. Used after first aid, out of combat; the combination is how a limb is healed in the field.
  - **Splint:** turns a Broken limb into **Splinted**: usable at the penalties of a heavily damaged limb, with a recovery timer that sleep advances; full status when it runs out. A surgical kit that skips the timer is to taste after Phase 4.
  - **Painkillers:** mask a part's penalties for a duration, never change its status, cannot mask Broken; side effects (a temporary stat penalty; thirst once §4.8 exists). The "keep fighting" trade.
- **Side effects use what exists:** Phase 4 meds carry only effects on stats and effects that exist then (§4.12, dizziness). Hunger, thirst and fatigue effects are added in Phase 5, when survival stats arrive (§4.8).
- **Player HP regen sources (always over time, never instant):** recovery timers, secondary aid's heal over time, resting at a safe location, food, high-tier meds with heavy side effects.
- **Meds are perishables and data:** every medical item carries a number of uses (§4.1 lifecycle rule) and leaves the game when spent; none has a condition. Each defines a **tier** and one or more **aid roles** (first, secondary, splint, mask): a bandage is first aid only, a surgical kit secondary only, a high-tier medkit can be both at once. Plus target parts (head+torso, arms, legs, all), potency, duration, side effects, buffs. Bandage, tourniquet and splint are one use; first aid, secondary aid and painkillers are multi-use kits. Spawn rate, job rewards and burn rate of meds are half of the loop's scarcity tuning (pillar 2).
- **Med crafting (later):** higher tiers bundle lower-tier items that also work alone (bandage, painkillers and anti-radiation drugs into one medkit, as Anomaly does). Not a linear ladder: the bundle is convenient but spends every component on use, so the player weighs loose items against the kit. Phase 4 ships a simple tier list; the bundle rule arrives with crafting.
- **Buff meds:** caffeine (fatigue down, thirst up), painkillers (above), etc.
- **Enemies share the model:** the same body, curve and penalty table apply to human enemies, so a leg hit slows a member and stops a push, an arm hit widens spread and a head hit slows reaction (§4.11). First Phase 4 cut is player only; enemies are the first add-to-taste step (§7).
- **Phase 4 first cut** (§7 planning rule): the penalty curve, player penalties, leg compounding, bleeding with bandages, first and secondary aid, splint, inspect screen. **To taste, in order:** enemy debuffs; painkiller masking; tourniquet as a distinct item; temporary head statuses (concussion, blackouts); arm compounding and left/right arms; surgical kit, med crafting and quality tiers.
- **Compounding injuries:** legs are core (above). For arms, a nice-to-have: looting or reloading with a damaged hand, or aiming with a wounded arm, costs status. Actions that cost status, no new mechanism.
- **Disease and wasting injuries (nice-to-have):** untreated wounds that slowly degrade (infection, wasting), cutting max HP or survival stats until treated. The one exception to "healing always reaches max"; it needs its own decision.
- **Cut lines:** radiation (until anomalies exist). Nice-to-haves come after Phase 4's core proves out.

### 4.8 Survival stats

- **Question:** do hunger, thirst and fatigue add pressure to the loop, or just chores?
- Three stats, drained by **game time** (not real time), modified by exertion and meds.
- Thresholds declare stat changes against §4.12 (stamina, steadiness, recovery), not damage, until the extreme end.
- **Sleep** restores fatigue, passes time (healing timers advance), and is the moment the **Director plans the next day** (§4.10). Sleep only in safe spots.
- **Cut lines:** temperature, disease, food spoilage.

### 4.9 Economy and jobs

- **Question:** does a visible profit ledger make players weigh every fight?
- Currency plus barter value on every item. Traders buy below and sell above value; faction standing shifts prices.
- **Ledger:** after every job, a breakdown: payout and loot value against rounds fired, component wear (valued at repair cost), armor wear, meds and food used. Net profit is the headline number.
- **Jobs:** templated (fetch, hunt, recover, deliver, escort), generated from the Director's world state, not hand-written quests.
- **Cut lines:** player-owned stash economy, investments, "fund things" (TBD) until the core ledger proves out.

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
- **Encounter group:** a small unit (2 to 4 at first) that acts as one. Members share alerts; a gunshot or a sighting alerts nearby groups. A group coordinator chooses the group's stance (push, hold, fall back) from its strength against the threat and its state (ammo, wounded). Wounds use the player's body model (§4.7); grades stay fixed, so a wound is a temporary modifier on the execution layer, never a demotion. Enemies do not treat themselves until Phase 7. Fire and maneuver is the base tactic: some members suppress while others move.
- **Leader:** one member per group, normally its highest grade. While a member is near and in sight of the leader, its effective grade is raised one step, never above the leader's own grade. Kill the leader and the lift ends at once.
- **Two tiers:** the Director (§4.10) is strategic: it decides which groups exist, where, and what they are after. The group coordinator is tactical and runs only while the group is live. A group is one object in both: live behavior when the player is present, one strength-weighted dice roll when not.
- **Factions:** a faction sets a group's relations (the §4.10 matrix) and its grade mix and gear. Relations first matter between faction and player. Enemy-versus-enemy fights come with the live zone in Phase 7.
- **Wildlife:** the same group model, simplified: a pack with an alpha as leader, surround and flank, flee when the alpha dies or the pack is hurt. No cover, grenades or suppression. Packs differ by species data, not code.
- **Grenades:** veterans use them to flush a player from cover. Gated on thrown objects and area damage, so cut until after Phase 4.
- **Cut lines:** morale and surrender, dialogue, voice lines beyond a few callouts, enemies that open doors or use vehicles, learning from the player, a player-side suppression effect (first version is AI behavior only), predator-prey ecosystems.

### 4.12 Player and weapon stats

- **Question:** can one modifier layer carry gear, load, wounds, stance and survival so the player can always answer "why am I slow"?
- **Attributes are fixed.** The player has a short sheet of base attributes (names TBD; Strength governs carry capacity) that never change over a character's life. Gear may augment an attribute (an exoskeleton adds Strength) or a stat directly (boots add speed). Everyone starts with the same sheet; starting backgrounds are a §9 fork.
- **Stats** are what systems read. Six at most: **carry capacity**, **movement speed**, **stamina** (pool and recovery), **steadiness** (sway, breath settle), **recoil control** (kick absorbed, return to aim), **handling** (reload, mag check, weapon swap). Each stat is its attribute's baseline plus whatever is true right now; nothing is stored or levelled.
- **Not stats:** toughness or a health pool (armor is the only tankiness, pillar 4; HP stays as §4.7); vision and hearing (head wounds and dizziness are screen and audio **effects**); luck, loot find or trade.
- **Weapon stats** belong to the gun and come from its class, components and condition (§4.3) and attachments (§4.4): **kick** (vertical, horizontal), **spread**, **weight and balance**, **ergonomics**, **reliability**. Damage is the round's (§4.2). **Seam rule:** what reaches the screen is the weapon stat shaped by the player stat: kick felt is kick less recoil control; sway seen is weight and balance less steadiness; reload time is ergonomics scaled by handling. A better gun and a steadier body both help, and the player can tell which did.
- **Modifier layer:** every source declares its changes in three verbs, **add** ("+8 kg carry"), **scale** ("speed −15%") and **cap** ("no sprint"). Adds, then scales, then caps, then a per-stat floor. Sources: equipment (per item while worn; its weight into load), load (thresholds against carry capacity: fine, slowed, no sprint, cannot move), wounds (§4.7), stance (crouch, prone, lean while held), survival (§4.8), meds and food (timed, and **suppression** of another source: painkillers mask wounds), artifacts (later), skills (deferred, §9 Q14).
- **Items reach the sheet by the repair rule:** an item with a % contributes by its condition and comes back when repaired or cleaned; an item with a number of uses is a timed change that consumes a use and is never a permanent source. The body follows the same rule: limb HP is a %, healing is repair. Wasting injuries (§4.7) are the one permanent change, and only downward.
- **Presentation follows the number:** limping, heavy breathing and a slow reload are read off resolved stats and effects; no system animates a penalty the sheet does not show.
- **Inspection:** a character screen (not HUD, §3.3) lists each stat with its contributors (*pack −8%, left leg −20%, painkiller suppresses left leg*). This is where the game feels RPG-like.
- **Order (start simple):** Phase 1 nothing; the arena's recoil and sprint constants move behind the layer in Phase 2. Phase 2 births the layer with only the three stats load touches (carry capacity, movement speed, stamina), the three verbs, equipment and load as sources, and a weight readout. Steadiness, recoil control and handling arrive with their first real source (furniture wear in Phase 3, wounds in Phase 4, stance when scheduled).
- **Cut lines:** more than six stats; any stat or attribute that grows with use; a visible level; per-character random attributes (until the backgrounds fork is decided).

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
- Not an RPG with levels: no XP levels, no stat points, no attribute that grows with use (§4.12). Skills that grow are deferred and, if they come, buff on top of gear (§9 Q14). (Enemy grades, §4.11, are not player levels.)
- No vehicles, base building, or crafting of weapons from scratch.
- No bullet sponges and no damage scaling with difficulty by default.

## 7 Recommended prototyping order

Principle: build the cheapest playable baseline first, then add systems in the order they **create costs**, so the ledger has something to measure by the time it arrives. Each phase ends playable and answers its question from §4.

Planning rule for complex systems: start with the simplest feature set we know we need, playtest, then add or remove to taste. If the inventory wants more management, move toward Tarkov; if less, toward unmodified STALKER. Data shapes that are expensive to retrofit (item size, container cells, ordered rounds) go in first; the UI that exploits them comes when playtest asks for it.

Milestones follow the phases: FPS-M*n* delivers Phase *n*, and FPS-M1.1 (menus) sits between Phases 1 and 2. The plan is the [Searchlight roadmap](roadmap.md).

| Phase | Name | Adds | Answers | Map |
|---|---|---|---|---|
| 1 | **Arena baseline** | Movement, one rifle, **mags + chamber from day one**, **per-body-part hit volumes from day one**, basic AI (patrol, take cover, shoot, chase), waves, death/restart | Does the gunplay feel weighty? | Pre-made arena |
| 1.2 | **Stance and body** | Crouch, prone, lean in every stance, head bob, hit volumes follow posture; a visible first-person body once the engine has skinned characters | Does posture change where I stand and when I peek? | Arena |
| 2 | **Items and loadout** | Data-driven items with the % or # uses lifecycle rule, rig and backpack as items, a tool class, carry weight through the stat layer (§4.12), loot drops from enemies, 3 weapons in 2 calibers | Does one item model carry everything? | Arena |
| 3 | **Weapon wear and repair** | 4 components, jams, repair parts and workbench, dismantling, armor items and their wear, the toolkit, limited-use cleaning kits crafted from components that also work alone; **end-of-round ledger** (rounds + wear valued) | Does wear drive scavenging? | Arena + range |
| 4 | **Wounds and healing** | A minimal game clock, damage-scaled penalties, leg compounding, recovery timers, bleeding, first/secondary aid, splint, meds as perishable data; enemy wounds to taste | Does limb damage create good downtime? | Arena + range |
| 5 | **Time, survival, rest** | Day cycle, time scaling and skipping, hunger/thirst/fatigue, sleep at a safe spot, healing timers advance on sleep | Do survival stats add pressure, not chores? | Arena + safe room |
| 6 | **Hub and jobs** | Trader hub, job board, currency, full ledger per job; arena becomes "the job site" | Does profit-as-score change player behavior? | Hub + 1–2 pre-made zones |
| 7 | **Director** | Zone graph, factions, offline dice sim, daily planning, live handover | Does the world feel alive with this little machinery? | 3–5 pre-made zones |
| Later | Depth | Attachments, ammo crafting, armor depth, damage types, anomalies/artifacts | — | — |

Stance and body (1.2) runs in slices parallel to Phase 2, like enemy AI: lean and head bob first (nothing new from the engine), crouch and prone when the engine's per-mode hulls land, the visible body when skinned characters do. Plan: the FPS-M1.2 section of the [Searchlight roadmap](roadmap.md#5-fps-m12-stance-and-body-fps-f4).

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
- **Ledger early (Phase 3), economy later (Phase 6):** a post-round cost report tests pillar 3 cheaply before building traders and jobs.
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

Settled entries keep their numbers so references stay stable.

1. **Setting:** Original, vaguely dystopian sci-fi. Open: neglected corner of a sophisticated world, or truly post-apocalyptic? (Not blocking until Phase 6.)
2. **Co-op timing:** if co-op is real, it constrains the Director and sim authority early. Recommend: design single-player, keep sim state serializable, decide co-op after Phase 5.
3. **Pivot decision point:** recommend revisiting Destiny-like vs Arma-like vs single-player after Phase 6, when the ledger and jobs are playable.
4. **Bleeding:** must-have, Phase 4.
5. **Head HP at 0:** instant death.
6. **Death penalty:** a difficulty option, either reload on death or permadeath. Phase 1 uses restart.
7. **Ballistics:** hitscan in Phase 1; physics-based projectiles are a long-term goal.
8. **Difficulty:** difficulty that toggles complexity rather than damage fits pillar 4; sliders per system make modding easier too. Confirm before Phase 5.
9. **Director name:** placeholder "the Director".
10. **Enemy grades:** five (rookie, novice, experienced, veteran, professional), fixed for life until Phase 7; legend for named NPCs deferred; grades change behavior, never HP or damage.
11. **Suppression:** AI behavior only at first. Does it get a player-side effect once wounds exist (Phase 4)? The head-wound vision effects (§4.7) are the natural reuse.
12. **Difficulty and AI:** is AI sophistication (grade mix, tactics on or off) part of the complexity toggles in Q8? Decide with Q8.
13. **Rank-up:** may the Director raise an enemy's grade over time (Phase 7 nice-to-have)?
14. **Skills that grow:** attributes and stats never grow; a separate skill layer may grow with use and buff stats through §4.12's modifier layer. Deferred: no milestone needs it, and the first version must state its anti-grind rule (a skill may not rise from an action that costs nothing on the ledger). Attribute names (Strength, …) are also still TBD.
15. **Starting backgrounds:** different baseline attribute sheets and starting kits (Anomaly's faction start) as a one-time pick. Decide at Phase 6 with the hub.
16. **Character screen:** raw numbers (kg, %) with load thresholds marked, since the screen is a deliberate inspection; the HUD shows only the white, yellow or red load icon (§3.3).
17. **Grid view timing:** after which playtest does the inventory gain the drag-and-drop grid view over its cell data (§4.1)? Default: after FPS-M2's list has been played.
18. **Special equipment:** weapons and armor only, or individual components too? What counts as a drawback? Decide before the first such item is carded (after Phase 4).
19. **Left and right arms:** one arm penalty table for now. Does a dominant-arm distinction (two-handed aim lost with either, pistol hand matters) earn its animation and handedness cost? Decide after the Phase 4 playtest.
20. **Temporary head statuses:** a concussion right after a blow to the head (dizziness, short blackouts behind an accessibility toggle) as an event status on top of the damage curve. Deferred; decide from the Phase 4 playtest.
21. **Inventory in awkward postures:** may the inventory screen open while prone or leaning, and does it cost extra time? Default: it opens in every stance at the usual cost; revisit after the FPS-M1.2 playtest.
22. **Repair vocabulary:** repair parts, cleaning kits, armor repair kits and the toolkit (which raises the repair cap, §4.3) are four items with one rule each. Does the toolkit carry uses, a condition, or neither (§4.1 lifecycle rule)? Decide before FPS-M3 is carded, with what the Phase 3 ledger values as "repair cost".
23. **Tool slots:** the slot list (§4.1) has none. Reserve quick tool slots with the first tool item (the Phase 3 toolkit), or let tools live on the rig? Decide with that card.
24. **One-handed weapons:** a Broken arm caps the player to one-handed weapons (§4.7), so the item model needs that class or flag. Name it when Phase 2 is carded; pistols are the first case.
25. **Ledger scope:** the Phase 3 end-of-round ledger and the Phase 6 full ledger (§4.9) list overlapping lines. Which lines belong to Phase 3 (rounds, wear, armor, meds), and which wait for the economy (payouts, loot value, food, lodging)?

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
8. **Game time separate from the sim tick:** a game clock that can scale and skip (sleep fast-forward) on top of the fixed 60 Hz tick (ADR-0009). A minimal clock that only counts game time arrives in Phase 4 for recovery timers; scaling and skipping arrive in Phase 5. *(Phase 4 / 5)*
9. **Save and load world state:** player, inventory, zone and actor state (ADR-0006 contract). *(Phase 5–6)*
10. **Switch between levels:** unload one zone and load another, carrying player state across. *(Phase 6)*
11. **Headless simulation:** run the offline world sim without rendering (ENG-F6 is related). *(Phase 7)*
12. **Posture:** crouch and prone hulls with clearance, lean with clearance in every stance, stance changes that take ticks. Crouch is [ENG-F33](https://github.com/WrackedFella/moho/issues/492); lean and prone are filed as [#544](https://github.com/WrackedFella/moho/issues/544). *(Phase 1.2)*
