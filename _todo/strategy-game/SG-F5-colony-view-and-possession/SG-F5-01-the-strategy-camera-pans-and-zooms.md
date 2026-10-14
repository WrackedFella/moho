# The strategy camera pans and zooms over the world

**Feature:** [SG-F5](https://github.com/WrackedFella/moho/issues/309)
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

In strategy mode the player moves the view over the world, independent of any pawn, and zooms in and out without the camera entering terrain.

## Deliverables

- Pan with the keyboard and the screen edge; zoom with the wheel.
- The camera never ends up inside terrain.

## Acceptance criteria

```gherkin
Scenario: The strategy camera pans over the world
  Given the player is in strategy mode
  When the player pans with the keyboard
  Then the view moves over the world
  And the pawn does not move

Scenario: Panning with the screen edge
  Given the player is in strategy mode
  When the player holds the pointer at a screen edge
  Then the view moves toward that edge

Scenario: The strategy camera cannot enter terrain
  Given the player is in strategy mode
  When the player zooms in over a hillside
  Then the camera stays above the terrain surface
```

## Verification

Manual: pan and zoom feel across flat and steep ground; no jump on entering strategy mode.

## Notes

- A strategy camera and a mode toggle exist today; whatever of the above already holds becomes a passing test.
