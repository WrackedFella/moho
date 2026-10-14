# A character that leaves the ground keeps its momentum

**Feature:** ENG-F21
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary
A character that jumps or walks off a slope or ledge keeps moving in the direction of its momentum; today it appears to move tangent to the slope it left. Reproduce first. Engine request #302; consumers: possessed pawns (SG-F1 #311, SG-F5 #309) and FPS movement (FPS-F2 #297).

## Deliverables
- A reproduction of the defect as a failing test, or the card closes as not reproducible with the tests kept as guards.
- The fix in shared character movement; no game line works around it.

## Acceptance criteria
Velocity at take-off is the velocity of the last grounded tick, plus the jump impulse when jumping ([ADR-0014](../../adr/0014-characters-move-by-velocity.md): momentum survives along slopes).

```gherkin
Scenario Outline: Leaving the ground keeps the direction and speed of travel
  Given a character moving at a steady speed <how>
  When it leaves the ground
  Then on its first airborne tick its velocity keeps the heading and speed of its last grounded tick
  And afterwards only gravity and air control change it
  Examples:
    | how                                   |
    | on flat ground and jumping            |
    | up a slope and jumping                |
    | across a slope and walking off its edge |
    | down a slope and walking off its edge |
    | on flat ground and walking off a ledge |

Scenario: Running up a ramp and off its top launches the character
  Given a character running up a ramp
  When it passes the ramp's top edge
  Then it keeps rising briefly before gravity brings it down
```

## Verification
1. Possess a pawn, run across a hillside and off its edge: the pawn continues in the direction it was running.
2. Run up a ramp or steep hill and off its crest: the pawn carries over the crest rather than dropping straight down.
