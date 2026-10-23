# FPS-F7 — The player picks a loadout and pays for its weight

**Status:** Draft
**Labels:** feature, line:fps

## End state

As FPS-F6: one item model for everything the player carries, in containers that are
gear, with capacity and weight as the cost of carrying more (GDD §4.1, §4.12).

## Summary

The second slice of FPS-M2 (GDD Phase 2). The player wears gear in equipment slots,
carries up to three weapons in two calibers, and swaps rig and backpack for ones taken
from bodies, which changes the shape of their inventory. Everything worn and carried
weighs something, and load against carry capacity slows the player through the stat
layer, shown as a white, yellow or red icon. This is where pillar 1 (gear is your
character) first shows: what you wear decides what you can reach and how fast you move.

**Moves toward the end state by:** birthing the stat layer (GDD §4.12) with its first
three stats and two sources, which wounds, stance and survival declare against later.
Closes FPS-M2's exit goal.

## Exit criteria

- The player carries a rifle, a pistol and an SMG; the pistol and SMG share loose
  rounds but not magazines.
- A data file adds a weapon in an existing caliber, and enemies can carry it and the
  player can equip it, with no code change.
- Swapping to a looted rig with more magazine slots lets the player carry more
  magazines on it; swapping to a smaller backpack refuses to fit what no longer fits.
- Picking up enough weight moves the load icon from white to yellow to red, and the
  player slows, loses sprint, then cannot move; dropping items reverses it.
- The inventory screen shows carried weight against carry capacity with the thresholds
  marked, and the character screen lists movement speed with each contributor.
- Justin playtests and judges whether weight makes the player choose what to carry.

## Scope

- In: equipment slots (primary 1, primary 2, sidearm, rig, backpack, body armor and
  helmet empty); a pistol and an SMG in a shared pistol caliber; weapon switching;
  equipping weapons, rigs and backpacks from the inventory or a body; dropping items
  into the world and picking them up; item weight; the stat layer with carry capacity,
  movement speed and stamina; equipment and load as its sources; load thresholds; the
  HUD load icon; weight readout; a character screen listing those stats by source;
  sprint limited by stamina; the rig strip after a reload or switch and on the
  check-gear key; a one-handed property on weapons (pistol), with no effect yet (§9 Q24).
- Out: armor items, attachments, wear (FPS-M3); wounds as a source (FPS-M4); stance as
  a source (FPS-F4); grid view.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Which three weapons | The current rifle in its caliber; a pistol and an SMG sharing a pistol caliber | A shared caliber tests that rounds are keyed by caliber and magazines by weapon, the case the item model must get right |
| Sprint and stamina | Born here (Justin, 2026-10-23): sprint exists, stamina limits it, load caps it | Two of the three Phase 2 stats need them, and "no sprint" is a load threshold. Cost: a new movement mechanic |
| Stat layer verbs | Add, scale and cap from the start, as §4.12 | Retrofitting a verb changes every source's declaration |
| One-handed weapons (§9 Q24) | A weapon property in data, read by nothing until FPS-M4 | Naming it now keeps the pistol's data stable |
| Armor slots | Present and empty | GDD §4.6; slots are cheap, items need wear (FPS-M3) |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Steadiness, recoil control, handling | No source exists until wear, wounds or stance (§4.12) | Their first source |
| Fast reload dropping the magazine; own mag back to the rig | Second reload variant (§9 Q26) | After the FPS-M2 playtest |
| Quick-use slots for consumables on the rig | Nothing usable in a fight exists until meds | FPS-M4 |
| Tool slots (§9 Q23) | First tool item is FPS-M3's toolkit | FPS-M3 |
| A second rifle, a shotgun | Three weapons in two calibers answer the Phase 2 question | Content plan, FPS-M6 |
| Enemies choosing loadouts by grade | Enemy grades slice 1 owns grade behavior | With FPS-F5 (enemy grades) |

## Items

| Item |
|---|
| FPS-F7-01, the player carries and switches between a rifle, a pistol and an SMG (equipment slots, shared pistol rounds) |
| FPS-F7-02, the player wears a looted rig or backpack and the inventory changes shape |
| FPS-F7-03, carried weight slows the player and shows as the load icon (stat layer, weight readout, character screen, dropping items) |
| FPS-F7-04, the player sprints until stamina runs out, and load takes sprint away |
| FPS-F7-05, the rig strip shows after a reload or switch and on the check-gear key |

Titles only until the feature is approved. Depends on FPS-F6-01 and -02.

## Notes

- Open for the Tech Lead:
  - TL: the stat layer is cross-cutting (wounds, stance, survival declare against it). Does it need an ADR, and does stance (FPS-F4) land a source before this does?
  - TL: does the character screen need engine UI controls beyond the inventory screen's?
  - TL: the check-gear key is a held action; confirm ENG-F26 (#359) tap and hold covers it.
