# The player fights from a rig loaded from data

**Feature:** FPS-F6
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The rifle, its magazines, loose rounds and the rig become items defined in data, and a
starting kit from data equips the player. Reload draws only from the rig's magazine
slots. This is the item model's first path through play (#590).

## Deliverables
- Items load from data with a class (weapon, consumable, tool), a size, and a condition
  or a number of uses, never both.
- Magazines hold an ordered stack of typed rounds; the rifle's data names its magazine.
- A rig item has a number of magazine slots; the player wears one from the starting kit.
- Reload takes the next magazine on the rig and puts the ejected one in the slot it frees.
- A tool item defined in data loads and can be listed in a kit.

## Acceptance criteria
```gherkin
Scenario: The run starts with the kit from data
  Given a starting kit of a rig with 3 magazine slots, a loaded rifle and 2 full magazines
  When a run starts
  Then the rifle is loaded and chambered
  And the rig holds 2 full magazines and 1 free slot

Scenario: Reload takes the next magazine from the rig
  Given the rig holds a full magazine
  And the rifle's magazine is empty
  When the player reloads
  Then the rifle holds the full magazine
  And the empty magazine is on the rig

Scenario: Reload with no magazine on the rig does nothing
  Given the rig holds no magazine
  When the player reloads
  Then no reload starts

Scenario: A kit that overfills the rig is refused
  Given a starting kit with more magazines than its rig has slots
  When the game loads it
  Then loading fails and names the kit

Scenario: An item with both a condition and uses is refused
  Given an item definition with a condition and a number of uses
  When the game loads it
  Then loading fails and names the file and the item id

Scenario: A mod adds a magazine with no code change
  Given a mod that defines a 20-round magazine for the rifle and puts it in the starting kit
  When a run starts with the mod enabled
  Then the rig holds a magazine that empties after 20 shots

Scenario: A tool item loads
  Given a tool item definition in data
  When the game loads it
  Then the item is available to kits and loot under its id
```

## Verification
- Start the arena, fire and reload through the starting magazines: reload feels as it
  did before, and runs dry when the rig is empty.

## Notes
- Damage moves from the rifle to the round in FPS-F6-03; until then a magazine's rounds
  are one type.
