# The arena's equipment table hands out magazines and rounds

**Feature:** FPS-F6
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
A table in the arena carries spawners: using one puts one magazine, or a stack of 10
rounds, into the backpack. It is arena scaffolding for testing loading and mixing on
demand, outside the scarcity loop. Depends on FPS-F6-02; the rounds spawner needs
FPS-F6-03.

## Deliverables
- An equipment table in the arena with one spawner per listed item.
- What each spawner gives, and how many, is data.
- Using a spawner with the interact key puts the items in the backpack.

## Acceptance criteria
```gherkin
Scenario Outline: A spawner puts its items in the backpack
  Given a spawner for <item> that gives <count>
  When the player uses it
  Then the backpack holds <count> more <item>

  Examples:
    | item                 | count |
    | rifle magazine       | 1     |
    | ball rounds          | 10    |
    | armor-piercing rounds| 10    |

Scenario: A spawner refuses when the backpack is full
  Given the backpack has no free cells for a magazine
  When the player uses the magazine spawner
  Then the backpack is unchanged
  And the player is told the backpack is full

Scenario: Spawners have no limit
  Given the player has used the magazine spawner 3 times
  When the player uses it again with a free cell
  Then the backpack holds one more magazine
```

## Verification
- Walk to the table between waves, take mags and both round types, and load a mixed
  magazine: the table is easy to find and doesn't get in the way of the fight.

## Notes
- Out: a "next wave" control (deferred in #590).
