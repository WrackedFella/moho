# The simulation pauses and runs at 1x, 2x or 3x

**Feature:** ENG-F11
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary
A game pauses its simulation and runs it at 1x, 2x or 3x on the fixed 60 Hz tick ([ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md)), while frames keep drawing. Engine request #307; consumer: the first SG-F4 (#310) card after its skeleton (the player pauses to give orders and speeds up waiting). The FPS can use pause too.

## Deliverables
- Pause, resume and a speed of 1x, 2x or 3x, set by the game.
- Speed changes how many ticks run per second of real time, never the tick length.
- Orders given while paused take effect without time advancing.

## Acceptance criteria
```gherkin
Scenario: A paused simulation does not advance
  Given the simulation is running
  When the game pauses it
  Then no tick advances simulation time while frames keep drawing

Scenario: An order given while paused takes effect at once
  Given the simulation is paused
  When the player gives an order
  Then the order's result is visible in the simulation state
  And simulation time has not advanced

Scenario Outline: Speed sets ticks per second of real time
  Given the simulation runs at <speed>
  When one second of real time passes on a machine that keeps up
  Then <ticks> ticks run
  Examples:
    | speed | ticks |
    | 1x    | 60    |
    | 2x    | 120   |
    | 3x    | 180   |

Scenario: Speed does not change results
  Given the same starting state and the same orders on the same ticks
  When one run plays at 1x and another at 3x
  Then both reach the same state at the same tick

Scenario: A slow machine slows the game rather than freezing it
  Given the simulation runs at 3x
  When frames take longer than the ticks they owe can run
  Then each frame runs at most a capped number of ticks
  And frames keep drawing

Scenario: The menu pause leaves the chosen speed alone
  Given the simulation runs at 2x
  When the player opens and closes the pause menu
  Then the simulation runs at 2x again
```

## Notes
- Out: the pause and speed controls and their bindings (strategy line, SG-F4's card).
- Applying orders while paused without advancing time is a direction decision for the Tech Lead to keep replay-safe ([ADR-0011](../../adr/0011-simulation-stays-network-ready.md)).
