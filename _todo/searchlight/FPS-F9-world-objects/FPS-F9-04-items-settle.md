<!-- board: gate=unset; eligible=no; priority=P3 -->
# Dropped items fall, tumble and settle before they are picked up

**Feature:** FPS-F9 (#616)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Dropped items and fast-reloaded magazines become physics objects: they leave the player
with a little velocity, fall, bounce and come to rest on the level and on each other, and
are picked up as in FPS-F9-03 once they lie still or while they move. Contents stay as
they were. Formerly FPS-F7-07. Depends on engine #600 and FPS-F9-03.

## Deliverables
- A dropped item spawns as a loose object with a shape and mass from its item data.
- Loose objects come to rest on level geometry and on each other.
- The level caps loose objects (number in data) and removes the oldest past the cap.
- Loose objects are host-simulated; a replay puts them where they landed.

## Acceptance criteria
```gherkin
Scenario: A dropped item comes to rest on the floor
  Given the player stands on the arena floor
  When the player drops a magazine
  Then within 5 seconds the magazine is at rest on the floor
  And it is not under the floor

Scenario: A dropped item rolls off a slope
  Given the player stands on a slope
  When the player drops a magazine
  Then it comes to rest lower down the slope than where it left the player

Scenario: Objects rest on each other
  Given a magazine lies on the floor
  When the player drops a second magazine onto it
  Then the second magazine comes to rest on or beside the first

Scenario: A settled object is picked up with its contents
  Given a magazine holding 12 rounds has come to rest
  When the player looks at it within reach and presses use
  Then the backpack holds it with 12 rounds

Scenario: The level removes the oldest object past the cap
  Given the cap is 50 and 50 loose objects are in the arena
  When the player drops another item
  Then the oldest loose object is gone
  And 50 remain

Scenario: A replay lands objects in the same place
  Given a recorded run in which the player dropped three magazines
  When the run is replayed
  Then each magazine comes to rest where it did in the recording
```

## Verification
- Drop magazines on the floor, on crates and down a slope: they tumble and settle
  believably, move smoothly, and never sink into the floor.
- With 50 loose objects resting, the frame rate is unchanged.

## Notes
- The fast reload's magazine (FPS-F7-06) is a dropped item and needs nothing extra here.
- Out: kicking, pushing or shooting objects, impact sounds (#600 cut lines).
