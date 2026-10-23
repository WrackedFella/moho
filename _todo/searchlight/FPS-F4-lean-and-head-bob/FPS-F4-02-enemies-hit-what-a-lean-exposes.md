# Enemies see and hit what a lean exposes

**Feature:** FPS-F4
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The player's hit volumes follow the lean: the head and upper body move with it, the legs stay
planted. A player hidden behind a corner is safe until they lean out, and then enemies see and
hit only what crosses the corner.

## Deliverables
- The head, torso and arms move with the lean; the legs do not.
- Enemy sight and enemy fire use the player's leaned body.
- Behind tall cover, an upright player is unseen by enemies past the corner; leaning out makes
  the player visible to them.

## Acceptance criteria
```gherkin
Scenario Outline: Hit volumes follow the lean
  Given the player is fully leaning <side>
  When a ray is cast at where the <part> now sits
  Then it hits the player's <part>
  Examples:
    | side  | part       |
    | left  | head       |
    | left  | left arm   |
    | right | head       |
    | right | right arm  |

Scenario: The legs stay where the player stands
  Given the player is fully leaning right
  When a ray is cast at the standing position of the legs
  Then it hits the player's legs

Scenario: An upright player behind a corner is not seen
  Given the player stands upright behind a tall wall corner
  And an enemy faces the corner from the far side
  When the enemy looks for the player
  Then the enemy does not see the player

Scenario: Leaning out makes the player visible
  Given the player stands behind a tall wall corner with an enemy on the far side
  When the player fully leans out past the corner
  Then the enemy sees the player

Scenario Outline: Only exposed parts are hit
  Given the player is fully leaning right past a tall wall corner
  When an enemy fires at the player's <part>
  Then the shot <outcome>
  Examples:
    | part     | outcome           |
    | head     | hits the head     |
    | right arm | hits the right arm |
    | legs     | hits the wall     |
    | left arm | hits the wall     |
```

## Verification
1. Arena playtest: hide upright behind a tall cover corner while a wave approaches; take no
   hits. Lean out: enemies react and the damage feedback names head, torso or right arm.

## Notes
Depends on FPS-F4-01 (the lean itself).
