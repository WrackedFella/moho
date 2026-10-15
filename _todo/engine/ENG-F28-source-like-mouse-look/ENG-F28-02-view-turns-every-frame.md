# The view turns every rendered frame

**Feature:** ENG-F28
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine, M2

## Summary
The first-person camera turns on every rendered frame, not only when a 60 Hz tick
runs. Look still reaches the simulation as a tick command (ADR-0011), and when it
arrives the simulation's look matches what the camera already showed.

## Deliverables
- Mouse and right-stick look show on the next rendered frame, in the FPS-prototype
  walk and the strategy first-person free-cam.
- The pitch limit holds on screen as well as in the simulation.

## Acceptance criteria
```gherkin
Scenario: Look shows before the tick that consumes it
  Given the game renders frames between ticks
  When the mouse moves and a frame renders before the next tick
  Then that frame's view is turned by the movement

Scenario Outline: The simulation lands where the camera already is
  Given mouse smoothing is <smoothing>
  When the mouse moves over several frames and then a tick runs
  Then the character's yaw and pitch after the tick equal the yaw and pitch the last frame showed

  Examples:
    | smoothing |
    | off       |
    | on        |

Scenario: The view does not pass the pitch limit
  Given the character looks straight up at the pitch limit
  When the mouse keeps moving up and a frame renders
  Then the view stays at the pitch limit
  And after the next tick the simulation's pitch is at the limit

Scenario: Stick look turns between ticks
  Given the right stick is held fully right
  When frames render between two ticks
  Then each frame's yaw is further right than the frame before

Scenario: A paused or unfocused game does not turn
  Given the game is paused or the window lost focus
  When the mouse moves and frames render
  Then the view does not turn
```

## Tech spec

## Verification
- At 144 Hz, sweep the mouse steadily. The view moves smoothly with no stepping at 60 Hz.
- Sweep and stop abruptly. The view never snaps back or jumps forward.
- Repeat in the strategy first-person free-cam.

## Notes
- Takes the smoothing state from ENG-F28-01, so it lands after it.
