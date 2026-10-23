# The player searches a dead enemy and takes what it carried

**Feature:** FPS-F6
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Enemies carry items from data. A dead enemy is a container: searching it takes time and
opens an inventory screen with the rig, the backpack and the body side by side, while
the world keeps running. The player also wears a backpack, reached only through this
screen. Depends on FPS-F6-01.

## Deliverables
- Enemy loadouts from data; a dead enemy keeps what it carried until the run restarts.
- A backpack item with a cell count, worn from the starting kit.
- An inventory key opens the player's own rig and backpack; the interact key on a dead
  enemy in reach searches it, then opens the same screen with the body added.
- Items move between body, backpack and rig; taking from a body puts the item in the
  backpack.
- Arena tuning: the starting kit and enemy loadouts leave the player short of rounds.

## Acceptance criteria
```gherkin
Scenario: Searching a dead enemy shows what it carried
  Given an enemy that carried 2 full magazines has died within reach
  When the player searches it and the search time passes
  Then the screen lists the body's 2 magazines beside the rig and the backpack

Scenario: A living enemy cannot be searched
  Given a living enemy within reach
  When the player presses interact on it
  Then no search starts

Scenario: Taken loot goes to the backpack
  Given the screen shows a body holding a magazine
  When the player takes the magazine
  Then the backpack holds the magazine
  And the body no longer does

Scenario: A full backpack refuses loot
  Given the backpack has no free cells for a magazine
  When the player takes a magazine from a body
  Then the magazine stays on the body
  And the screen says the backpack is full

Scenario: The player moves a magazine onto the rig
  Given the backpack holds a full magazine and the rig has a free slot
  When the player moves the magazine to the rig
  Then the rig holds it and the backpack does not

Scenario: Reload never draws from the backpack
  Given the rig holds no magazine and the backpack holds a full one
  When the player reloads
  Then no reload starts

Scenario: The world keeps running while the screen is open
  Given an enemy is shooting at the player
  When the player opens the inventory
  Then the enemy's hits still damage the player

Scenario: Moving away cancels a search
  Given the player is searching a dead enemy
  When the player moves out of reach before the search time passes
  Then the body's contents are not shown

Scenario: A restart clears bodies and restores the kit
  Given the player has looted bodies during a run
  When the run restarts
  Then no bodies remain and the player holds the starting kit
```

## Verification
- Play three runs without looting and three runs looting between and during waves:
  the runs without looting run dry earlier. Record the kit and loadout numbers tried
  on this issue.
- Searching mid-fight feels risky but not punishing; the screen is readable at a glance.

## Notes
- Moving items is instant in this card; per-move time waits for the playtest.
