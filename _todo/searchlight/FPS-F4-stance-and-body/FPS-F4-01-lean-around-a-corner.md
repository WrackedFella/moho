# The player leans around a corner to look and shoot

**Feature:** FPS-F4
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Holding Q or E leans the player left or right from standing: the view shifts sideways and rolls
so the player can look and shoot past a corner without stepping out. A wall beside the player
stops the lean short.

## Deliverables
- Holding Q leans left, holding E leans right; releasing returns upright.
- The lean blends in and out over a short time rather than snapping.
- A wall on the lean side stops the lean short; the view never passes through geometry.
- While a lean is held the player moves slower than walking.
- Shots fired while leaning leave from the leaned eye.

## Acceptance criteria
```gherkin
Scenario Outline: Holding a lean key shifts and rolls the view to that side
  Given the player stands in the open
  When the player holds <key> until the lean settles
  Then the eye is about 0.35 m to the <side> of where it stood
  And the view is rolled toward the <side>
  Examples:
    | key | side  |
    | Q   | left  |
    | E   | right |

Scenario: Releasing the lean key returns upright
  Given the player is fully leaning left
  When the player releases Q
  Then the eye returns to its standing position and the view is level

Scenario: The lean takes time
  Given the player stands in the open
  When the player presses Q
  Then on the next tick the eye has moved left but less than the full lean

Scenario: A wall stops the lean short
  Given a wall 0.15 m to the player's left
  When the player holds Q until the lean settles
  Then the eye stays on the player's side of the wall
  And the lean is less than a full lean

Scenario: The most recently pressed side wins
  Given the player holds Q
  When the player also presses E
  Then the player leans right
  And releasing E while Q is still held leans left again

Scenario: Moving while leaning is slower than walking
  Given the player holds Q
  When the player moves forward
  Then the player moves slower than walking upright

Scenario: A shot fired while leaning leaves from the leaned eye
  Given the player is fully leaning right at a corner with a target visible only past it
  When the player fires
  Then the shot hits the target
```

## Verification
1. In the arena, stand beside a tall cover corner and hold E: the view peeks past the corner
   and rolls; release and it returns smoothly.
2. Hold Q against a wall: the lean stops short with no view through the wall.
3. Lean while walking: movement is slower than walking upright.

## Notes
Lean is part of the per-tick command (feature decision). Sprint arrives with FPS-F7 and is refused while a lean is held. Hold-or-toggle and rebinding wait on
ENG-F26 #359.
