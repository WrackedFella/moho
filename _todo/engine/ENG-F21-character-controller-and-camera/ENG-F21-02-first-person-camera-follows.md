# A first-person camera follows a character smoothly

**Feature:** ENG-F21 (#123)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

The engine builds a first-person view from a character's eye and look direction,
interpolated between the last two ticks so it moves smoothly at any refresh rate. It
only reads simulation state. The strategy game's first-person view moves onto it; its
isometric camera stays. Consumers: the strategy game now, P.2 next.

## Deliverables

- A camera view from a character's eye (game-set eye height) along its look direction.
- The eye interpolates between the previous and current tick by the frame's blend factor.
- Building the camera changes no simulation state.
- The strategy game's first-person view uses it; the isometric camera stays strategy-side.

## Acceptance criteria

```gherkin
Scenario: The camera sits at the character's eye
  Given a character at (0, 0, 0) with eye height 1.6 m
  When the camera is built at the end of a tick
  Then the camera is at (0, 1.6, 0)

Scenario: The camera looks where the character looks
  Given a character facing +Z and looking 30° up
  When the camera is built
  Then the camera looks along +Z, 30° up

Scenario: The camera blends between ticks
  Given a character that moved from (0, 0, 0) to (1, 0, 0) in the last tick
  When the camera is built halfway to the next tick
  Then the camera is at (0.5, eye height, 0)

Scenario: Building the camera changes nothing in the simulation
  Given a character mid-walk
  When the camera is built many times between two ticks
  Then the character's state after the next tick is the same as if it had never been built
```

## Verification

- Strategy game at 144 Hz and 30 Hz: first-person view moves without judder; switching
  to isometric and back still works.
