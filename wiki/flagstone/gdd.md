# Flagstone: game design document (v0.1)

**Status:** draft v0.1. Order of work is in the [roadmap](roadmap.md). Open questions are at the [end](#open-questions).

**Working pitch:** a colony base-builder on editable voxel terrain. You direct a small group of pawns to mine and
build, the colony grows by automating what pawns do by hand, and at any moment you can drop into one pawn's eyes and
do the work yourself.

This page describes what the player experiences. It names no crates or types; the engineering view is in
[architecture/overview](../architecture/overview.md) and the [ADRs](../../_todo/adr/README.md).

---

## 1 Pillars

Each pillar is a filter: a feature that serves none of them is cut.

1. **Orders, not micromanagement.** The player marks *what* should happen; pawns decide *who* and *how*. Direct
   control is the exception, entered on purpose.
2. **Strategy mode is the game.** First person is a way to possess one pawn, not a second game.
3. **The world is yours to reshape.** Terrain is editable and what you change stays changed. Trust in the world
   (no falling through, no tears, no lost progress) comes before more features.
4. **A loop that closes.** Mine, build, automate, progress, in the spirit of Satisfactory or Factorio with
   RimWorld or Dwarf Fortress colony control.
5. **The setting can change.** Mining, building, units, automation, progression and combat never depend on it.

## 2 Modes

| | Strategy mode (default) | Possession mode (first person) |
|---|---|---|
| Camera | Top-down or isometric: pan, zoom, rotate | First person from the possessed pawn |
| Player does | Select, mark work, place buildings, set priorities | Walk, mine, build and carry as that pawn |
| Pawn under control | None; pawns follow their own work | One; its own work is paused |
| On leaving | n/a | Control returns to strategy mode; the pawn resumes its work. Re-entering without a selection possesses the last possessed pawn |
| HUD | Colony: resources, selection, build menu, time controls | Pawn: hotbar, held tool, pickup feedback |

Today the only pawn is the player pawn, which sits inactive while the player is in strategy mode. Later every pawn
acts on its own when not possessed. A possessed pawn follows the same rules as any other: there is no player-only
game rule.

## 3 Core loop

```mermaid
flowchart LR
    M["Mark ground to dig"] --> D["Pawns dig"] --> Y["Resources join colony totals"]
    Y --> B["Place buildings"] --> C["Pawns construct"] --> A["Automate and progress"]
    A --> M
```

Moment to moment: look at the colony from above, mark work, watch it get done, spend what came in. Step into a pawn
when doing it by hand is faster or more fun.

**Prototype goal.** A 20-minute session in which a new player starts a world, marks ground to dig, watches pawns dig
and bank the yield, places a building that pawns construct, and takes over a pawn to do a task by hand, without a
crash, a fall through the world, or lost progress.

## 4 Strategy mode

### 4.1 Colony view and selection
- Pan, zoom and rotate the camera over the world; it never ends up inside terrain.
- Select a pawn, a building or a block by clicking; box-select several pawns. Selected things are visibly marked and
  described in a selection panel.
- **Underground view:** a height slice hides everything above a chosen level, as in Dwarf Fortress. X-ray is a later
  nice-to-have.
- Pause and 1×/2×/3× speed. Orders can be given while paused.

### 4.2 Orders and work
- Mark blocks or a rectangular volume to dig; marks show on the terrain and can be cancelled.
- Idle pawns pick up marked work on their own, one pawn per task. A pawn that cannot reach a mark yet waits and tries
  again as the terrain changes.
- A pawn walks to the work, digs it and the yield joins the colony totals shown in the HUD.
- Marks are shared by all pawns, survive save and load, and can be set through a height cut.
- A direct move order overrides a pawn's own work (should).
- Pawns differ by move speed and carry capacity only (should). Per-pawn job priorities are deferred.

### 4.3 Buildings
- A build menu lists what is available; choosing one shows a placement preview that is valid or invalid.
- Placing a building creates a construction site; pawns deliver the cost and build it.
- A finished building is a persistent object on the grid with a distinct shape. Buildings are objects, not
  terrain, so terrain stays voxel-only.
- First buildings are generic (storage, workbench, wall, floor) with no setting flavour.
- Buildings can be deconstructed for a partial refund (should). The build menu is a general action menu that
  follows the selection, not a buildings-only list (should).

## 5 Possession mode

- A key toggles possession; the pawn the player leaves keeps working.
- While possessed: walk, jump, mine the aimed block with an equipped tool, see yields in the hotbar.
- Mining requires an equipped tool; yields stack in the pawn's inventory.
- A "+N resource" popup confirms each pickup.
- The player can choose which pawn to possess from the colony list (should).
- Placing or building what strategy mode can mark is a later option.

## 6 World and persistence

- A new world gives the colony a flat, safe start area; no pawn ever spawns or loads inside or under terrain.
- Terrain reads as intentional: biome edges, cliffs, caves and ore are placed deliberately, and ore is findable.
  Known problems: smoothing, caves and level-of-detail transitions need work.
- Mining never leaves visible holes in neighbouring terrain.
- What was mined and built survives quitting, killing the process, and reloading. The game autosaves every two
  minutes (configurable) and on exit, so a crash costs at most a few minutes.
- **Light follows what is dug and built:** shafts and caves are dark under a roof and lit under open sky, and the
  first light-emitting block is a glowing crystal.
- **Mining granularity will shrink.** Today a block breaks whole. Later a block is a grid of sub-blocks that wear
  away gradually, so nothing outside the destruction step may assume whole-block mining.

## 7 Non-goals for the prototype

- Needs, moods, health, combat and enemies. The loop must be fun with only work and building.
- Ground items as world entities. Yields live in the pawn's inventory and in colony totals.
- Multiplayer (the simulation stays network-ready, nothing more).
- Any setting, story or factions.

## 8 Deferred ideas

Intent only, not decomposed. Each waits for a named consumer. Setting and system ideas are candidates, not decisions.

**Next after the prototype**
- Automation: machines and logistics doing what hand-mining did.
- Progression: unlocks gating buildings and tools, setting-neutral.
- Stockpile zones and hauling as a real activity (the prototype uses colony totals).
- Combat, hostile pressure, needs and moods.

**Setting candidates** (the setting is deliberately undecided)
An overseer sent by a megacorporation to mine an alien world for a prized resource, one planet per level,
  with the corporation corrupt and hiding what it knows about the place. Fits the "alien corporation" candidate in the
  vision.
Planets as levels, each gated by a prohibition the player works around with upgrades (no ice planet until
  units have internal heating). The vision's "worlds reached through portals" is the same idea in a different
  wrapper.
Planet types: desert, jungle, ice, asteroid, volcanic, storm, ocean. Resources: copper, iron, coal,
  lead, crystals and gas for fuel, a rare objective ore.
Indigenous life, ancient constructs and a fungus that grows over bases as passive pressure.

**Systems**
Dual-purpose units, where mining gear is repurposed for fights and combat abilities help mining (a
  "harden" mode that also protects against cave-ins). Combat should feel improvised.
Cave-ins that block paths, slow units and hurt those nearby.
Supply costs for ammunition and repairs, so a turret-ringed base is expensive to keep running.
Unit customisation from parts, then perks from a skill tree. First-person control buffs the possessed
  unit.
Patrol routes for mining and transport vehicles that carry materials along player-built paths.
Score by resources gathered against resources available, and timed hold-out levels.
A voiced guide or mission AI, tutorial levels set in a mining academy.
- Third-person pawn model, richer movement, other worlds.

**Audio direction:** ambient, loopable and low, reflecting the environment (insectoid life gets short
erratic music, machines get electronic), with voiced tutorials and tooltips as an aim.

## Open questions

For Justin; the defaults above hold if left unanswered.

- **Q1.** World generation: what are the top three things that look wrong? Needed before SG-F6 can have exit criteria.
- **Q2.** Playtest bugs beyond spawn, mesh gaps and persistence? They shape the polish round.
- **Q3.** Hauling and stockpile zones: the cards settled on colony totals. Do you still want real hauling after the
  prototype, or is the instant bank the design?
- **Q4.** Which deferred ideas (section 8) are worth promoting to the vision, and which are dropped?
- **Q5.** When the setting is picked, does it become a vision decision or a per-world choice?

Questions for the Tech Lead are on the issues, not here.
