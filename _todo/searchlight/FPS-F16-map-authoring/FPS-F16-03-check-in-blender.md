# Check lists the problems the game would find in the open map

**Feature:** FPS-F16
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
A Check button in the Searchlight panel runs the game's own map check on the open scene, without opening the game window, and lists each problem with the object it names. The rules live once, in the game.

## Deliverables
- The game checks a map and reports every problem it finds, without opening a window.
- A Check button that lists those problems in Blender; picking one selects its object.

## Acceptance criteria
```gherkin
Scenario: A sound map has no problems
  Given a scene the game loads and plays
  When the designer presses Check
  Then Blender reports no problems
  And no game window opens

Scenario Outline: Check reports what the game would refuse
  Given a scene with <fault>
  When the designer presses Check
  Then Blender lists a problem saying <problem>

  Examples:
    | fault                                         | problem                                        |
    | no player spawn                               | the player spawn is missing                    |
    | two player spawns                             | there is more than one player spawn            |
    | an enemy spawn and no waypoint                | enemies have no waypoint to patrol             |
    | two waypoints with the same number            | waypoint numbers repeat                        |
    | waypoints no enemy spawn can walk to          | no enemy spawn can reach a waypoint            |

Scenario: Every problem is listed, not only the first
  Given a scene with no player spawn and two waypoints with the same number
  When the designer presses Check
  Then Blender lists both problems

Scenario: A problem leads to its object
  Given Check lists a problem naming a waypoint
  When the designer picks that problem
  Then that waypoint is selected in Blender

Scenario: Check and launch agree
  Given a scene Check reports a problem for
  When the game launches on its export
  Then the game refuses it with the same message
```

## Tech spec

## Verification
- On a map with a waypoint walled off from every enemy spawn, Check lists the reach problem; removing the wall clears it.

## Notes
- The problem wording is the game's; the table names the meaning, not the exact text.
