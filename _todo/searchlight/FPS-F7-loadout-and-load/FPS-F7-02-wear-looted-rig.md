# The player wears a looted rig or backpack and the inventory changes shape

**Feature:** FPS-F7
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Rigs and backpacks come in more than one make, enemies wear them, and the player can
swap their own for one taken from a body. A rig or backpack keeps its contents when it
changes hands, so a swap trades containers; the player then moves what they want.
Depends on #596 (FPS-F6-02); independent of FPS-F7-01.

## Deliverables
- At least two rigs (different magazine slot counts) and two backpacks (different cell
  counts) in data.
- Enemy loadouts can wear any of them.
- Wearing a rig or backpack from a body swaps it with the worn one, each keeping its
  contents.

## Acceptance criteria
```gherkin
Scenario: A bigger rig holds more magazines
  Given the player wears a 3-slot rig and a dead enemy wears a 5-slot rig
  When the player wears the enemy's rig
  Then the player's rig has 5 magazine slots
  And the body now holds the 3-slot rig

Scenario: A swapped rig keeps its contents
  Given the player's rig holds 2 magazines and the body's rig holds 1
  When the player wears the body's rig
  Then the player's rig holds the body's 1 magazine
  And the 3-slot rig on the body still holds the player's 2 magazines

Scenario: A smaller backpack refuses what no longer fits
  Given the player has swapped to a 2-cell backpack that holds 2 one-cell items
  When the player moves another item into it
  Then the item is not moved
  And the screen says the backpack is full

Scenario: The new rig reloads at once
  Given the player has swapped to a rig holding a full magazine for the held weapon
  When the player reloads
  Then the rifle takes that magazine
```

## Verification
- Loot a rig and a backpack mid-run and swap to them: the trade is quick to read on the
  screen and the change in what the rig holds is obvious.

## Notes
- Worn gear's weight arrives with FPS-F7-03.
