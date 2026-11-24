# Each footstep swings the view out to its own side, tracing a figure-8

**Feature:** FPS-F4 (#602)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The head bob traces a horizontal figure-8: each footstep swings the eye out to that foot's side and back through the centre, so the view reads as alternating steps rather than a sway. Size follows speed but stays restrained. Today's bob dips on every step and sways once per stride, which traces a U, not a figure-8.

## Deliverables
- The figure-8 bob path, replacing today's dip-and-sway path.
- The speed rule below, including its ceiling above walking speed.

## Acceptance criteria

```gherkin
Feature: Footsteps trace a figure-8

  Background:
    Given the head-bob setting is 100 %
    And the player is on the ground

  Scenario: Each footstep swings to its own side
    When the player walks forward for two footsteps
    Then during the first footstep the eye is only ever on one side of its resting point
    And during the second footstep the eye is only ever on the other side

  Scenario: The path crosses the centre between footsteps
    When the player walks forward for one stride
    Then the eye passes through its resting point at the start of each footstep

  Scenario: Each footstep's loop rises and falls
    When the player walks forward for one footstep
    Then the eye is above its resting point for part of the footstep
    And below its resting point for part of the footstep

  Scenario: The figure-8 lies on its side
    When the player walks forward for one stride
    Then the eye's sideways travel is larger than its up-and-down travel

  Scenario Outline: The loop grows with speed, restrained above a walk
    When the player moves at <speed> of walking speed
    Then the loop is <size> the walking loop

    Examples:
      | speed | size                     |
      | 0     | absent: the view is still |
      | half  | smaller than             |
      | 1x    | the same as              |
      | 2x    | no more than 1.5 times   |

  Scenario: The setting still scales it
    Given the head-bob setting is 0 %
    When the player walks forward for one stride
    Then the eye does not move from its resting point
```

## Tech spec
<!-- Tech Lead -->

## Verification
- Walk, then strafe, in the arena at 50 % and 100 %: the view reads as left, right footsteps, "grounded, not seasick".
- Fire while walking: the bob alone does not move where rounds go (aim and hit volumes stay unbobbed, as in #609).

## Notes
- The 2x row has no in-game source until sprint (FPS-F7); it fixes the ceiling now so sprint inherits it.
