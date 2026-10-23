# A fast reload drops the old magazine, and picking it up returns it to the rig

**Feature:** FPS-F7
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The reload gets its two variants (GDD §4.2): a fast reload drops the old magazine to the
ground with its rounds, and the retention reload (the only one today) stows it and takes
longer. Picking up your own dropped magazine puts it in a free rig slot (GDD §4.1).
A dropped magazine is a world item: it rests under the player and is picked up with the
use key (#626, FPS-F9-03), and falls and settles once #628 (FPS-F9-04) lands. Depends
on #626.

## Deliverables
- Two reload inputs: retention (as today) and fast. Default binding: tap reload for
  retention, double-tap for fast, rebindable with ENG-F26 (#359).
- Fast reload takes the weapon's reload time; retention takes that time plus a retention
  time in weapon data (GDD §9 Q26: variants add their time).
- A fast-dropped magazine keeps its rounds and lies in the world as an item.
- Picking up a magazine the player dropped puts it in a free rig slot, else the backpack.

## Acceptance criteria
```gherkin
Scenario: A fast reload drops the old magazine
  Given the rifle holds a half-full magazine and the rig holds a full one
  When the player fast-reloads
  Then the rifle holds the full magazine
  And the half-full magazine lies on the ground with its rounds

Scenario: A fast reload is quicker than a retention reload
  Given a weapon with a reload time and a retention time
  When the player fast-reloads, and later retention-reloads
  Then the fast reload finishes after the reload time
  And the retention reload finishes after the reload time plus the retention time

Scenario: A retention reload keeps the magazine
  Given the rifle holds a half-full magazine and the rig holds a full one
  When the player retention-reloads
  Then the half-full magazine is on the rig

Scenario: The player's own dropped magazine returns to the rig
  Given the player fast-dropped a magazine and the rig has a free slot
  When the player picks the magazine up
  Then the rig holds it with the rounds it had

Scenario: With the rig full, the picked-up magazine goes to the backpack
  Given the player fast-dropped a magazine and the rig has no free slot
  When the player picks the magazine up
  Then the backpack holds it

Scenario: An empty-chamber fast reload still pays the charge
  Given the rifle's chamber is empty
  When the player fast-reloads
  Then the reload finishes after the reload time plus the charge time
```

## Verification
- Fight a wave using both reloads: the fast reload is the panic button and the dropped
  magazines are worth going back for. Record the retention time tried on this issue.

## Notes
- The empty-chamber scenario assumes #567 (FPS-F2-08) has landed.
- TL: does fast reload read the handling stat, or a fixed time first (handling has no
  source in FPS-M2)?
