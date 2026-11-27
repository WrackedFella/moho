# FPS-F1 — Game Design Document (v0.22)

**Status:** draft v0.23, approved for Phase 1 (v0.3 added pillar 2, the item lifecycle rule, §4.12 player and weapon stats, the planning rule in §7 and the §4.7 wound penalties; v0.4 adds the prototype gate in §7, gear that adds containers, the attachment system and a mocked grenade in the prototype, and a shotgun with a tube-fed reload; v0.5 sets the world structure: a hub and missions into hand-made maps built on a modular kit, with generated maps from that kit as a later phase, §4.13; v0.6 makes simulated ballistics a core system in four realism tiers, §4.14, and takes magazine wear out; v0.7 fixes how maps are loaded: discrete levels loaded whole and linked by transitions, larger than the arena from Phase 3, with a streamed world deferred, §5; v0.8 names the map editor: Blender with a Searchlight add-on that places the game's map components, §5; v0.9 makes the head bob a figure-8 that leg wounds reshape, and gives the player's shots spread that movement and rapid fire widen, shown by the crosshair, §3.3, §4.7; v0.10 makes every container keep its own contents, so a dropped backpack takes what is in it, with a quick-drop key as an open question, §4.1, §9 Q37; v0.11 makes difficulty a set of per-system complexity settings chosen by preset, some locked at new game, with Hard as the full design and the prototype, Medium as the default, permadeath as its own toggle, magazine wear back as an option, one rule for kits and their parts, and the mission rating renamed Threat, §4.15). v0.12 makes the armor the rig, with magazine slots by kind and pockets; packs and pockets hold by weight, not cells, and nest; normal and quick reloads with quick opt-in; and the ammo check a readout setting with three coloured bands and the round type, §4.1, §4.2, §4.6; v0.13 sets the grid view: a growing grid per container, sorted on demand, with colour-coded status overlays, §4.1, §9 Q17); v0.14 makes each ammo type one data profile and lets the barrel scale muzzle velocity, §4.14; v0.15 lets sprint or fire cancel a reload until the new magazine seats, §4.2; v0.16 gives every weapon a resting spread, the cone left when the player is settled, worsened by condition and shaped by steadiness, whose first source is stance, §3.3, §4.3, §4.12, §9 Q40; v0.17 simplifies the inventory to one grid: packs and armor only add to or scale carry capacity and hold nothing of their own, nothing refuses a pick-up for room, the quick-drop key and nested packs are cut, and placing a bag as a container is deferred with object placement, §4.1, §4.2, §4.6, §6, §9 Q37, Q41; it reverses v0.10's per-container contents and v0.12's per-pack maximum; v0.18 marks loot in the world: a soft dot over anything lootable that fades with distance, and a look-at panel that names the item beside its pip and, if the player keeps looking, slides open its details, one coloured pip per weapon or armor part, both HUD settings; holding the interact key uses a med or food from the world or equips gear, §3.3, §4.1, §4.3, §4.15, §9 Q42, Q43); v0.20 names the weapon slots primary, secondary and sidearm, with melee to come in the standard loadout, adds fire modes (semi-auto, burst of 3 by default, full-auto) and lets a weapon swap cancel a reload from Phase 2, §4.1, §4.2; v0.22 drops the setting that picked what the reload key does: R is always the normal reload and the quick reload is opt-in on its own key, §4.2; v0.23 reads the ammo check in four bands by thirds (full, mostly full, about half, almost empty) plus empty, a reading on a third going to the lower band, with one pip per sixth; rounds leave the muzzle, aimed at the crosshair point, and a handedness setting mirrors it, §4.2, §4.14, §4.15, §9 Q19). Writing this page is
[FPS-F1](../../_todo/searchlight/FPS-F1-game-design-document/_feature.md)'s deliverable. Open questions are in §9.
**Working pitch:** a survival-focused looter shooter with realistic gunplay, where your gear is your character, your rig is the game, and every fight is a line on a ledger.
**Working target:** single-player first, co-op later, mod support a priority. Pivots (Destiny-like, Arma-like) are open questions, not commitments (§9).

---

## 1 Core pillars

Each pillar is a filter: a feature that serves none of them is cut.

1. **Gear is your character.** No levels, no stat points. Capability comes from what you carry and its condition, on a body whose attributes are fixed (§4.12). Skills that grow with use are a deferred layer that buffs gear, never replaces it (§9 Q14).
2. **You are always short of something.** Managing what you carry, where it sits, what condition it is in and how many uses it has left is core play, not a menu chore. Non-renewable items set the pace: the player is constantly scavenging for the next thing that runs out.
3. **Every encounter has a price.** Rounds, component wear, armor damage, meds, food, time. Success is measured as profit, not kills or survival.
4. **Difficulty comes from systems, not sponges.** Enemies die to a few well-placed rounds; danger comes from jams, wounds, scarcity and bad decisions. Armor is the only legitimate source of tankiness. Difficulty chooses how complex a system is, never how tough anyone is (§4.15).
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

The wheel runs as **hub and missions** (§4.13): the player lives in a hub between missions, takes a job from the board, and travels out to a hand-made map to do it. Steps 1, 2, 6 and 7 happen in the hub; 3 to 5 on the mission.

Secondary loops (run away, wait it out) exist but are smaller and obvious.

### 3.2 Rules vs modes

- **Core rules** (weapons, items, wounds, survival, economy, Director) know nothing about score, rounds, waves or match timers.
- **Arena scaffolding** (waves, score, round timer, restart) lives in a separate **wave layer** used to test the core rules in Phases 1–5. It is retired as the default experience in Phase 6, when the arena becomes a job site.
- Wave attacks may return later as a job type; no map has a mode.

### 3.3 Micro loop (moment to moment)

- **Movement:** grounded and weighty. Walk / sprint / crouch / prone, with lean as the peek in every stance. Stance changes take a fixed time per step and pass through crouch (modifiers from stats or items are a later revisit); prone crawls, cannot sprint or jump and turns slowly. Stamina limits sprint; carry weight limits stamina. No slide, no double jump, no mantle in Phase 1.
- **Your body is in the world:** the player's hit volumes follow stance and lean, so what sticks out of cover is what gets hit (the cover rule applied to the player). Head bob traces a figure-8, one loop per footstep on that foot's side, and is a setting; it only moves the view. Every held action (sprint, crouch, prone, lean) is hold or toggle by a checkbox beside its keybinding.
- **Combat:** few rounds to kill, unforgiving positional damage. Stand-and-shoot beats run-and-gun: moving and rapid fire widen the spread, and it closes again to the weapon's resting spread when the player settles; a steadier body (crouched, prone, unhurt) tightens all of it. The crosshair is always the centre of the shot, and its gap shows the current spread. Reloading is a commitment (mag in hand, round in chamber). Weapons can fail.
- **Information:** limited HUD, the Hard default (§4.15): each HUD element has its own switch, and the HUD preset fills them in. On Hard the ammo count is not shown as a number; you check the mag (an action), or hover a magazine in the inventory. The constant count and the check replace each other. Condition is inspected, not displayed constantly. The rig's contents show as a small strip that appears after a reload, use or weapon swap and fades; holding the check-gear key shows it on demand. Load is one icon: white for normal, yellow for overloaded, red for overburdened (cannot move).
- **Loot in the world:** anything the player can loot carries a **loot pip**, a small soft off-white dot that fades in as the player comes within range and out as they leave it. Every lootable thing in range and on screen shows one, whether or not the player is looking at it, while nothing blocks the player's line of sight to it (it never shows through walls). Looking at a lootable thing within reach, within a small cone around the screen centre (about 7°, data), fades in the **look-at panel** behind its pip: a small translucent panel with cut corners whose first line is the pip, the use prompt and the name; if the player keeps looking for half a second, the details slide open (§4.1). Both are HUD elements with their own settings (§4.15); they tell the player where loot is and what it is, never what is inside an unsearched body or crate.

## 4 Systems

Each system states the question its prototype must answer, the design, and its cut line.

### 4.1 Items and loadout (keystone: everything depends on it)

- **Question:** can one data-driven item model carry weapons, mags, rounds, meds, food, repair kits and tools without special cases?
- Items are defined in data files (moddable), not code. Four classes: **weapons** (held, fire or strike), **consumables** (used up: rounds, meds, food, parts, kits), **tools** (held or applied: toolkit, crowbar, detector) and **gear** (worn or fitted, neither used up nor held: rigs, magazines, later backpacks and armor). The tool class exists from Phase 2; the first tool item, the toolkit, arrives in Phase 3 (§4.3), and the rest wait until the world has something for them to act on. Items differ by make, condition, contents and attachments, never by rarity tier or random affix.
- **Lifecycle rule:** an item with a **condition (%)** can be repaired; an item with a **number of uses (#)** cannot, and leaves the game when spent. Never both on one item; an item with neither (attachments, magazines) does not change. Magazines do not wear unless the magazine wear setting is on (§4.15), which gives them a condition; otherwise wear belongs to weapons and armor (§4.3, §4.6). Balancing the spawn rate, job rewards and burn rate of the #-items is the game loop's main tuning knob (pillar 2).
- **Three layers of access:** *hands* (what you hold), the *rig* (the magazine slots on the worn armor or vest: reached by the reload key, no screen) and the *inventory* (one grid holding everything else the player carries, reached through the inventory screen while time keeps passing). Reload only takes from the rig's slots; with none there it does nothing, and the player reloads by hand from the inventory by moving a magazine onto the weapon, in the normal reload time.
- **Gear sets your slots and your capacity:** the armor is the rig. Beside its condition and weight it has magazine slots by kind (some rifle, some pistol; each magazine names in data which kind it takes). A plain chest rig is armor with no protection. Armor, backpacks and other gear never hold items of their own: each only changes the player's **carry capacity** through the modifier layer (§4.12), by adding to it ("+10 kg") or scaling it ("+20 %"), while worn. Worn gear weighs what it weighs: lighter armor gives fewer slots and less capacity, heavier armor gives more and slows the player more (§4.6). A naked player has no magazine slots and only the body's base capacity; it is possible but never expected.
- **One inventory:** everything the player carries outside the hands, the equipment slots and the magazine slots is in one inventory, as in STALKER. Nothing refuses a pick-up for lack of room; what the player carries is limited only by load against carry capacity (below), up to cannot move. A backpack is an item like any other: worn in its slot it adds or scales capacity, carried in the inventory it is only its weight, and it never has contents. Dropping or swapping a worn backpack or armor changes capacity and moves nothing else; a dropped one is a dropped item like any other (a loose object that falls and settles, Phase 2). Bodies and world containers (crates) still hold their own items and show beside the player's inventory when searched. Placing a bag in the world as a container of its own is deferred (§9 Q41).
- **Equipment slots:** primary, secondary, sidearm, body armor (the rig), backpack, helmet. A suit slot arrives with anomalies. The standard weapon loadout is melee, sidearm, primary and secondary: the melee slot, and holding the player to those weapon slots, arrive with the inventory feature that enforces the loadout, not before.
- **Grid inventory is the destination, not the start.** Items carry a size from the first data file, so a grid can be added without reshaping the data. The first inventory screen is a list over that data (no room limit, no dragging); the grid view follows the FPS-M2 list playtest (FPS-F19, §9 Q17). It is not tile-tetris: the inventory is one grid of fixed width that grows rows to fit, so cells never refuse an item; a sort arranges it biggest first; each cell carries a colour-coded status overlay (condition %, uses #, magazine X/Y, white to red as the value falls), with the rest in a hover tooltip and a Details action. The armor's magazine slots stay a separate fixed slot row beside it, so what reload can reach is always called out; a searched body or crate shows as its own grid.
- **Finding loot (loot pips and the look-at panel, §3.3):** the pip marks a loose item, a body with something left on it, or a container not yet emptied; a searched body or crate that is empty loses its pip. Pip range is a setting: **None**, **Near** (about the distance a player notices a dropped magazine) or **Far** (across a room or a small yard); the distances are data. The look-at panel opens on the lootable thing within reach that sits inside the look cone, closest to the screen centre; the use key acts on that same thing, and the panel replaces the plain use prompt: the pip stays and the panel fades in behind it, its first line the use prompt and the name, its second the hold action where there is one. Its setting: **None** (the plain use prompt only), **Simple** (the prompt lines only), **Complex** (details open at once) or **Dynamic** (details slide open after half a second; the default). Shift and the interact key together open or close them at once. Above the details the panel is indented for the pip; the details are not. Details are what the item's inventory cell and tooltip show (condition %, uses #, magazine fill, weight), and for a weapon or armor piece one **condition pip** per part, coloured on the inventory's white-to-red scale, a hollow pip for a missing part (§4.3). A body or crate shows its name and whether it has been searched; its contents only show by searching it.
- **Tap or hold the interact key:** a tap picks a thing up (or searches a body or crate). Holding it, past the same long press as the ammo check, uses a med or food item where it lies, in its normal use time, with any uses left going to the inventory; or takes a weapon or piece of gear and equips it, into an empty slot of its kind or in place of the one there (for a weapon, the one in hand), the replaced item going to the inventory. The hold follows the interact binding; it has none of its own. Anything else treats a hold as a tap.
- **Looting:** bodies and containers open a side-by-side screen and take time. Pickups go to the inventory; a magazine goes into a rig slot by a move or a Put in a slot action. The player's own dropped magazine returns to a free rig slot of its kind on pick-up.
- Carry weight against the player's carry capacity (§4.12: the body's base, plus or scaled by worn gear) drives stamina and speed through load thresholds: fine, slowed, no sprint, cannot move. The HUD icon has three states (§3.3): white is fine, yellow covers slowed and no sprint, red is cannot move. The inventory and character screens show the numbers (kg against capacity) with the thresholds marked (§9 Q16).
- **Special equipment (later):** sci-fi items that are situational and carry a drawback (a gravity gun, an x-ray sight as a weapon component). Hard rule: never an overpowered weapon or armor. Shape is open (§9 Q18).
- **Cut lines:** stashes until the ledger proves out (§4.9); containers inside the inventory (packs that hold items, nesting) and a quick-drop key for worn gear (§9 Q37); placing bags as world containers (§9 Q41); rarity, colour or affix loot; detectors and artifact slots until anomalies exist; the grid view until the list has been playtested.

### 4.2 Firearms and magazines

- **Question:** does a physical-magazine model make reloading a meaningful decision without being tedious?
- A magazine is an item with a caliber, a capacity and an **ordered stack of rounds** (last in, first out).
- The **chamber is a one-round magazine** on the weapon. Firing pulls from chamber, then cycles from the inserted mag.
- **Reload (key):** swaps to the next compatible mag in the rig's slots. Two variants, each with its own binding; the reload key always does the normal one:
  - **Normal reload:** the normal time; the spent mag goes to the inventory.
  - **Quick reload:** slightly shorter; the spent mag drops to the ground (recoverable). Opt-in: the player binds its own key, unbound by default; there is no setting that swaps what the reload key does.
- **Tactical vs empty reload:** a reload is quicker when a round is chambered (swap the mag and keep firing) than when the chamber is empty (swap, then charge the weapon); a gun that ran dry is the slowest case. Both times belong to the weapon, and handling scales them (§4.12). The quick reload subtracts a small saving from these times (§9 Q26). Reloading early costs the rounds left in the mag unless it is retained; reloading late costs time at the worst moment. With no ammo count on the HUD (§3.3), judging that is the decision the physical-mag model exists to create.
- **Load mags:** filling mags from loose rounds takes time and happens out of combat (or very riskily in it).
- **Check mag:** an action that gives an approximate reading in four bands by thirds of capacity: full (every round), mostly full (above two thirds), about half (above one third, up to two thirds) and almost empty (one round up to one third), plus empty (no rounds). A magazine exactly on a third reads the lower band, so the check never tells the player they have more than they do. "+1" marks a chambered round ("Full + 1"). Full and mostly full read white, about half yellow, almost empty and empty red. The check also names the type of the round that fires next. How it reads is a setting: pips (six for every weapon, as a progress bar, one pip to each sixth of capacity, rounded up so any round lights one, and the sixth only when full; the default), words, a constant counter that replaces the check, or none. The exact count is in the inventory (hover or select the magazine).
- **Mixed rounds:** round types are data (FMJ, AP, HP and so on), and the ordered stack holds any mix. Loading from different loose stacks in sequence is how a player mixes a mag; the rules fire them in order. This is a standing test case for the magazine model.
- **Round data:** a round definition carries muzzle velocity, drag, penetration and damage (§4.14), so a mixed magazine fires rounds that fly differently, in order.
- **Cancelling a reload:** sprinting, or firing with a round chambered, cancels a reload until the new magazine seats; from then on the reload finishes (one commit point, as in established shooters). A normal reload cancelled early leaves the old magazine in the weapon. A quick reload cancelled after the old magazine has dropped leaves it on the ground and the weapon with only its chambered round. With an empty chamber, fire does nothing during a reload. Swapping weapons cancels too, from Phase 2, where the pistol and SMG arrive.
- **Fire modes:** semi-auto, burst and full-auto, named per weapon in data. A burst is 3 rounds by default and may be set per weapon. Full-auto arrives with the SMG in Phase 2; burst, a per-weapon burst count and switching between a weapon's modes come later.
- **Reload styles:** the prototype shows more than one. Detachable magazines (rifle, SMG, pistol) and a **tube-fed shotgun** loaded one round at a time, interruptible by firing; a double-barrel break-action is optional. The tube is an integral magazine, so the ordered round stack and chamber rules carry it unchanged. The shotgun arrives in Phase 3, where the action component (§4.3) lives.
- **Ammunition settings (§4.15):** with Magazines (Medium, Hard), a wheel picks among the filled magazines on the rig, and picking one costs a reload's time; without a pick, reload takes the next compatible magazine in rig order. With Pooled (Easy) there are no magazine items: rounds sit in the inventory by type, reload fills the weapon from the pool and keeps the chambered round, and the wheel picks the round type.
- **Cut lines:** other per-weapon-class reload mechanics (open bolt, bolt hold-open, pump action beyond the tube); a separate chamber-check action; a dedicated mixing UI beyond choosing which stack to load from; clips/stripper clips; dual-feed weapons.

### 4.3 Weapon components, durability and repair

- **Question:** does component wear create a reason to scavenge, without micromanagement?
- Each weapon has **4 components**, each with condition 0–100%:

| Component | What wear does |
|---|---|
| Barrel | Resting spread grows (§4.12): at full condition the weapon's class value, at an effective 0 % three times it (the factor is data); at low condition, chance of a squib/stuck round |
| Action (bolt / slide) | Jam chance (clear with an action) |
| Receiver / frame | Jam and misfeed chance; slows wear repair cap |
| Furniture (stock / grip) | Recoil and sway grow |

- Wear per shot is driven by the round type and the weapon's own quality.
- **The weapon's condition is the average of its components,** shown on the item and used by traders for value; the inspect screen shows each one, and so does the look-at panel's details, one condition pip per component (§4.1). Every system asks the weapon for its condition for one effect (accuracy, reliability, recoil) and never reads a component directly, so the Simple and None repair settings (§4.15) answer the same question.
- **A component below 60 % is broken:** below that threshold it counts at double the rate for every effect it drives, so a component at 40 % counts as 20 % (60 − 2 × 20) for jams, spread or recoil. The curve stays continuous at the threshold; the threshold is data.
- **A missing component** (removed or salvaged): the weapon cannot fire. Armor missing a part, once armor has parts, gives no protection or other benefit and is worn for looks only. Either sells for far less.
- **Repair parts** are keyed by `(component, weapon class)`, and some by caliber: e.g. "rifle bolt parts", "5.45 barrel". Generic toolkits raise the cap of what can be repaired; parts restore condition.
- Repair at a workbench (safe location) is better than field repair.
- **Cleaning is weapon repair.** A cleaning kit restores condition to a whole weapon or to one component; the kit has a number of uses and is gone when spent (§4.1 lifecycle rule). Kits are crafted, bought or found.
- **Kits and their parts** (repair and medical alike): a kit's parts work on their own and are focused (replacement springs restore a magazine, a barrel brush cleans a barrel, a bandage stops a bleed); the crafted kit works on anything in its category. Crafting spends the parts; using a kit spends a use. Parts name what they restore and kits their category, in data.
- Weapons are also **sources of parts**: dismantle a junk weapon to salvage its components.
- **Cut line:** more than 4 components; per-weapon unique parts.

### 4.4 Attachments

- **Question:** can attachments be slot-based and still feel like a real rail system?
- Slots: muzzle, optic, under-barrel, side rail, stock, mag well. Compatibility by mount type (rail, dovetail, proprietary).
- Attachments are items and do not wear (initially).
- **Prototype scope:** the attachment *system* lands after Phase 3 (§7, Phase 3.1): slots and mount types in data, fit rules, and swapping a scope and/or a laser sight at run time. The attachment items themselves are mocked: one scope and one laser, crude in effect and art. Weapon components (§4.3) are items and swap by the same fit rules, which makes weapons modular.
- **Cut line (deferred past the prototype):** "extensions" (conversion kits, caliber swaps, attachments with their own components), rail variety beyond the first mount types, a weapon-mounted light, magnified second views.

### 4.5 Ammunition crafting

- **Question:** does breaking down rounds give scavenging a second axis without becoming an accounting chore?
- A round breaks down into **casing (caliber-specific)**, **projectile (caliber-specific)**, **powder** and **primer** (both generic, by size class).
- Crafting needs a press (workbench) and components. Powder and primers are the scarce, transferable resources.
- **Cut line:** hand-loads of variable quality; specialist rounds (AP, HP) beyond one tier.

### 4.6 Armor

- **Recommendation: simplify to a single condition value per armor piece** for the prototype.
- Armor has a protection class per body region it covers (head, torso). Hits reduce condition; protection falls with condition.
- Armor occupies the body armor and helmet slots (§4.1). Body armor is the rig: it carries the magazine slots and adds carry capacity (§4.1), so protection, fast access and carrying are one choice. Heavy armor is heavy and slows the player further, a movement-speed scale declared through the modifier layer (§4.12); light armor trades protection for weight and slots. Phase 2 ships rigs as armor with no protection; protection arrives in Phase 3 with wear, so armor damage joins the first ledger.
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
  - Legs: sprint speed and stamina while sprinting down first, then walk speed; a limp in camera and footstep rhythm grows with the penalty: the wounded leg's side of the head-bob figure-8 shortens, and the bob's sideways swing narrows toward straight up and down as either leg's penalty grows. **Running on a leg below 60 % causes more damage**, to the leg and to Player HP: the "keep going" cost decision is core, not a nice-to-have. Broken (0 %): the player is forced prone, the one place a wound restricts stance; no sprint, walk capped low, no jump or climb, more fall damage. Both Broken: crawl pace. Short of 0 %, a leg only changes speed and never limits stance; a Splinted leg is not Broken and lets the player stand again. The player can always move.
- **Reading the body:** world first, menu second (§3.3). A limp, a grunt on sprint, trembling sights and blur tell the player before the inspect screen (a body silhouette coloured by part status) confirms it.
- **Healing always reaches max.** Nothing in the base game lowers a pool's maximum (wasting injuries, below, are the nice-to-have exception). Limbs can be healed on the spot with the right first- and secondary-aid combination. **Player HP never heals on the spot** (damage setting Full; the Limbs and Health bar settings relax it, §4.15): it returns over game time (§10 item 8) through **recovery timers**, fastest while resting or asleep at a safe spot (§4.8), and the more damage, the longer the wait. High-tier, debuff-heavy meds can speed it for a fight, but the intended experience is nursing wounds: waiting out the timer by crafting, trading or other downtime. The decision this creates is the one the game is built on: sit out the timer and pay for food and lodging, or take a job at half strength.
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
- **Jobs:** templated (fetch, hunt, recover, deliver, escort), generated from the Director's world state, not hand-written quests. A job is a mission (§4.13): the board offers a few, each showing its conditions before the player commits.
- **Cut lines:** player-owned stash economy, investments, "fund things" (TBD) until the core ledger proves out.

### 4.10 Living world: the Director (A-Life equivalent, name TBD)

- **Question:** can a daily planner plus a cheap offline sim produce a world that feels alive?
- **World shape:** several discrete zones (maps) linked by **boundary points**. NPCs spawn at boundaries and travel between zones through them.
- **Arrival and gathering:** NPCs enter a map at its boundaries and transition points (doors, gates, roads leading out), as if they came in from the next map; they never appear at a spawn point inside it. A map marks gathering areas (bases, camps) that NPCs want to reach and hold; these are AI goals, not spawn points. The rest of NPC behavior outside a fight is not yet designed. Until the Director lands, the prototype spawns encounter groups from wave data at enemy spawn points, each standing in for a transition point (§4.11). Each zone is a hand-made mission map (§4.13); the player enters a zone on a mission from the hub, while the Director's state persists between missions.
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
- **Rule that bounds everything:** skill changes *behavior*, never toughness. Enemies of every grade take the same hits to kill (§6, no bullet sponges). A mission's Threat (§4.13) may change the mix of grades, not their damage or health.
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
- **Grenades:** veterans use them to flush a player from cover. Real thrown objects and area damage are cut until after the prototype. The prototype gets a **mocked grenade system** after Phase 4 (§7, Phase 4.1): the throw spawns an object on the ground where the thrower looks, a sound cue plays, and the object despawns. It proves the item, throw and cue path for the player; damage and AI use come with the real version.
- **Cut lines:** morale and surrender, dialogue, voice lines beyond a few callouts, enemies that open doors or use vehicles, learning from the player, a player-side suppression effect (first version is AI behavior only), predator-prey ecosystems.

### 4.12 Player and weapon stats

- **Question:** can one modifier layer carry gear, load, wounds, stance and survival so the player can always answer "why am I slow"?
- **Attributes are fixed.** The player has a short sheet of base attributes (names TBD; Strength governs carry capacity) that never change over a character's life. Gear may augment an attribute (an exoskeleton adds Strength) or a stat directly (boots add speed). Everyone starts with the same sheet; starting backgrounds are a §9 fork.
- **Stats** are what systems read. Six at most: **carry capacity**, **movement speed**, **stamina** (pool and recovery), **steadiness** (how still the body holds the weapon: it shrinks the shot cone, and later sway and breath settle), **recoil control** (kick absorbed, return to aim), **handling** (reload, mag check, weapon swap). Each stat is its attribute's baseline plus whatever is true right now; nothing is stored or levelled.
- **Not stats:** toughness or a health pool (armor is the only tankiness, pillar 4; HP stays as §4.7); vision and hearing (head wounds and dizziness are screen and audio **effects**); luck, loot find or trade.
- **Weapon stats** belong to the gun and come from its class, components and condition (§4.3) and attachments (§4.4): **kick** (vertical, horizontal), **spread** (the **resting spread**: the cone a settled player's rounds scatter in; every weapon has one, above zero), **weight and balance**, **ergonomics**, **reliability**. Damage is the round's (§4.2). **Seam rule:** what reaches the screen is the weapon stat shaped by the player stat: kick felt is kick less recoil control; the shot cone is the weapon's spread, widened by movement and rapid fire, shaped by steadiness; sway seen is weight and balance less steadiness; reload time is ergonomics scaled by handling. A better gun and a steadier body both help, and the player can tell which did.
- **Resting spread and the shot cone:** the cone a shot is drawn from is (resting spread + movement spread + fire spread), under the weapon's cap, divided by steadiness: twice as steady, half the cone. Resting spread is the floor: settling closes the cone to it, never to zero, so a shot always lands inside a small circle centred on the crosshair, and the crosshair's gap at rest shows it. **Inputs:** the weapon's class value (data); its condition for accuracy (§4.3, the barrel); attachments later (§4.4). Steadiness is 1.0 for a standing, unhurt player, and rises or falls by its sources: stance (crouched and prone steadier), wounds (arms and head, §4.7), survival (§4.8), meds and artifacts later; it has a floor, so a cone never grows without limit. Nothing else widens or tightens the cone: difficulty does not (§4.15), and ammunition does not yet (§9 Q40). Bracing on cover or a bipod, and aiming down sights (Phase 3.1), would be later steadiness or weapon sources, not new rules.
- **Modifier layer:** every source declares its changes in three verbs, **add** ("+8 kg carry"), **scale** ("speed −15%") and **cap** ("no sprint"). Adds, then scales, then caps, then a per-stat floor. Sources: equipment (per item while worn; its weight into load), load (thresholds against carry capacity: fine, slowed, no sprint, cannot move), wounds (§4.7), stance (crouch, prone, lean while held), survival (§4.8), meds and food (timed, and **suppression** of another source: painkillers mask wounds), artifacts (later), skills (deferred, §9 Q14).
- **Items reach the sheet by the repair rule:** an item with a % contributes by its condition and comes back when repaired or cleaned; an item with a number of uses is a timed change that consumes a use and is never a permanent source. The body follows the same rule: limb HP is a %, healing is repair. Wasting injuries (§4.7) are the one permanent change, and only downward.
- **Presentation follows the number:** limping, heavy breathing and a slow reload are read off resolved stats and effects; no system animates a penalty the sheet does not show.
- **Inspection:** a character screen (not HUD, §3.3) lists each stat with its contributors (*pack −8%, left leg −20%, painkiller suppresses left leg*). This is where the game feels RPG-like.
- **Order (start simple):** Phase 1 nothing; the arena's recoil and sprint constants move behind the layer in Phase 2. Phase 2 births the layer with only the three stats load touches (carry capacity, movement speed, stamina), the three verbs, equipment and load as sources, and a weight readout. Steadiness arrives with stance, its first real source, when every weapon gains its resting spread (Phase 1.2); recoil control and handling arrive with theirs (furniture wear in Phase 3, wounds in Phase 4).
- **Cut lines:** more than six stats; any stat or attribute that grows with use; a visible level; per-character random attributes (until the backgrounds fork is decided).

### 4.13 Hub and missions

- **Question:** does choosing a mission by what it pays, and what it may cost, make every trip out a decision?
- **Hub:** one persistent place between missions: traders, the job board, the workbench, storage. The hub improves through gear and services (workbench tier, trader stock, storage size), never through character stats (pillar 1, §9 Q30).
- **Mission:** a job (§4.9) taken from the board and played on one hand-made map (§5). The board offers a few missions and shows each one's **conditions** before the player commits: map type, Threat, and modifiers (weather, a faction present, power off). The Director chooses what is on offer; the player chooses between offers, never sets free sliders.
- **Inside a hand-made map, per mission:** which map of a type, the start and exit points, loot and enemy spawns by category, and which building fills each **building slot** (an authored footprint holding one of several pre-built variants). Geometry is never generated in this version.
- **Loot rule:** the **place decides the pool**: a map's type and a site's kind decide which items can appear (science equipment from tech sites, meds from clinics). **Threat decides the amount and the ceiling:** more loot, a higher rarity cap, or one guaranteed reward at a tier. Threat changes the grade mix and density of enemies (§4.11), never their toughness.
- **Later phase, generated maps:** missions on maps assembled from the kit's modules (§5), cut from the hand-made maps. Not before the hand-made version is played.
- **Cut lines:** free parameter sliders, hub upgrades that raise stats, a seamless world, generated geometry before the later phase.

### 4.14 Ballistics

- **Question:** does a simulated round make distance, cover and ammunition choice readable decisions without a ballistics course?
- **Every round fired is a simulated object** stepped by the fixed tick: it takes its muzzle velocity, drag and penetration from its round definition (§4.2), falls under gravity, loses speed in flight, and does less damage the slower it arrives. Each ammo type is one data profile (velocity, drag, penetration, damage), so a new type such as old or +P ammo is data. The weapon's barrel scales the muzzle velocity of the rounds it fires, so the same ammo flies and hits differently from a carbine and a rifle; it never lifts damage above the ammo's own. Pellets are rounds. Enemies fire the same rounds and lead a moving target by grade (§4.11). The host owns every round in flight, so hits stay host-resolved (§9 Q2).
- **Where a round starts:** at the weapon's muzzle, never the eye. Every firearm is held out in front of the eye, so the muzzle sits beside it on the shooting side, below it and ahead of it (offset across, down and forward, so a round never appears in the player's own plane); each weapon names that point in its data. The round is aimed from the muzzle at the point the crosshair covers (or, when nothing is under it, at a fixed distance along its line), so the crosshair stays the centre of the shot (§3.3) while a wall lip that hides the gun but not the eye catches the round. Aiming down sights or through a scope (Phase 3.1) moves the muzzle under the eye, on the centre line, still slightly ahead. The **handedness** setting (Right by default, or Left) mirrors the muzzle to that side. Using and interacting still reach from the eye.
- **Realism stacks in four tiers,** each a difficulty setting (§4.15). Hitscan stays as the *Simple* setting for A/B playtests.

| Tier | Name | Adds | Lands with |
|---|---|---|---|
| 1 (floor) | **Flight** | Travel time, drop, drag, damage by arrival speed, one ricochet off a hard surface at a shallow angle (angle plus dice), enemies lead | Phase 1.3 |
| 2 | **Materials** | Surfaces have a material; a round pierces thin cover, bodies and armor by its penetration against resistance and condition, with blunt damage when stopped | Phase 3 |
| 3 | **Atmosphere** | Wind, per-round drag curves, scope zeroing | Phase 3.1 |
| 4 (ceiling) | **Marksman** | Heart rate and breath sway the aim, steadied by holding breath and by posture (Sniper Elite's realistic preset without its assists) | Phase 5 |

- **Where it is felt:** in the arena a rifle round drops about 6 cm at 100 m, so tier 1 shows as leading a runner (about 0.7 m at 100 m), ricochets and weak pistols at range. Drop becomes a decision past 200 m, on the range and on the larger hand-made mission maps (§4.13), where long sightlines make tiers 2 and 3 matter. A **gravity scale** is a tuning knob so drop can be read on short maps.
- **Impacts are seen:** from Phase 1.3 a round in flight is drawn as a tracer and leaves a plain mark where it strikes a world surface, so spread, drop and a bounce can be read. Later: marks and impact sounds by surface material (tier 2, Phase 3), and visible wound sites on characters where they were hit (with the wounds work, §4.7, Phase 4).
- **Friendly fire:** an enemy round wounds anyone it reaches, other enemies included, and an enemy holds fire while an ally blocks its line to the target (Phase 1.3).
- **Cut lines:** spin drift, Coriolis, temperature and altitude; fragmentation and deformation; more than one ricochet; ricochet off bodies; visible bullet meshes (a tracer is drawn instead); bullets that move loose objects or hit other bullets; rewinding time for shots in co-op.

### 4.15 Difficulty and complexity settings

- **Question:** can a player choose how much of the game's machinery they manage, without the game ever making enemies tougher or the player weaker?
- **Rule (pillar 4):** difficulty never scales damage, health, accuracy or toughness. It chooses **how complex each system is**: whether weapons have parts, whether limbs are wounded, whether rounds live in magazines. Enemies differ by grade and by the mission's **Threat** (grade mix and density, §4.11, §4.13), never by a multiplier.
- **Hard is the full design; Medium is the default.** The system sections (§4.2, §4.3, §4.7) describe Hard, and the prototype builds Hard. A new game opens on Medium, for players who want a more streamlined, classic shooter. Easier settings take machinery away; they never add rules Hard lacks.
- **Presets pick settings; settings are the truth.** Easy, Medium and Hard are named bundles of per-system settings, defined in data (a mod can add a preset). Changing any setting turns the label to *Custom*. **Every combination is legal:** no pair is forbidden; where a setting reads an input another setting removed, the input counts as perfect (jams with repair None read only round reliability and luck) or takes a forced value (magazine wear reads Off with repair None).
- **Two kinds of setting:**
  - **Locked at new game:** settings that decide what the world generates (loot tables, NPC gear, trader stock, which items exist and what they track). They are set when the game starts, kept with the run, and shown read-only afterwards; changing one means a new game.
  - **Live:** settings that only change how a rule resolves from now on (a jam roll, a round's flight, what the HUD shows). They change at any time and take effect at the next tick, like any other command.
- **Kept where:** settings the simulation reads, locked or live, belong to the run and go in its save once saves exist (ADR-0040: everything the tick reads is saved); before saves, a run lasts until the player quits. HUD settings are presentation and stay a player preference outside the save. In co-op the host's run settings bind everyone, and each player keeps their own HUD.

| System | Easy | Medium (default) | Hard (prototype) | Changes |
|---|---|---|---|---|
| Repair and durability | **None:** weapons and armor never wear; no repair items spawn or sell | **Simple:** one condition % per weapon and armor piece; repair items spawn whole, fairly often | **Components:** §4.3; components with their own condition, aggregated into the item's; kits spawn partly used and rare, are assembled from sourced parts, or bought dear. Armor keeps one condition until armor parts are built (§4.6) | Locked |
| Magazine wear | Off | Off | **Off by default;** optional **On:** a magazine has a condition %, wears with use and feeds jams (§4.1). With repair Simple a cleaning kit restores it; with Components a magazine part (replacement springs) or one use of a cleaning kit. Reads Off with repair None (§9 Q39) | Locked |
| Damage and healing | **Health bar:** one HP pool; armor mitigates; hit location still scales damage; a health kit restores HP on the spot | **Limbs:** §4.7 parts, penalty curve, Broken and bleeding; one basic medkit that stops a bleed, treats a limb and restores some HP on the spot | **Full:** §4.7 as written; HP never heals on the spot. Meds assembled from parts when med crafting is built | Locked |
| Ammunition | **Pooled:** no magazine items; reload fills the weapon from rounds in the inventory, keeping the chambered round; a wheel picks the round type | **Magazines:** §4.2, with the wheel picking among filled magazines (a reload's time) | **Magazines:** as Medium | Locked |
| Jams | **Off** | **Forgiving:** from the weapon's condition, low and gentle | **Full:** from weapon components, magazine condition (with magazine wear on), round reliability and luck; steep enough to make repair pay | Live |
| HUD | **Full:** constant ammo count, health, stance, status conditions, load, crosshair; loot pips Far; later compass, map, threats, stealth | **Partial:** health and threat markers hidden; ammo check as Pips; loot pips Near | **Minimal:** §3.3: stance, status conditions and load only; ammo check as Pips; loot pips Near | Live, per player |
| Ballistics (§4.14) | **Simple:** hitscan | **Flight:** tier 1 (and tier 2 when built) | Every tier built | Live; a round in flight keeps the rules it was fired with |
| Permadeath (§9 Q6) | A toggle on the New Game screen, set by no preset, off by default. On: death ends the run. Off: reload on death | | | Locked both ways |

- **HUD and ammo:** each HUD element has its own switch, and the HUD preset only fills them in. The constant ammo count and the ammo check (§4.2 check mag) replace each other: Full shows the count with no check; Partial and Minimal hide the count and the check reads as Pips. On Hard the exact count is in the inventory, by hovering a magazine. The player can still set the readout by hand (On HUD, Pips, Text, None). Colour coding of the readout is an accessibility switch outside the preset.
- **HUD and loot:** loot pips and the look-at panel are HUD elements like the rest. The HUD preset sets pip range (Full: Far; Partial and Minimal: Near) and leaves the panel on Dynamic in every preset; the player can set either by hand. The panel's condition pips follow the repair setting: one pip per component with Components, one pip for the whole item with Simple, none with None. The colour coding switch covers condition pips too.
- **Why these locks:** repair, magazine wear, damage and ammunition each change the item set or what items track. Going down loses information (four component conditions); going up has to invent it for every item in the world. Jams, ballistics and the HUD read the world without changing what is in it, so they stay live. Permadeath is a promise made at the start of a run; switching it either way mid-run would make it meaningless, and it is a separate choice from how complex the systems are.
- **Stubs:** a setting whose behaviour is not built yet (magazine wear, armor parts, med crafting) exists in the run with one value and no row on the page, so building it later adds no new item kind or data shape.
- **What Hard keeps pluggable,** so an easier setting is a data set and a small rule later, not a rewrite. Each goes into the Tech Lead notes of the card that builds the system:
  1. Systems read their setting from the run; none assumes Hard. Until the Difficulty page exists every run is Hard with permadeath off.
  2. Loot, enemy loadouts and trader stock are data tables per setting.
  3. Every system asks an item for its condition for one effect (§4.3); None answers 100 %, Simple its one %, Components the matching component with the broken rule. Armor asks the same way for protection.
  4. A magazine's data allows a condition, absent while magazine wear is off; each kit part names what it restores and each kit its category (§4.3).
  5. A hit resolves in two steps: which part and how much damage (the same in every setting, so enemies die to the same rounds), then the body model applies it. HP exists in every setting.
  6. Meds are aid roles in data (§4.7). The Easy and Medium kits carry an "HP on the spot" role that Full's tables never spawn.
  7. Reload asks one source for the next feed: a magazine, or the inventory pool. The chamber exists in every setting.
  8. Jam chance is one formula over named inputs (weapon condition, magazine condition, round reliability, luck), its weights in data per setting; a missing input reads as perfect.
  9. Every HUD element switches on its own, and nothing in the simulation reads the HUD.
- **Where the settings sit:** one **Difficulty** page beside the settings pages (Controls, Mouse, Audio, Graphics, Display, Game). The preset is at the top, one row per system below; locked rows carry a lock mark. The same page is the **New Game screen**: opened from New Game every row is editable, the preset starts on Medium and Start replaces Apply; opened from the pause menu, locked rows are read-only. It is not listed in the main menu's Settings, since run settings belong to a run. Its HUD preset row writes the per-element switches that live on the **Game** page, beside field of view, head bob, crosshair, handedness, the ammo readout, loot pips and the look-at panel; it is marked as the player's preference for all games, the one row not kept with the run.
- **Order:** the prototype builds Hard and the pluggable shapes above, since they are cheap now and costly to retrofit. Easier settings and the Difficulty page come after the prototype gate, each with its system's feature; Pooled ammunition is the dearest.
- **Cut lines:** per-setting sliders with free numbers (settings are discrete modes, so playtests compare like with like); damage, health or accuracy multipliers of any kind; changing a locked setting on an existing run or save; per-mission overrides of run settings (missions vary by Threat). AI sophistication and survival stats as rows wait until their phases are played (§9 Q12).


## 5 Modes and levels: how maps are made

| Mode / level | Map source | Notes |
|---|---|---|
| Phase 1 arena | **Pre-made, baked file** | One compact arena with cover, sightlines and verticality; authored outside the engine. The home map until Phase 3; a test level after that |
| First large map, with the range (Phase 3+) | **Pre-made, baked file on the modular kit** | The first level larger than the arena: 300 m plus sightlines for ballistics tiers 2 and 3 (§4.14), a range with targets at known distances, and the map that fixes the kit's metrics (§9 Q31). Replaces the arena as the home map |
| Hub (Phase 6+) | **Pre-made, baked file** | One persistent map between missions (§4.13) |
| Mission maps, zones (Phase 6+) | **Pre-made, baked files on the modular kit** | Hand-made terrain and structures. Spawn points, loot points, start and exit points, boundaries, building slots, safe spots and kit connection points are named markers inside the map file |
| Loot, spawns, starts and exits | **Generated at load / daily** | Procedural placement onto authored markers, driven by the Director and the mission's conditions |
| Buildings in slots | **Generated at load** | One pre-built variant per authored slot that fits its footprint |
| Underground / interiors | **Pre-made on the kit** | Modular generated interiors come with the later phase |
| Generated mission maps (later phase) | **Generated at load from kit modules** | Areas of the hand-made maps fitted together at kit connection points. Seam checks and a seeded retry are part of the work |

Until the later phase, no procedural geometry: generation is limited to *what* is placed *where* on authored maps.

**Levels and transitions:** the world is a set of discrete levels. A level loads whole, with its own colliders, navigation and markers, and the player moves between levels through a transition at an exit marker, with a load in between (the STALKER model). Nothing streams. This decides three things:

- **Size:** a level is as large as one load and one physics world allow, and no larger than a few kilometres across, so positions keep sub-millimetre precision in single-precision floats. The arena is small; the first large map (Phase 3) and the mission maps (Phase 6) are large enough that rounds drop and wind matters (§4.14) without a gravity scale.
- **Bounds:** a level has an authored boundary; a round or actor that leaves it is removed. A round never crosses a transition.
- **Co-op:** every player is on the same level; a transition moves the party together (§9 Q2).

A streamed or cell-based world, where the engine loads and unloads parts of one map around each player, is a separate decision with engine cost of its own (streamed colliders and navigation, per-cell origins, saves per cell). It is not taken now and is revisited only if level transitions hurt the loop (§9 Q33).

**Modular kit:** every hand-made map is built from day one on one kit with fixed metrics (grid size, wall, floor and door heights) and marked connection points, so the later phase can cut modules from existing maps instead of rebuilding them. The kit's numbers are fixed with the first map (§9 Q31).

**Authoring:** maps are built in **Blender** with a **Searchlight add-on**. The add-on places the game's **map components** as typed objects: player and enemy spawns, waypoints, cover and its direction, dummies and the level's mode now; loot points, building slots, boundaries, safe spots, kit connection points and anomalies as the features that use them arrive. A component is a point, an oriented point or a volume, with a kind chosen from the game's data (an anomaly's kind, a loot point's site kind) and typed overrides on its defaults. The add-on exports the glTF the game loads, and can launch the game on the open map and list the problems the game would find in it. The components are defined once, and the add-on and the game both read that definition, so a feature that adds a component adds it to both. Users: the team; opening it to modders is §9 Q34. The add-on never ships inside the game.

**Art:** test maps are blocked out by hand on the kit in Blender (Authoring above); no generator is written for them. Placeholder assets are free and licence-compatible (CC0 preferred, ADR-0007). Bespoke assets are commissioned later, to the kit's metrics.

## 6 Scope boundaries: what this game is NOT

- Not multiplayer in the prototype (co-op is a later decision; see §9 Q2).
- Not a voxel or destructible-terrain game.
- Not a seamless open world: a hub and missions into discrete levels loaded whole and linked by transitions (§4.13, §5); a streamed world is a later decision (§9 Q33).
- Not a score-chasing game: waves are a prototype test feature.
- Not story-driven: no authored campaign, no dialogue trees in scope.
- Not an RPG with levels: no XP levels, no stat points, no attribute that grows with use (§4.12). Skills that grow are deferred and, if they come, buff on top of gear (§9 Q14). (Enemy grades, §4.11, are not player levels.)
- No vehicles or crafting of weapons from scratch. No base building or object placement before V1; placing objects in the world, starting with bags as containers, is a deferred idea (§9 Q41).
- No bullet sponges and no damage scaling with difficulty (§4.15).

## 7 Recommended prototyping order

Principle: build the cheapest playable baseline first, then add systems in the order they **create costs**, so the ledger has something to measure by the time it arrives. Each phase ends playable and answers its question from §4.

Planning rule for complex systems: start with the simplest feature set we know we need, playtest, then add or remove to taste. If the inventory wants more management, move toward Tarkov; if less, toward unmodified STALKER. Data shapes that are expensive to retrofit (item size, ordered rounds) go in first; the UI that exploits them comes when playtest asks for it.

**Prototype gate:** the prototype is done when Phases 1 through 5 and the inserts 1.2, 1.3, 3.1 and 4.1 are playable, with enemy grades and group tactics (§4.11) and placeholder art and animation (free or generated assets, not production quality) in place. Test levels with scripted states (spawn enemies and items, set a limb, set a survival meter, advance the clock) are part of the gate. The systems are the deliverable: a mocked attachment or grenade counts when the system around it is real. At the gate `dev` is promoted to `main`, and the repository split is considered with the Engine project. Cut order if time presses: grenades, then attachments, then the fifth archetype.

Milestones follow the phases: FPS-M*n* delivers Phase *n*, and FPS-M1.1 (menus) sits between Phases 1 and 2. The plan is the [Searchlight roadmap](roadmap.md).

| Phase | Name | Adds | Answers | Map |
|---|---|---|---|---|
| 1 | **Arena baseline** | Movement, one rifle, **mags + chamber from day one**, **per-body-part hit volumes from day one**, basic AI (patrol, take cover, shoot, chase), waves, death/restart | Does the gunplay feel weighty? | Pre-made arena |
| 1.2 | **Stance and body** | Crouch, prone, lean in every stance, head bob, hit volumes follow posture; a visible first-person body once the engine has skinned characters | Does posture change where I stand and when I peek? | Arena |
| 1.3 | **Bullets fly** | Simulated rounds (§4.14 tier 1): travel time, drop, drag, damage by arrival speed, one ricochet; enemies lead; hitscan kept as the Simple toggle | Does a round in flight make distance a decision? | Arena (drop read with the gravity scale until the Phase 3 large map exists) |
| 2 | **Items and loadout** | Data-driven items with the % or # uses lifecycle rule, rig and backpack as items, a tool class, carry weight through the stat layer (§4.12), loot drops from enemies, 3 weapons in 2 calibers | Does one item model carry everything? | Arena |
| 3 | **Weapon wear and repair** | Wear on weapons and armor only (magazine wear is a later option, §4.15): 4 components, jams, repair parts and workbench, dismantling, armor items and their wear, the toolkit, limited-use cleaning kits crafted from components that also work alone; a tube-fed shotgun (§4.2); surface materials and round penetration through cover and armor (§4.14 tier 2); **end-of-round ledger** (rounds + wear valued) | Does wear drive scavenging? | First large map (range) + arena as a test level |
| 3.1 | **Attachments** | The attachment system (§4.4): mount types, slots, fit rules, swap a scope and/or a laser; items mocked; weapon components swap by the same rules; wind and scope zeroing (§4.14 tier 3) | Can a weapon be rebuilt in the field without special cases? | Arena + range |
| 4 | **Wounds and healing** | A minimal game clock, damage-scaled penalties, leg compounding, recovery timers, bleeding, first/secondary aid, splint, meds as perishable data; enemy wounds to taste | Does limb damage create good downtime? | Arena + range |
| 4.1 | **Grenades (mocked)** | A grenade item and throw: an object spawns on the ground at the sight point, a sound cue plays, the object despawns (§4.11). Cuttable | Does a throwable fit the item model? | Arena + range |
| 5 | **Time, survival, rest** | Day cycle, time scaling and skipping, hunger/thirst/fatigue, sleep at a safe spot, healing timers advance on sleep; heart rate and breath sway (§4.14 tier 4) | Do survival stats add pressure, not chores? | Arena + safe room |
| 6 | **Hub and missions** | Trader hub, job board offering missions with their conditions, currency, full ledger per job; random starts, exits, spawns and building slots on hand-made maps; the place-decides-pool loot rule; arena becomes "the job site" | Does profit-as-score change player behavior? | Hub + 2 hand-made mission maps on the kit |
| 7 | **Director** | Zone graph, factions, offline dice sim, daily planning, live handover | Does the world feel alive with this little machinery? | 3–5 pre-made zones |
| Later | Generated maps | Mission maps assembled from kit modules cut from the hand-made maps (§4.13, §5) | Does assembly add variety the hand-made pool lacks? | Kit modules |
| Later | Depth | Richer attachments and extensions, ammo crafting, armor depth, real grenades with area damage, damage types, anomalies/artifacts | — | — |

Stance and body (1.2) runs in slices parallel to Phase 2, like enemy AI: lean and head bob first (nothing new from the engine), crouch and prone when the engine's per-mode hulls land, the visible body when skinned characters do. Plan: the FPS-M1.2 section of the [Searchlight roadmap](roadmap.md#5-fps-m12-stance-and-body-fps-f4).

Bullets fly (1.3) runs beside Phase 2 and lands before Phase 2 fixes its item data files, since a round's ballistic fields belong to the round item. It needs no items, UI or engine work itself.

Enemy AI (§4.11) is not a phase of its own; it lands in slices alongside the phases:
- **After Phase 1 approval, parallel to Phase 2:** enemy grades (five grades as data; accuracy, reaction time, pauses, cover use; waves shift up the ladder). Needs no items, UI or navigation.
- **Before Phase 3:** group alerts, leader lift, fire and maneuver. Needs cover and exposure queries over the map (§10 item 4).
- **Phase 6:** faction relations toward the player, wildlife packs with the first mutant type.
- **Phase 7:** groups as the Director's unit, enemy-versus-enemy fights, optional rank-up.
- **After Phase 4:** grenades.

Why this order:
- **Mags and hit volumes in Phase 1:** both are expensive to retrofit (they shape weapon code and character setup) and cheap to include at the start.
- **Bullets before items:** round ballistics are fields on the round item, so they exist before Phase 2 fixes the data shape.
- **Items before wear:** repair parts, mags and meds are all items; building them without the item model means rebuilding them.
- **Wear before wounds:** wear is testable on a range with no AI changes; wounds need the AI to hit you meaningfully, which Phase 1–2 AI already does.
- **Survival after wounds:** survival stats only matter once time passes and downtime exists, which healing timers create.
- **Ledger early (Phase 3), economy later (Phase 6):** a post-round cost report tests pillar 3 cheaply before building traders and jobs.
- **Arena scaffolding is disposable:** score, waves and timers sit in the wave layer (§3), so dropping them in Phase 6 touches no core rules.
- **Director last:** it needs zones, factions and AI to sim; it's also the system most at risk from a pivot (§9 Q2).

## 8 Rough content plan

| Content | Phase 1 | By Phase 6 | Full target (single-player) |
|---|---|---|---|
| Maps | 1 arena | 1 hub + 2 mission maps + range | 1 hub + 4–5 zones, several mission maps each |
| Weapons | 1 rifle | 5 (pistol, SMG, 2 rifles, shotgun) in 3 calibers | 12–15 across 5–6 calibers |
| Enemy types | 1 human | 2 humans + 1 mutant | 4 human factions + 4–5 mutant types |
| Enemy grades (§4.11) | 5 as data on a type, once approved | 5 | 5 (+ legend for named NPCs) |
| Meds | — | 6–8 | 15–20 |
| Food/drink | — | 4–6 | 10–15 |
| Jobs | — | 3 templates | 6–8 templates |

## 9 Open questions

Settled entries keep their numbers so references stay stable.

1. **Setting:** Original, vaguely dystopian sci-fi. Open: neglected corner of a sophisticated world, or truly post-apocalyptic? (Not blocking until Phase 6.)
2. **Co-op timing:** the engine targets co-op for up to four players with a trusted host that runs the AI, and a casual competitive mode stays possible. The simulation takes one command per player slot, host actions read slot 0 only, and the host resolves hits. The game's co-op design waits until after Phase 5.
3. **Pivot decision point:** recommend revisiting Destiny-like vs Arma-like vs single-player after Phase 6, when the ledger and jobs are playable.
4. **Bleeding:** must-have, Phase 4.
5. **Head HP at 0:** instant death.
6. **Death penalty:** settled (§4.15): permadeath is a toggle on the New Game screen, off by default, set by no preset and fixed for the run in both directions. Phase 1 uses restart.
7. **Ballistics:** core, decided 2026-10-26. Rounds are simulated from Phase 1.3 (§4.14 tier 1: travel time, drop, drag, one ricochet); materials and penetration land with Phase 3, wind and zeroing with Phase 3.1, breath and heart rate with Phase 5. Hitscan stays as the Simple toggle.
8. **Difficulty:** settled by §4.15: presets over per-system complexity settings, never damage.
9. **Director name:** placeholder "the Director".
10. **Enemy grades:** five (rookie, novice, experienced, veteran, professional), fixed for life until Phase 7; legend for named NPCs deferred; grades change behavior, never HP or damage.
11. **Suppression:** AI behavior only at first. Does it get a player-side effect once wounds exist (Phase 4)? The head-wound vision effects (§4.7) are the natural reuse.
12. **Difficulty and AI:** AI sophistication (grade mix, tactics on or off) may become a §4.15 setting later, with survival stats; not before their phases are played. A mission's Threat already sets its grade mix (§4.13).
13. **Rank-up:** may the Director raise an enemy's grade over time (Phase 7 nice-to-have)?
14. **Skills that grow:** attributes and stats never grow; a separate skill layer may grow with use and buff stats through §4.12's modifier layer. Deferred: no milestone needs it, and the first version must state its anti-grind rule (a skill may not rise from an action that costs nothing on the ledger). Attribute names (Strength, …) are also still TBD.
15. **Starting backgrounds:** different baseline attribute sheets and starting kits (Anomaly's faction start) as a one-time pick. Decide at Phase 6 with the hub.
16. **Character screen:** raw numbers (kg, %) with load thresholds marked, since the screen is a deliberate inspection; the HUD shows only the white, yellow or red load icon (§3.3).
17. **Grid view timing:** answered (v0.13): after FPS-M2's list has been played, as FPS-F19; the grid grows to fit and nothing refuses an item for room. v0.17: one grid for the whole inventory, with the magazine slots as their own row (§4.1).
18. **Special equipment:** weapons and armor only, or individual components too? What counts as a drawback? Decide before the first such item is carded (after Phase 4).
19. **Left and right arms:** one arm penalty table for now. Handedness is a setting that mirrors the gun (§4.14). Does the dominant arm also matter to wounds (two-handed aim lost with either, pistol hand matters)? Decide after the Phase 4 playtest.
20. **Temporary head statuses:** a concussion right after a blow to the head (dizziness, short blackouts behind an accessibility toggle) as an event status on top of the damage curve. Deferred; decide from the Phase 4 playtest.
21. **Inventory in awkward postures:** may the inventory screen open while prone or leaning, and does it cost extra time? Default: it opens in every stance at the usual cost; revisit after the FPS-M1.2 playtest.
22. **Repair vocabulary:** repair parts, cleaning kits, armor repair kits and the toolkit (which raises the repair cap, §4.3) are four items with one rule each. Does the toolkit carry uses, a condition, or neither (§4.1 lifecycle rule)? Decide before FPS-M3 is carded, with what the Phase 3 ledger values as "repair cost".
23. **Tool slots:** the slot list (§4.1) has none. Reserve quick tool slots with the first tool item (the Phase 3 toolkit), or let tools live on the rig? Decide with that card.
24. **One-handed weapons:** a Broken arm caps the player to one-handed weapons (§4.7), so the item model needs that class or flag. Name it when Phase 2 is carded; pistols are the first case.
25. **Ledger scope:** the Phase 3 end-of-round ledger and the Phase 6 full ledger (§4.9) list overlapping lines. Which lines belong to Phase 3 (rounds, wear, armor, meds), and which wait for the economy (payouts, loot value, food, lodging)?
26. **Reload time stacking:** answered 2026-11-25: the normal reload takes the tactical or empty time; the quick reload takes it less a small saving (§4.2).

27. **Armor and rig slots:** answered 2026-11-25: the armor is the rig, with magazine slots by kind (§4.1, §4.6); since v0.17 its carrying is a carry-capacity modifier, not pockets. Open: whether clothing is its own layer under armor.
28. **Mission persistence:** does a map keep its state between missions (looted containers, dead enemies, cleared sites), or is each mission a fresh instance? The Director's faction state persists either way (§4.10). Decide when Phase 6 is carded.
29. **Failed mission cost:** what does dying or failing a mission cost: nothing beyond the death penalty, the loadout carried, or an entry deposit? Extends Q6. Decide with Q6.
30. **Hub upgrades:** the hub improves through gear and services only (§4.13). Which ones (workbench tier, trader stock, storage size), and how does that sit with §6 (no base building before V1), the §4.9 stash cut line and placed containers (Q41)? Decide when Phase 6 is carded.
31. **Kit metrics:** grid size, wall, floor and door heights, and the connection-point convention of the modular kit (§5). Fix them with the first hand-made map, with the Tech Lead.
32. **Breath and heart rate:** are hold-breath and heart rate stats through the §4.12 modifier layer, and does holding breath cost fatigue? Decide when Phase 5 is carded (§4.14 tier 4).
33. **Streamed world:** levels load whole and link by transitions (§5). Does a later phase want one streamed map (cells loaded around each player) instead, and what would it buy that larger levels do not? Not before Phase 6 has been played; if it comes, it is an engine feature with its own request (streamed colliders and navigation, per-cell origins, saves per cell).
34. **Player-made maps:** the map editor (§5) is for the team. Does mod support (header) cover maps, and would that mean shipping the add-on to modders or building an editor inside the game (an engine feature)? Not before Phase 6.
35. **Crosshair offset:** a setting that moves the crosshair, and the shot centre with it, up from the screen centre (Halo MCC's option). Decide with the next settings card after FPS-F18.
36. **Crosshair styles:** a choice of crosshair styles, or none; the default is the cross whose gap opens with spread (FPS-F18). Decide with the next settings card after FPS-F18.
37. **Quick drop and nested containers:** closed (v0.17, Justin 2026-11-26): cut. Packs no longer hold items (§4.1), so there is nothing to drop as a unit and nothing to nest; a worn pack is dropped like any item.
38. **Reincarnation:** a permadeath option where the player, on death, possesses a random NPC in the world and the run continues as them. Far post-MVP; needs the Director (§4.10).
39. **Magazine condition and reload speed:** with magazine wear on (§4.15), does a worn magazine also slow the reload, through handling (§4.12)? Decide when magazine wear is built.
40. **Ammunition and resting spread:** does a round type widen the resting spread (Old ammo less consistent, match ammo tighter), as a field of its ammo profile (§4.14)? Default: no; every round of a caliber shares the weapon's resting spread. Decide when a second round type of one caliber is in play (Phase 3 at the earliest).
41. **Placing objects:** the player places a bag in the world, where it becomes a static object that works as a container (searched like a crate), the first case of a larger object placement and base building idea. Deferred far future, possibly post-V1 (§6). Open: whether a placed bag stays on a map between missions (with Q28) and how it sits beside hub storage (Q30). Cut line when it comes: bags only, placed on the ground where the player stands, no building pieces, no placement grid.
42. **Loot pips and the look-at panel in co-op and in the dark:** does a pip show for an item a teammate can see but the player cannot, and does darkness hide a pip the way it hides the item? Default: no to both for now: only the player's own line of sight counts, and pips ignore light until light matters to sight (Phase 5 day cycle). Decide with the Phase 5 night or with co-op.
43. **Magazine fill in the look-at panel:** the details show what the inventory cell shows, so a magazine on the floor reads its exact X/Y before pick-up. Default: yes, since the panel only opens within reach and the inventory is already exact (§4.2). Alternative: the panel shows the ammo check's bands, and the exact count waits for pick-up. Decide with the feature's first card.

## 10 Engine fit and engine requests

Available from the engine: wgpu rendering (cascaded shadows, SSAO, skybox), a rapier3d-based
collide-and-slide character mover and triangle-mesh colliders, audio, input mapping, the event bus,
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
10. **Switch between levels:** unload one level whole and load another whole, carrying player state across, with a loading screen and inside a load budget; the hub to a mission map and back, and the party together in co-op. A level is a few kilometres across at most and carries an authored boundary that the game can read, so the game removes what leaves it (§5). *(first large map Phase 3 as a start-up load; switching Phase 6)*
11. **Headless simulation:** run the offline world sim without rendering (ENG-F6 is related). *(Phase 7)*
12. **Posture:** crouch and prone hulls with clearance, lean with clearance in every stance, stance changes that take ticks. Crouch is [ENG-F33](https://github.com/WrackedFella/moho/issues/492); lean and prone are filed as [#544](https://github.com/WrackedFella/moho/issues/544). *(Phase 1.2)*
13. **Assemble a level from pieces:** build one level at load from several authored pieces fitted at connection points, with colliders, markers and navigation that work across the joins, the same result from the same seed and content version (co-op), inside a load-time budget. *(later phase: generated maps)*
14. **Draw a round in flight:** a tracer that follows a position the game owns and updates each tick, rather than a fixed-lifetime effect; later, a query that reports where a ray enters and leaves a surface, for penetration (a reverse ray works until then). Belongs with [ENG-F40 #636](https://github.com/WrackedFella/moho/issues/636) (muzzle flash, tracer, impact marks); not yet raised there. *(Phase 1.3; entry and exit Phase 3, optional)*
15. **HUD elements tied to the world** ([#1035](https://github.com/WrackedFella/moho/issues/1035), answered as ENG-F55 [#1047](https://github.com/WrackedFella/moho/issues/1047)): place a HUD element (a soft-edged dot, a panel with text) at a point in the world each frame, fade it, and know when that point is behind the player or off screen; draw translucent panels in shapes other than rectangles (cut corners) with an inset border. *(after Phase 2: loot pips and the look-at panel, FPS-F23)*
