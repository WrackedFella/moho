# Dropped items fall, tumble and settle in the world

**Feature:** FPS-F7
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Dropped items and fast-reloaded magazines become physics objects: they leave the
player with a little velocity, fall, bounce and come to rest on the arena, and the
player picks one up by looking at it. Their contents stay as they were. Needs engine
request #600 (loose physics objects); until then items rest under the drop point
(#606, FPS-F7-06).

## Deliverables
- A dropped item, or a magazine from a fast reload, spawns as a loose object with a shape
  and mass from its item data.
- Loose objects come to rest on world geometry and on each other.
- Looking at a loose object and pressing interact picks it up, as in #606 and FPS-F7-06.
- The arena caps loose objects (number in data) and removes the oldest past the cap.

## Acceptance criteria
```gherkin
Scenario: A dropped item comes to rest on the floor
  Given the player stands on the arena floor
  When the player drops a magazine
  Then the magazine comes to rest on the floor within a few seconds
  And it is not under the floor

Scenario: A fast-reloaded magazine leaves from the weapon
  Given the player fast-reloads while standing
  When the old magazine leaves the weapon
  Then it falls from the weapon's position and comes to rest near the player's feet

Scenario: A loose object is picked up by looking at it
  Given a magazine has come to rest on the floor
  When the player looks at it within reach and presses interact
  Then the magazine is no longer in the world
  And it is in the player's rig or backpack with its rounds

Scenario: Objects rest on each other
  Given a magazine lies on the floor
  When the player drops a second magazine onto it
  Then the second magazine comes to rest on or beside the first

Scenario: The arena removes the oldest object past the cap
  Given the cap is 50 and 50 loose objects are in the arena
  When the player drops another item
  Then the oldest loose object is gone
  And 50 remain
```

## Verification
- Drop magazines on the floor, on crates and down a slope: they tumble and settle
  believably, move smoothly, and never sink into the floor.
- With 50 loose objects resting, the frame rate is unchanged.

## Notes
- Out: kicking or pushing objects, shooting them, impact sounds (cut lines on #600).
