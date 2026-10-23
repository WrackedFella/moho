# Carried weight slows the player and shows as the load icon

**Feature:** FPS-F7
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Every item weighs something, and what the player wears and carries is their load.
Load against carry capacity passes thresholds (fine, slowed, no sprint, cannot move)
that change movement speed through the stat layer (GDD §4.12), shown as a white, yellow
or red HUD icon. The inventory shows the numbers, and a character screen lists each
stat with its contributors. Dropping items is how the player sheds load. Depends on
#596 (FPS-F6-02).

## Deliverables
- Item weight in data; load is worn gear plus everything in it.
- Carry capacity from a fixed base attribute plus what gear adds; gear may add to it.
- Movement speed and carry capacity resolve through the stat layer from two sources,
  equipment and load, with add, scale and cap.
- Load thresholds in data; the load icon: white (fine), yellow (slowed, no sprint),
  red (cannot move).
- Inventory shows carried kg against capacity with the thresholds marked.
- A character screen lists carry capacity, movement speed and stamina, each with its
  contributors.
- Dropping an item from the inventory puts it on the ground; the interact key picks it
  up into the backpack.

## Acceptance criteria
```gherkin
Scenario Outline: Load sets the icon and movement speed
  Given carry capacity is 30 kg and the thresholds are slowed at 60%, no sprint at 80% and cannot move at 100%
  When the player carries <load> kg
  Then the load icon is <icon>
  And the player moves at <speed>

  Examples:
    | load | icon   | speed         |
    | 15   | white  | full speed    |
    | 20   | yellow | reduced speed |
    | 25   | yellow | reduced speed |
    | 30   | red    | no movement   |

Scenario: Dropping an item restores speed
  Given the player is slowed by load
  When the player drops items until the load is below the slowed threshold
  Then the load icon is white
  And the player moves at full speed

Scenario: A dropped item can be picked up again
  Given the player has dropped a magazine
  When the player presses interact on it
  Then the backpack holds the magazine

Scenario: Gear that adds carry capacity raises the thresholds
  Given the player is slowed by load
  When the player wears a backpack that adds 10 kg of carry capacity
  Then the load icon is white

Scenario: The inventory shows load against capacity
  Given the player carries 20 kg against 30 kg of capacity
  When the player opens the inventory
  Then it shows 20 kg of 30 kg with each threshold marked

Scenario: The character screen explains movement speed
  Given the player is slowed by load
  When the player opens the character screen
  Then movement speed lists load as a contributor with its change
```

## Verification
- Overload in the arena by looting everything: the slowdown is felt before the icon is
  checked, and dropping gear to escape a wave is a real choice. Record the capacity,
  weights and thresholds tried on this issue.

## Notes
- The sprint cap at the no-sprint threshold is in FPS-F7-04.
- Attribute names stay TBD (GDD §9 Q14); the screen may show a placeholder name.
