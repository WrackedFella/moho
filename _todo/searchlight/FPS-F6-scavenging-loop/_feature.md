# FPS-F6 — The player loots fallen enemies for the rounds that keep them fighting

**Status:** Draft
**Labels:** feature, line:fps

## End state

Searchlight's items are data: weapons, magazines, rounds, meds, food, kits and tools are
one model that mods extend, carried in containers that are themselves gear (GDD §4.1).
Managing what you carry is the second pillar: the player is always short of something
and always scavenging for it.

## Summary

The first slice of FPS-M2 (GDD Phase 2, items and loadout): the arena's rifle,
magazines and rounds become items defined in data, carried on a rig and in a backpack
that are items too. The player starts each run short of ammunition, so keeping up with
the waves means searching bodies mid-fight and loading magazines from loose rounds,
with the world still running. That turns the arena into the scavenging loop in
miniature, end to end through data, sim, UI and loot, before FPS-F7 widens it.

**Moves toward the end state by:** proving one item model carries a weapon, magazines,
rounds and a tool through a list view over grid-shaped data. FPS-F7 (loadout and load)
adds weapons, calibers and carry weight on top; FPS-M3 adds wear and the first
#-use kits.

## Exit criteria

- A data file adds a new magazine or round type, and the player can loot, load and fire
  it with no code change.
- The player can run dry in the arena and recover by looting: a run where the player
  never loots ends with an empty rifle before a run that loots does.
- Reload draws only from the rig; the backpack is reached only through the inventory
  screen, while enemies keep acting.
- A magazine loaded from two round types fires them in the order loaded.
- An item declaring both a condition and a number of uses is refused at load, naming
  the file and the id (GDD §4.1 lifecycle rule).
- Justin playtests and judges whether scavenging under fire is tense rather than tedious.

## Scope

- In: items in data with a class (weapon, consumable, tool), a size, and condition or
  uses (never both); the rifle, its magazine and loose rounds as items; two round types
  for the one caliber; magazines as an ordered stack of rounds; a rig item with a few
  magazine slots; a backpack item with a cell count and weight limit; a starting kit
  from data; enemies carrying items from data; searching a body (takes time); a list
  inventory screen showing rig, backpack and an opened body side by side; moving items
  between them; loading a magazine from loose rounds (takes time per round); one tool
  item defined in data that can be carried (no use); an equipment table in the arena
  whose spawners hand the player one magazine or a handful of rounds per use.
- Out: more weapons and calibers, equipment slots, carry weight's effect on movement,
  the rig strip on the HUD (all FPS-F7); grid view; anything with wear.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Split of FPS-M2 | Two features: this one is a thin end-to-end loop (data → rig → screen → loot); FPS-F7 adds breadth (weapons, calibers, slots, load) | The roadmap's split (model and loot, then UI) leaves the first feature with nothing to loot through; the screen is where the integration risk is. Tech Lead may still resize |
| Item size and container cells | In the data from the first file; the list enforces cells as a count, no placement | Retrofitting size onto every item later is the expensive part of a grid (GDD §4.1, §9 Q17) |
| Where loot comes from | Dead enemies only, as containers searched through a screen | Crates and world piles need loot points; the arena has none |
| What the arena tests | Starting kit is short of rounds; waves outpace it unless the player loots | Without scarcity the item model has nothing to prove in play (pillar 2) |
| Rounds | Loose rounds are a stack consumable keyed by caliber and type; a stack has a count, not uses | Uses (#) are for kits and meds; a stack splits and merges, uses do not |
| Equipment table | Arena scaffolding: one magazine per use, rounds in a stack of 10 per use, amounts in data. Scarcity exit criterion is judged without it | Testing loading and mixing needs supply on demand; it sits in the mode layer (§3.2) so zones never inherit it |
| Retention on reload | Ejected magazine goes back to the rig slot it came from | Fast reload (drop to the ground) waits; one reload exists today (§9 Q26) |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Grid view with drag and rotation | List first, then to taste (§9 Q17) | After the FPS-M2 playtest |
| Fast reload dropping the magazine; own mag returning to the rig on pick-up | Needs world pickups and a second reload variant (§9 Q26) | FPS-F7 or later, to taste |
| Dropping items into the world | Only ammunition exists, so nothing is junk yet | FPS-F7, with weight |
| Splitting stacks by hand, sorting, filtering | List works without them at this item count | Item count grows (FPS-M3) |
| Crates and loot points; looting time by container type | Arena has no loot markers | FPS-M6 zones |
| Item inspection, icons, 3D preview | Text list is enough to test the loop | After playtest |
| Bodies despawning | Arena runs are short; bodies stay until restart | Dead enemies' sim cost becomes a problem |
| Stash | Cut until the ledger (§4.1) | FPS-M6 |
| A "next wave" control so the player chooses when the next wave spawns | Arena scaffolding, not an item behavior | Next arena or mode-layer card, or with FPS-F5 (enemy grades) |

## Items

| Item |
|---|
| FPS-F6-01, the player fights from a rig loaded from data (rifle, magazines, rounds and rig as items; reload draws from the rig) |
| FPS-F6-02, the player searches a dead enemy and takes what it carried (body as container, inventory screen with rig, backpack and body) |
| FPS-F6-03, the player loads magazines from loose rounds, mixed types fire in order |
| FPS-F6-04, the arena starts the player short of rounds (starting kit and enemy loot tuned in data) |
| FPS-F6-05, the arena's equipment table hands out magazines and rounds (spawners as items or markers, amounts in data) |

Titles only until the feature is approved.

## Notes

- Open for the Tech Lead:
  - TL: do the inventory and loot screens need generic list or two-panel controls from the engine UI (ENG-F18), or are they FPS-owned? File an engine request if generic.
  - TL: the magazine today counts rounds; it becomes an ordered stack of typed rounds. Size the migration of FPS-F2's rules and the empty-chamber reload card #567.
  - TL: the world keeps running while the screen is open. What input does the player keep (mouse look, movement) while it is up?
  - TL: does the tool class need anything beyond a class value and a definition that loads?
