<!-- board: gate=unset; eligible=no; priority=P2 -->
# The player drops an item, it lies where it fell, and looking at it picks it up

**Feature:** FPS-F9 (#616)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Dropping an item from the inventory screen puts it in the world on the floor beneath the
player, keeping its contents; looking at it within reach and pressing use picks it up into
the backpack. Static placement is the workaround until FPS-F9-04 (engine #600). Moves the
drop and pick-up out of #606 (FPS-F7-03). Depends on FPS-F9-01 and #596 (FPS-F6-02,
inventory screen).

## Deliverables
- A drop action on items in the inventory screen.
- A dropped item lies on the floor below the drop point, drawn with its own shape.
- The prompt names the item: "Pick up <item>".
- Pick-up puts the item in the backpack with its contents.
- Two uses of one item on one tick: the lower player slot gets it.

## Acceptance criteria
```gherkin
Scenario: A dropped item lies on the floor
  Given the player stands on the arena floor holding a half-full magazine in the backpack
  When the player drops the magazine
  Then the backpack no longer holds it
  And it lies on the floor beneath the player

Scenario: An item dropped at a ledge lies on what is below
  Given the player stands at the edge of a crate
  When the player drops a magazine
  Then it lies on the top of the crate or the floor below, not in the air

Scenario: Looking at a dropped item shows its prompt
  Given a magazine lies on the floor within reach
  When the player looks at it
  Then the prompt names the use key and "Pick up rifle magazine"

Scenario: Picking up returns the item with its contents
  Given a magazine holding 12 rounds lies on the floor
  When the player looks at it within reach and presses use
  Then it is no longer in the world
  And the backpack holds it with 12 rounds

Scenario: A full backpack refuses the pick-up
  Given the backpack has no free cells for a magazine
  When the player uses a magazine on the floor
  Then the magazine stays on the floor
  And the player is told the backpack is full

Scenario: A restart clears dropped items
  Given items lie in the arena
  When the run restarts
  Then no dropped items remain
```

## Verification
- Drop magazines around the arena, walk away and back: they are where you left them and
  easy to pick out.

## Notes
- Fast-dropped magazines returning to the rig are FPS-F7-06's rule, on this pick-up.
